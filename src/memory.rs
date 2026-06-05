use core::cell::UnsafeCell;

use crate::serial;

const PAGE_SIZE: u64 = 4096;
const PAGE_ENTRIES: usize = 512;
const TABLE_COUNT: usize = 16;

const PTE_PRESENT: u64 = 1 << 0;
const PTE_WRITABLE: u64 = 1 << 1;
const PTE_USER: u64 = 1 << 2;
const PTE_HUGE: u64 = 1 << 7;
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

pub fn create_user_address_space() -> Option<PageTableRoot> {
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

        link_table(tables, pml4, 0, pdpt, PTE_USER);
        link_table(tables, pdpt, 1, user_pd, PTE_USER);
        link_table(tables, user_pd, 0, user_pt, PTE_USER);
        map_page(tables, user_pt, 0, 0, PTE_USER | PTE_NO_EXECUTE);

        link_table(tables, pml4, 511, kernel_pdpt, 0);
        link_table(tables, kernel_pdpt, 510, kernel_pd, 0);
        map_huge_page(tables, kernel_pd, 0, 0, 0);

        let _ = PAGE_SIZE;
        serial::write_line("nk: user page tables created");
        Some(PageTableRoot {
            pml4_phys: table_phys(tables, pml4),
        })
    }
}

unsafe fn link_table(
    tables: &mut [PageTable; TABLE_COUNT],
    parent: usize,
    entry: usize,
    child: usize,
    extra_flags: u64,
) {
    tables[parent].0[entry] = table_phys(tables, child) | PTE_PRESENT | PTE_WRITABLE | extra_flags;
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

unsafe fn map_huge_page(
    tables: &mut [PageTable; TABLE_COUNT],
    table: usize,
    entry: usize,
    phys: u64,
    extra_flags: u64,
) {
    tables[table].0[entry] = phys | PTE_PRESENT | PTE_WRITABLE | PTE_HUGE | extra_flags;
}

unsafe fn table_phys(tables: &[PageTable; TABLE_COUNT], index: usize) -> u64 {
    tables[index].0.as_ptr() as u64
}
