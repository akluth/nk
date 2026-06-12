use core::cell::UnsafeCell;

use crate::{limine::KernelAddress, scheduler::USER_TASKS, serial};

const PAGE_SIZE: u64 = 4096;
const PAGE_ENTRIES: usize = 512;
const USER_IMAGE_PT_COUNT: usize = (USER_IMAGE_PAGES + PAGE_ENTRIES - 1) / PAGE_ENTRIES;
const USER_STACK_PT_COUNT: usize = 1;
const TABLES_PER_TASK: usize = 320;
const TABLE_COUNT: usize = TABLES_PER_TASK * USER_TASKS;
const KERNEL_MAPPED_PAGES: usize = 131072;
const KERNEL_PT_COUNT: usize = (KERNEL_MAPPED_PAGES + PAGE_ENTRIES - 1) / PAGE_ENTRIES + 1;
pub const USER_IMAGE_BASE: u64 = 0x0000_0000_4000_0000;
pub const USER_IMAGE_SIZE: usize = 32 * 1024 * 1024;
pub const USER_IMAGE_PAGES: usize = USER_IMAGE_SIZE / PAGE_SIZE as usize;
pub const USER_STACK_BASE: u64 = 0x0000_0000_4200_0000;
pub const USER_STACK_SIZE: usize = 16 * 1024;
const USER_STACK_PAGES: usize = USER_STACK_SIZE / PAGE_SIZE as usize;

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
struct UserStacks {
    stacks: [UserStack; USER_TASKS],
}

impl UserStacks {
    const fn empty() -> Self {
        Self {
            stacks: [UserStack::empty(); USER_TASKS],
        }
    }
}

#[repr(align(4096))]
#[derive(Clone, Copy)]
struct UserStack {
    bytes: [u8; USER_STACK_SIZE],
}

impl UserStack {
    const fn empty() -> Self {
        Self {
            bytes: [0; USER_STACK_SIZE],
        }
    }
}

#[repr(align(4096))]
#[derive(Clone, Copy)]
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

#[repr(align(4096))]
struct UserImages {
    images: [UserImage; USER_TASKS],
}

impl UserImages {
    const fn empty() -> Self {
        Self {
            images: [UserImage::empty(); USER_TASKS],
        }
    }
}

static mut USER_IMAGES: UserImages = UserImages::empty();
static mut USER_STACKS: UserStacks = UserStacks::empty();

pub fn create_user_address_spaces(
    kernel: KernelAddress,
    framebuffer: Option<FramebufferMapping>,
) -> [Option<PageTableRoot>; USER_TASKS] {
    unsafe {
        let tables = &mut *PAGE_TABLES.0.get();
        for table in tables.iter_mut() {
            table.clear();
        }
    }

    [
        create_user_address_space(0, kernel, framebuffer),
        create_user_address_space(1, kernel, framebuffer),
        create_user_address_space(2, kernel, framebuffer),
        create_user_address_space(3, kernel, framebuffer),
    ]
}

fn create_user_address_space(
    task_index: usize,
    kernel: KernelAddress,
    framebuffer: Option<FramebufferMapping>,
) -> Option<PageTableRoot> {
    if task_index >= USER_TASKS {
        return None;
    }

    unsafe {
        let tables = &mut *PAGE_TABLES.0.get();

        let table_base = task_index * TABLES_PER_TASK;
        let pml4 = table_base;
        let pdpt = table_base + 1;
        let user_pd = table_base + 2;
        let user_pts = table_base + 3;
        let stack_pt = user_pts + USER_IMAGE_PT_COUNT;
        let kernel_pdpt = stack_pt + USER_STACK_PT_COUNT;
        let kernel_pd = kernel_pdpt + 1;
        let kernel_pts = kernel_pd + 1;
        let framebuffer_pdpt = kernel_pts + KERNEL_PT_COUNT;
        let framebuffer_pd = framebuffer_pdpt + 1;
        let framebuffer_pts = framebuffer_pd + 1;

        link_table(tables, kernel, pml4, 0, pdpt, PTE_USER);
        link_table(tables, kernel, pdpt, 1, user_pd, PTE_USER);
        for table in 0..USER_IMAGE_PT_COUNT {
            link_table(tables, kernel, user_pd, table, user_pts + table, PTE_USER);
        }

        let user_image_phys = virt_to_phys(
            core::ptr::addr_of!(USER_IMAGES.images[task_index]) as u64,
            kernel,
        );
        for page in 0..USER_IMAGE_PAGES {
            let table = user_pts + page / PAGE_ENTRIES;
            let entry = page % PAGE_ENTRIES;
            map_page(
                tables,
                table,
                entry,
                user_image_phys + (page as u64 * PAGE_SIZE),
                PTE_USER,
            );
        }

        let user_stack_phys = virt_to_phys(
            core::ptr::addr_of!(USER_STACKS.stacks[task_index]) as u64,
            kernel,
        );
        let stack_page_base = ((USER_STACK_BASE - USER_IMAGE_BASE) / PAGE_SIZE) as usize;
        link_table(
            tables,
            kernel,
            user_pd,
            stack_page_base / PAGE_ENTRIES,
            stack_pt,
            PTE_USER,
        );
        for page in 0..USER_STACK_PAGES {
            map_page(
                tables,
                stack_pt,
                (stack_page_base + page) % PAGE_ENTRIES,
                user_stack_phys + (page as u64 * PAGE_SIZE),
                PTE_USER | PTE_NO_EXECUTE,
            );
        }

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
        serial::write_str("nk: user page tables created for task ");
        serial::write_dec_u8(task_index as u8);
        serial::write_line("");
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

fn page_indexes(virt: u64) -> (usize, usize, usize, usize) {
    (
        ((virt >> 39) & 0x1ff) as usize,
        ((virt >> 30) & 0x1ff) as usize,
        ((virt >> 21) & 0x1ff) as usize,
        ((virt >> 12) & 0x1ff) as usize,
    )
}

pub fn clear_user_image(index: usize) -> bool {
    if index >= USER_TASKS {
        return false;
    }

    unsafe {
        let image = &mut (*core::ptr::addr_of_mut!(USER_IMAGES)).images[index];
        for byte in &mut image.bytes {
            *byte = 0;
        }
    }
    true
}

pub fn copy_user_segment(index: usize, virt: u64, data: &[u8], mem_size: usize) -> bool {
    if index >= USER_TASKS || virt < USER_IMAGE_BASE {
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
        let image = &mut (*core::ptr::addr_of_mut!(USER_IMAGES)).images[index];
        image.bytes[offset..offset + data.len()].copy_from_slice(data);
        for byte in &mut image.bytes[offset + data.len()..offset + mem_size] {
            *byte = 0;
        }
    }

    true
}

pub fn copy_user_space(source: usize, target: usize) -> bool {
    if source >= USER_TASKS || target >= USER_TASKS {
        return false;
    }

    unsafe {
        let images = &mut (*core::ptr::addr_of_mut!(USER_IMAGES)).images;
        let source_image = images[source];
        images[target] = source_image;

        let stacks = &mut (*core::ptr::addr_of_mut!(USER_STACKS)).stacks;
        let source_stack = stacks[source];
        stacks[target] = source_stack;
    }

    true
}

pub fn write_user_stack(index: usize, offset: usize, data: &[u8]) -> bool {
    if offset
        .checked_add(data.len())
        .map_or(true, |end| end > USER_STACK_SIZE)
    {
        return false;
    }

    unsafe {
        if index >= USER_TASKS {
            return false;
        }
        let page = &mut (*core::ptr::addr_of_mut!(USER_STACKS)).stacks[index];
        page.bytes[offset..offset + data.len()].copy_from_slice(data);
    }

    true
}

pub fn clear_user_stack(index: usize) -> bool {
    unsafe {
        if index >= USER_TASKS {
            return false;
        }
        let page = &mut (*core::ptr::addr_of_mut!(USER_STACKS)).stacks[index];
        for byte in &mut page.bytes {
            *byte = 0;
        }
    }
    true
}

pub const fn user_stack_top(_index: usize) -> u64 {
    USER_STACK_BASE + USER_STACK_SIZE as u64
}
