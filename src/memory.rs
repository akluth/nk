use core::cell::UnsafeCell;

use crate::{limine::KernelAddress, serial};

const PAGE_SIZE: u64 = 4096;
const PAGE_ENTRIES: usize = 512;
const TABLE_COUNT: usize = 16;
const KERNEL_MAPPED_PAGES: usize = 512;
pub const USER_CODE_VIRT: u64 = 0x0000_4000_0000;
pub const USER_STACK_TOP: u64 = 0x0000_4000_3000;

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

#[repr(align(4096))]
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

static mut USER_CODE_PAGE: UserPage = UserPage::empty();
static mut USER_STACK_PAGE: UserPage = UserPage::empty();

pub fn create_user_address_space(kernel: KernelAddress) -> Option<PageTableRoot> {
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
        let kernel_pt = 6;

        link_table(tables, kernel, pml4, 0, pdpt, PTE_USER);
        link_table(tables, kernel, pdpt, 1, user_pd, PTE_USER);
        link_table(tables, kernel, user_pd, 0, user_pt, PTE_USER);
        let user_code_phys = page_phys(core::ptr::addr_of!(USER_CODE_PAGE), kernel);
        let user_stack_phys = page_phys(core::ptr::addr_of!(USER_STACK_PAGE), kernel);

        map_page(tables, user_pt, 0, user_code_phys, PTE_USER);
        map_page(
            tables,
            user_pt,
            2,
            user_stack_phys,
            PTE_USER | PTE_NO_EXECUTE,
        );

        let (pml4_index, pdpt_index, pd_index, pt_index) = page_indexes(kernel.virtual_base);
        link_table(tables, kernel, pml4, pml4_index, kernel_pdpt, 0);
        link_table(tables, kernel, kernel_pdpt, pdpt_index, kernel_pd, 0);
        link_table(tables, kernel, kernel_pd, pd_index, kernel_pt, 0);

        for page in 0..KERNEL_MAPPED_PAGES {
            map_page(
                tables,
                kernel_pt,
                pt_index + page,
                kernel.physical_base + (page as u64 * PAGE_SIZE),
                0,
            );
        }

        let _ = PAGE_SIZE;
        serial::write_line("nk: user page tables created");
        Some(PageTableRoot {
            pml4_phys: table_phys(tables, kernel, pml4),
        })
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

unsafe fn table_phys(tables: &[PageTable; TABLE_COUNT], kernel: KernelAddress, index: usize) -> u64 {
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

pub fn install_user_code(code: &[u8]) -> Option<(u64, u64)> {
    if code.len() > PAGE_SIZE as usize {
        return None;
    }

    unsafe {
        let page = &mut *core::ptr::addr_of_mut!(USER_CODE_PAGE);
        for byte in &mut page.bytes {
            *byte = 0x90;
        }
        page.bytes[..code.len()].copy_from_slice(code);
        Some((USER_CODE_VIRT, USER_STACK_TOP))
    }
}
