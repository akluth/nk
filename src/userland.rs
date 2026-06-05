use core::{
    arch::{asm, global_asm},
    cell::UnsafeCell,
};

use crate::{
    gdt,
    memory::{self, PageTableRoot},
    serial,
};

pub type VirtAddr = u64;
pub type PhysAddr = u64;

#[derive(Clone, Copy)]
pub enum Syscall {
    Yield = 0,
}

#[derive(Clone, Copy)]
pub struct Mapping {
    pub virt: VirtAddr,
    pub phys: PhysAddr,
    pub len: u64,
    pub flags: MappingFlags,
}

#[derive(Clone, Copy)]
pub struct MappingFlags {
    bits: u64,
}

impl MappingFlags {
    pub const READ: Self = Self { bits: 1 << 0 };
    pub const WRITE: Self = Self { bits: 1 << 1 };
    pub const EXECUTE: Self = Self { bits: 1 << 2 };
    pub const USER: Self = Self { bits: 1 << 3 };

    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    pub const fn bits(self) -> u64 {
        self.bits
    }
}

pub struct AddressSpace {
    mappings: [Option<Mapping>; 16],
    root: Option<PageTableRoot>,
    entry: VirtAddr,
    stack_top: VirtAddr,
}

impl AddressSpace {
    pub const fn new() -> Self {
        Self {
            mappings: [None; 16],
            root: None,
            entry: 0,
            stack_top: 0,
        }
    }

    pub fn map(&mut self, mapping: Mapping) -> bool {
        for slot in &mut self.mappings {
            if slot.is_none() {
                *slot = Some(mapping);
                return true;
            }
        }

        false
    }

    pub fn validation_token(&self) -> u64 {
        let mut token = 0;

        for mapping in &self.mappings {
            if let Some(mapping) = mapping {
                token ^= mapping.virt;
                token ^= mapping.phys;
                token ^= mapping.len;
                token ^= mapping.flags.bits();
            }
        }

        if let Some(root) = self.root {
            token ^= root.pml4_phys();
        }

        token
    }

    pub fn install_root(&mut self, root: PageTableRoot) {
        self.root = Some(root);
    }

    pub fn install_task(&mut self, entry: VirtAddr, stack_top: VirtAddr) {
        self.entry = entry;
        self.stack_top = stack_top;
    }

    pub fn root(&self) -> Option<PageTableRoot> {
        self.root
    }

    pub fn entry(&self) -> VirtAddr {
        self.entry
    }

    pub fn stack_top(&self) -> VirtAddr {
        self.stack_top
    }
}

struct GlobalAddressSpace(UnsafeCell<AddressSpace>);

unsafe impl Sync for GlobalAddressSpace {}

static USER_ADDRESS_SPACE: GlobalAddressSpace =
    GlobalAddressSpace(UnsafeCell::new(AddressSpace::new()));

extern "C" {
    fn enter_ring3(pml4: u64, entry: u64, stack: u64, code: u16, data: u16, kernel_stack: u64) -> !;
}

global_asm!(
    r#"
    .global enter_ring3
enter_ring3:
    mov rsp, r9
    mov cr3, rdi
    push r8
    push rdx
    pushfq
    pop rax
    or rax, 0x200
    push rax
    push rcx
    push rsi
    iretq
"#
);

pub fn init() {
    let address_space = unsafe { &mut *USER_ADDRESS_SPACE.0.get() };
    let mapped = address_space.map(Mapping {
        virt: 0x0000_4000_0000,
        phys: 0,
        len: 0x1000,
        flags: MappingFlags::READ
            .union(MappingFlags::WRITE)
            .union(MappingFlags::EXECUTE)
            .union(MappingFlags::USER),
    });

    if mapped && address_space.validation_token() != 0 {
        serial::write_line("nk: user address-space model ready");
    }
}

pub fn install_page_table_root(root: PageTableRoot) {
    unsafe {
        (*USER_ADDRESS_SPACE.0.get()).install_root(root);
    }
    let (user_code, user_data) = gdt::user_selectors();
    let _ = (user_code, user_data);
    serial::write_line("nk: user page-table root installed");
}

pub fn install_first_task() {
    const USER_CODE: &[u8] = &[
        0xb8, 0x00, 0x00, 0x00, 0x00, // mov eax, 0
        0xcd, 0x80, // int 0x80
        0xeb, 0xfe, // jmp $
    ];

    if let Some((entry, stack_top)) = memory::install_user_code(USER_CODE) {
        unsafe {
            (*USER_ADDRESS_SPACE.0.get()).install_task(entry, stack_top);
        }
        serial::write_line("nk: first ring3 task installed");
    }
}

pub fn start_first_task() -> ! {
    let address_space = unsafe { &mut *USER_ADDRESS_SPACE.0.get() };
    let root = address_space.root().expect("user page-table root missing");
    let (code, data) = gdt::user_selectors();

    serial::write_line("nk: entering ring3");
    unsafe {
        enter_ring3(
            root.pml4_phys(),
            address_space.entry(),
            address_space.stack_top(),
            code,
            data,
            gdt::kernel_stack_top(),
        );
    }
}

pub fn smoke_test_syscall() {
    unsafe {
        asm!(
            "int 0x80",
            in("rax") Syscall::Yield as u64,
            options(nostack, preserves_flags)
        );
    }
}
