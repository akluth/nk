use core::cell::UnsafeCell;

use crate::{limine::KernelAddress, serial};

const PAGE_SIZE: u64 = 4096;
const PAGE_ENTRIES: usize = 512;
const TABLE_COUNT: usize = 24;
const KERNEL_MAPPED_PAGES: usize = 4096;
const KERNEL_PT_COUNT: usize = (KERNEL_MAPPED_PAGES + PAGE_ENTRIES - 1) / PAGE_ENTRIES + 1;
pub const USER_IMAGE_BASE: u64 = 0x0000_0000_4000_0000;
pub const USER_IMAGE_SIZE: usize = 1536 * 1024;
pub const USER_IMAGE_PAGES: usize = USER_IMAGE_SIZE / PAGE_SIZE as usize;
pub const USER_STACK_BASE: u64 = 0x0000_0000_4018_0000;

const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
const PTE_USER: u64 = 1 << 2;
const PTE_NO_EXECUTE: u64 = 1 << 63;

#[repr(align(4096))]
#[derive(Clone, Copy)]
struct PageTable([u64; PAGE_ENTRIES]);

impl PageTable {
    const fn empty() -> Self {
        Self([0; PAGE_ENTRIES])
    }

    fn clear(&mut self) {
        self.0 = [0; PAGE_ENTRIES];
    }
}

struct PageTables(UnsafeCell<[PageTable; TABLE_COUNT]>);

unsafe impl Sync for PageTables {}

static PAGE_TABLES: PageTables = PageTables(UnsafeCell::new([PageTable::empty(); TABLE_COUNT]));

#[derive(Clone, Copy)]
pub struct PageTableRoot {
    pml4_phys: u64,
}

impl PageTableRoot {
    pub const fn pml4_phys(self) -> u64 {
        self.pml4_phys
    }
}

#[derive(Clone, Copy)]
pub struct FramebufferMapping {
    pub virt: u64,
    pub phys: u64,
    pub len: u64,
}

#[repr(align(4096))]
#[allow(dead_code)]
pub struct UserPage {
    bytes: [u8; PAGE_SIZE as usize],
}

impl UserPage {
    const fn empty() -> Self {
        Self {
            bytes: [0; PAGE_SIZE as usize],
        }
    }
}

#[repr(align(4096))]
pub struct UserImage {
    bytes: [u8; USER_IMAGE_SIZE],
}

impl UserImage {
    const fn empty() -> Self {
        Self {
            bytes: [0; USER_IMAGE_SIZE],
        }
    }
}

static mut USER_IMAGE: UserImage = UserImage::empty();
static mut USER_STACK_PAGE: UserPage = UserPage::empty();
static mut USER_STACK_PAGE_1: UserPage = UserPage::empty();
static mut USER_STACK_PAGE_2: UserPage = UserPage::empty();
static mut USER_STACK_PAGE_3: UserPage = UserPage::empty();

pub fn create_user_address_space(
    kernel: KernelAddress,
    framebuffer: Option<FramebufferMapping>,
) -> Option<PageTableRoot> {
    unsafe {
        let tables = &mut *PAGE_TABLES.0.get();
        for table in tables.iter_mut() {
            table.clear();
        }

        let pml4 = 0;
        let pdpt = 1;
        let user_pd = 2;
        let user_pt = 3;
        let kernel_pdpt = 4;
        let kernel_pd = 5;
        let kernel_pts = 6;
        let framebuffer_pdpt = 15;
        let framebuffer_pd = 16;
        let framebuffer_pts = 17;

        link_table(tables, kernel, pml4, 0, pdpt, PTE_USER);
        link_table(tables, kernel, pdpt, 1, user_pd, PTE_USER);
        link_table(tables, kernel, user_pd, 0, user_pt, PTE_USER);
        let user_stack_phys = page_phys(core::ptr::addr_of!(USER_STACK_PAGE), kernel);
        let user_stack_1_phys = page_phys(core::ptr::addr_of!(USER_STACK_PAGE_1), kernel);
        let user_stack_2_phys = page_phys(core::ptr::addr_of!(USER_STACK_PAGE_2), kernel);
        let user_stack_3_phys = page_phys(core::ptr::addr_of!(USER_STACK_PAGE_3), kernel);

        let user_image_phys = virt_to_phys(core::ptr::addr_of!(USER_IMAGE) as u64, kernel);
        for page in 0..USER_IMAGE_PAGES {
            map_page(
                tables,
                user_pt,
                page,
                user_image_phys + (page as u64 * PAGE_SIZE),
                PTE_USER,
            );
        }
        map_page(
            tables,
            user_pt,
            ((USER_STACK_BASE - USER_IMAGE_BASE) / PAGE_SIZE) as usize,
            user_stack_phys,
            PTE_USER | PTE_NO_EXECUTE,
        );
        map_page(
            tables,
            user_pt,
            ((USER_STACK_BASE - USER_IMAGE_BASE) / PAGE_SIZE) as usize + 1,
            user_stack_1_phys,
            PTE_USER | PTE_NO_EXECUTE,
        );
        map_page(
            tables,
            user_pt,
            ((USER_STACK_BASE - USER_IMAGE_BASE) / PAGE_SIZE) as usize + 2,
            user_stack_2_phys,
            PTE_USER | PTE_NO_EXECUTE,
        );
        map_page(
            tables,
            user_pt,
            ((USER_STACK_BASE - USER_IMAGE_BASE) / PAGE_SIZE) as usize + 3,
            user_stack_3_phys,
            PTE_USER | PTE_NO_EXECUTE,
        );

        let (pml4_index, pdpt_index, pd_index, pt_index) = page_indexes(kernel.virtual_base);
        link_table(tables, kernel, pml4, pml4_index, kernel_pdpt, 0);
        link_table(tables, kernel, kernel_pdpt, pdpt_index, kernel_pd, 0);
        for table in 0..KERNEL_PT_COUNT {
            link_table(
                tables,
                kernel,
                kernel_pd,
                pd_index + table,
                kernel_pts + table,
                0,
            );
        }

        for page in 0..KERNEL_MAPPED_PAGES {
            let table = kernel_pts + ((pt_index + page) / PAGE_ENTRIES);
            let entry = (pt_index + page) % PAGE_ENTRIES;
            map_page(
                tables,
                table,
                entry,
                kernel.physical_base + (page as u64 * PAGE_SIZE),
                0,
            );
        }

        if let Some(framebuffer) = framebuffer {
            map_range(
                tables,
                kernel,
                pml4,
                framebuffer_pdpt,
                framebuffer_pd,
                framebuffer_pts,
                framebuffer.virt,
                framebuffer.phys,
                framebuffer.len,
                PTE_NO_EXECUTE,
            );
        }

        let _ = PAGE_SIZE;
        serial::write_line("nk: user page tables created");
        Some(PageTableRoot {
            pml4_phys: table_phys(tables, kernel, pml4),
        })
    }
}

unsafe fn map_range(
    tables: &mut [PageTable; TABLE_COUNT],
    kernel: KernelAddress,
    pml4: usize,
    pdpt: usize,
    pd: usize,
    pts_start: usize,
    virt: u64,
    phys: u64,
    len: u64,
    extra_flags: u64,
) {
    let aligned_virt = virt & !(PAGE_SIZE - 1);
    let page_offset = virt - aligned_virt;
    let aligned_phys = phys - page_offset;
    let pages = ((len + page_offset + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
    let (pml4_index, pdpt_index, pd_index, pt_index) = page_indexes(aligned_virt);

    link_table(tables, kernel, pml4, pml4_index, pdpt, 0);
    link_table(tables, kernel, pdpt, pdpt_index, pd, 0);

    let pt_count = (pt_index + pages + PAGE_ENTRIES - 1) / PAGE_ENTRIES;
    for table in 0..pt_count {
        link_table(tables, kernel, pd, pd_index + table, pts_start + table, 0);
    }

    for page in 0..pages {
        let table = pts_start + ((pt_index + page) / PAGE_ENTRIES);
        let entry = (pt_index + page) % PAGE_ENTRIES;
        map_page(
            tables,
            table,
            entry,
            aligned_phys + (page as u64 * PAGE_SIZE),
            extra_flags,
        );
    }
}

unsafe fn link_table(
    tables: &mut [PageTable; TABLE_COUNT],
    kernel: KernelAddress,
    parent: usize,
    entry: usize,
    child: usize,
    extra_flags: u64,
) {
    tables[parent].0[entry] =
        table_phys(tables, kernel, child) | PTE_PRESENT | PTE_WRITABLE | extra_flags;
}

unsafe fn map_page(
    tables: &mut [PageTable; TABLE_COUNT],
    table: usize,
    entry: usize,
    phys: u64,
    extra_flags: u64,
) {
    tables[table].0[entry] = phys | PTE_PRESENT | PTE_WRITABLE | extra_flags;
}

unsafe fn table_phys(
    tables: &[PageTable; TABLE_COUNT],
    kernel: KernelAddress,
    index: usize,
) -> u64 {
    virt_to_phys(tables[index].0.as_ptr() as u64, kernel)
}

fn virt_to_phys(virt: u64, kernel: KernelAddress) -> u64 {
    virt - kernel.virtual_base + kernel.physical_base
}

fn page_phys(page: *const UserPage, kernel: KernelAddress) -> u64 {
    virt_to_phys(page as u64, kernel)
}

fn page_indexes(virt: u64) -> (usize, usize, usize, usize) {
    (
        ((virt >> 39) & 0x1ff) as usize,
        ((virt >> 30) & 0x1ff) as usize,
        ((virt >> 21) & 0x1ff) as usize,
        ((virt >> 12) & 0x1ff) as usize,
    )
}

pub fn clear_user_image() {
    unsafe {
        let image = &mut *core::ptr::addr_of_mut!(USER_IMAGE);
        for byte in &mut image.bytes {
            *byte = 0;
        }
    }
}

pub fn copy_user_segment(virt: u64, data: &[u8], mem_size: usize) -> bool {
    if virt < USER_IMAGE_BASE {
        return false;
    }

    let offset = (virt - USER_IMAGE_BASE) as usize;
    if offset
        .checked_add(mem_size)
        .map_or(true, |end| end > USER_IMAGE_SIZE)
        || data.len() > mem_size
    {
        return false;
    }

    unsafe {
        let image = &mut *core::ptr::addr_of_mut!(USER_IMAGE);
        image.bytes[offset..offset + data.len()].copy_from_slice(data);
        for byte in &mut image.bytes[offset + data.len()..offset + mem_size] {
            *byte = 0;
        }
    }

    true
}

pub const fn user_stack_top(index: usize) -> u64 {
    USER_STACK_BASE + PAGE_SIZE * (index as u64 + 1)
}
