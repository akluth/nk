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
const USER_STACK_LOW_SIZE: usize = 256 * 1024;
const USER_STACK_LOW_BASE: u64 = USER_STACK_BASE - USER_STACK_LOW_SIZE as u64;
const USER_POOL_PAGES: usize = (USER_IMAGE_PAGES + USER_STACK_PAGES) * USER_TASKS;
const OWNER_FREE: u8 = 0;

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
#[derive(Clone, Copy)]
struct UserPage {
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
struct UserPagePool {
    pages: [UserPage; USER_POOL_PAGES],
}

impl UserPagePool {
    const fn empty() -> Self {
        Self {
            pages: [UserPage::empty(); USER_POOL_PAGES],
        }
    }
}

static mut USER_POOL: UserPagePool = UserPagePool::empty();
static mut USER_PAGE_OWNERS: [u8; USER_POOL_PAGES] = [OWNER_FREE; USER_POOL_PAGES];
static mut USER_VIRT_TO_POOL: [[u16; USER_IMAGE_PAGES + USER_STACK_PAGES]; USER_TASKS] =
    [[u16::MAX; USER_IMAGE_PAGES + USER_STACK_PAGES]; USER_TASKS];
static mut KERNEL_ADDRESS: KernelAddress = KernelAddress {
    physical_base: 0,
    virtual_base: 0,
};

pub fn create_user_address_spaces(
    kernel: KernelAddress,
    framebuffer: Option<FramebufferMapping>,
) -> [Option<PageTableRoot>; USER_TASKS] {
    unsafe {
        KERNEL_ADDRESS = kernel;
        for index in 0..USER_POOL_PAGES {
            USER_PAGE_OWNERS[index] = OWNER_FREE;
        }
        for task in 0..USER_TASKS {
            for page in 0..USER_IMAGE_PAGES + USER_STACK_PAGES {
                USER_VIRT_TO_POOL[task][page] = u16::MAX;
            }
        }
    }
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

        let stack_page_base = ((USER_STACK_BASE - USER_IMAGE_BASE) / PAGE_SIZE) as usize;
        link_table(
            tables,
            kernel,
            user_pd,
            stack_page_base / PAGE_ENTRIES,
            stack_pt,
            PTE_USER,
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
        free_task_pages(index);
        clear_user_mappings(index);
    }
    allocate_user_range(index, USER_STACK_LOW_BASE, USER_STACK_LOW_SIZE, true)
        && allocate_user_range(index, USER_STACK_BASE, USER_STACK_SIZE, true)
}

unsafe fn free_task_pages(index: usize) {
    let owner = (index as u8) + 1;
    for page_index in 0..USER_POOL_PAGES {
        if USER_PAGE_OWNERS[page_index] == owner {
            USER_PAGE_OWNERS[page_index] = OWNER_FREE;
            for byte in &mut USER_POOL.pages[page_index].bytes {
                *byte = 0;
            }
        }
    }
    for page in 0..USER_IMAGE_PAGES + USER_STACK_PAGES {
        USER_VIRT_TO_POOL[index][page] = u16::MAX;
    }
}

unsafe fn clear_user_mappings(index: usize) {
    let tables = &mut *PAGE_TABLES.0.get();
    let table_base = index * TABLES_PER_TASK;
    let user_pts = table_base + 3;
    let stack_pt = user_pts + USER_IMAGE_PT_COUNT;

    for table in 0..USER_IMAGE_PT_COUNT {
        tables[user_pts + table].clear();
    }
    tables[stack_pt].clear();
}

unsafe fn alloc_pool_page(owner: usize) -> Option<usize> {
    let owner = (owner as u8) + 1;
    for page_index in 0..USER_POOL_PAGES {
        if USER_PAGE_OWNERS[page_index] == OWNER_FREE {
            USER_PAGE_OWNERS[page_index] = owner;
            for byte in &mut USER_POOL.pages[page_index].bytes {
                *byte = 0;
            }
            return Some(page_index);
        }
    }
    None
}

fn user_page_index(virt: u64) -> Option<usize> {
    if virt < USER_IMAGE_BASE || virt >= USER_STACK_BASE + USER_STACK_SIZE as u64 {
        return None;
    }
    let page = ((virt - USER_IMAGE_BASE) / PAGE_SIZE) as usize;
    if page < USER_IMAGE_PAGES + USER_STACK_PAGES {
        Some(page)
    } else {
        None
    }
}

unsafe fn map_user_pool_page(index: usize, virt_page: usize, pool_page: usize, extra_flags: u64) {
    let tables = &mut *PAGE_TABLES.0.get();
    let table_base = index * TABLES_PER_TASK;
    let user_pts = table_base + 3;
    let stack_pt = user_pts + USER_IMAGE_PT_COUNT;
    let (table, entry) = if virt_page < USER_IMAGE_PAGES {
        (user_pts + virt_page / PAGE_ENTRIES, virt_page % PAGE_ENTRIES)
    } else {
        let stack_page = virt_page - USER_IMAGE_PAGES;
        (stack_pt, stack_page)
    };
    let phys = virt_to_phys(
        core::ptr::addr_of!(USER_POOL.pages[pool_page]) as u64,
        KERNEL_ADDRESS,
    );
    map_page(tables, table, entry, phys, PTE_USER | extra_flags);
    USER_VIRT_TO_POOL[index][virt_page] = pool_page as u16;
}

unsafe fn ensure_user_page(index: usize, virt_page: usize, extra_flags: u64) -> Option<usize> {
    if index >= USER_TASKS || virt_page >= USER_IMAGE_PAGES + USER_STACK_PAGES {
        return None;
    }
    let existing = USER_VIRT_TO_POOL[index][virt_page];
    if existing != u16::MAX {
        return Some(existing as usize);
    }
    let pool_page = alloc_pool_page(index)?;
    map_user_pool_page(index, virt_page, pool_page, extra_flags);
    Some(pool_page)
}

pub fn allocate_user_range(index: usize, virt: u64, len: usize, no_execute: bool) -> bool {
    if index >= USER_TASKS || len == 0 {
        return false;
    }
    let Some(end) = virt.checked_add(len as u64) else {
        return false;
    };
    if virt < USER_IMAGE_BASE || end > USER_STACK_BASE + USER_STACK_SIZE as u64 {
        return false;
    }

    let first = virt & !(PAGE_SIZE - 1);
    let last = (end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let flags = if no_execute { PTE_NO_EXECUTE } else { 0 };
    let mut page = first;
    while page < last {
        unsafe {
            let Some(virt_page) = user_page_index(page) else {
                return false;
            };
            if ensure_user_page(index, virt_page, flags).is_none() {
                return false;
            }
        }
        page += PAGE_SIZE;
    }
    true
}

pub fn user_range_mapped(index: usize, virt: u64, len: usize) -> bool {
    if index >= USER_TASKS {
        return false;
    }
    if len == 0 {
        return true;
    }
    let Some(end) = virt.checked_add(len as u64) else {
        return false;
    };
    if virt < USER_IMAGE_BASE || end > USER_STACK_BASE + USER_STACK_SIZE as u64 {
        return false;
    }

    let mut page = virt & !(PAGE_SIZE - 1);
    let last = (end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    unsafe {
        while page < last {
            let Some(virt_page) = user_page_index(page) else {
                return false;
            };
            if USER_VIRT_TO_POOL[index][virt_page] == u16::MAX {
                return false;
            }
            page += PAGE_SIZE;
        }
    }
    true
}

pub fn copy_user_segment(index: usize, virt: u64, data: &[u8], mem_size: usize) -> bool {
    if index >= USER_TASKS || virt < USER_IMAGE_BASE || virt >= USER_STACK_BASE {
        return false;
    }

    let end_virt = virt.saturating_add(mem_size as u64);
    if end_virt > USER_STACK_BASE
        || data.len() > mem_size
        || ((virt - USER_IMAGE_BASE) as usize)
            .checked_add(mem_size)
            .map_or(true, |end| end > USER_IMAGE_SIZE)
    {
        return false;
    }

    unsafe {
        let mut relative = 0usize;
        while relative < mem_size {
            let target = virt + relative as u64;
            let Some(virt_page) = user_page_index(target) else {
                return false;
            };
            let Some(pool_page) = ensure_user_page(index, virt_page, 0) else {
                return false;
            };
            let page_offset = (target as usize) & (PAGE_SIZE as usize - 1);
            let count = (mem_size - relative).min(PAGE_SIZE as usize - page_offset);
            let copy_count = count.min(data.len().saturating_sub(relative));
            if copy_count > 0 {
                USER_POOL.pages[pool_page].bytes[page_offset..page_offset + copy_count]
                    .copy_from_slice(&data[relative..relative + copy_count]);
            }
            for byte in &mut USER_POOL.pages[pool_page].bytes
                [page_offset + copy_count..page_offset + count]
            {
                *byte = 0;
            }
            relative += count;
        }
    }

    true
}

pub fn copy_user_space(source: usize, target: usize) -> bool {
    if source >= USER_TASKS || target >= USER_TASKS {
        return false;
    }

    unsafe {
        free_task_pages(target);
        clear_user_mappings(target);
        for virt_page in 0..USER_IMAGE_PAGES + USER_STACK_PAGES {
            let source_pool = USER_VIRT_TO_POOL[source][virt_page];
            if source_pool == u16::MAX {
                continue;
            }
            let Some(target_pool) = alloc_pool_page(target) else {
                return false;
            };
            USER_POOL.pages[target_pool] = USER_POOL.pages[source_pool as usize];
            let flags = if virt_page >= USER_IMAGE_PAGES {
                PTE_NO_EXECUTE
            } else {
                0
            };
            map_user_pool_page(target, virt_page, target_pool, flags);
        }
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
        for (data_offset, byte) in data.iter().enumerate() {
            let stack_offset = offset + data_offset;
            let virt = USER_STACK_BASE + stack_offset as u64;
            let Some(virt_page) = user_page_index(virt) else {
                return false;
            };
            let Some(pool_page) = ensure_user_page(index, virt_page, PTE_NO_EXECUTE) else {
                return false;
            };
            let page_offset = stack_offset & (PAGE_SIZE as usize - 1);
            USER_POOL.pages[pool_page].bytes[page_offset] = *byte;
        }
    }

    true
}

pub fn clear_user_stack(index: usize) -> bool {
    unsafe {
        if index >= USER_TASKS {
            return false;
        }
        free_user_range(index, USER_STACK_LOW_BASE, USER_STACK_LOW_SIZE);
        for page in 0..USER_STACK_PAGES {
            let virt_page = USER_IMAGE_PAGES + page;
            let existing = USER_VIRT_TO_POOL[index][virt_page];
            if existing != u16::MAX {
                USER_PAGE_OWNERS[existing as usize] = OWNER_FREE;
                for byte in &mut USER_POOL.pages[existing as usize].bytes {
                    *byte = 0;
                }
                USER_VIRT_TO_POOL[index][virt_page] = u16::MAX;
            }
        }
        let tables = &mut *PAGE_TABLES.0.get();
        let stack_pt = index * TABLES_PER_TASK + 3 + USER_IMAGE_PT_COUNT;
        tables[stack_pt].clear();
    }
    allocate_user_range(index, USER_STACK_LOW_BASE, USER_STACK_LOW_SIZE, true)
        && allocate_user_range(index, USER_STACK_BASE, USER_STACK_SIZE, true)
}

unsafe fn free_user_range(index: usize, virt: u64, len: usize) {
    let Some(end) = virt.checked_add(len as u64) else {
        return;
    };
    let mut page = virt & !(PAGE_SIZE - 1);
    let last = (end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    while page < last {
        if let Some(virt_page) = user_page_index(page) {
            let existing = USER_VIRT_TO_POOL[index][virt_page];
            if existing != u16::MAX {
                USER_PAGE_OWNERS[existing as usize] = OWNER_FREE;
                for byte in &mut USER_POOL.pages[existing as usize].bytes {
                    *byte = 0;
                }
                USER_VIRT_TO_POOL[index][virt_page] = u16::MAX;
                let tables = &mut *PAGE_TABLES.0.get();
                let user_pts = index * TABLES_PER_TASK + 3;
                let table = user_pts + virt_page / PAGE_ENTRIES;
                let entry = virt_page % PAGE_ENTRIES;
                tables[table].0[entry] = 0;
            }
        }
        page += PAGE_SIZE;
    }
}

pub const fn user_stack_top(_index: usize) -> u64 {
    USER_STACK_BASE + USER_STACK_SIZE as u64
}
