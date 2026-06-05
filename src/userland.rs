use core::{
    arch::{asm, global_asm},
    cell::UnsafeCell,
};

use crate::{
    gdt,
    memory::{self, PageTableRoot},
    scheduler::{self, TrapFrame},
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

}

struct GlobalAddressSpace(UnsafeCell<AddressSpace>);

unsafe impl Sync for GlobalAddressSpace {}

static USER_ADDRESS_SPACE: GlobalAddressSpace =
    GlobalAddressSpace(UnsafeCell::new(AddressSpace::new()));

extern "C" {
    fn enter_ring3_frame(pml4: u64, frame: *const TrapFrame, kernel_stack: u64) -> !;
}

global_asm!(
    r#"
    .global enter_ring3_frame
enter_ring3_frame:
    mov rsp, rdx
    push rdi
    mov rax, [rsi + 112]
    push rax
    mov rax, [rsi + 152]
    push rax
    mov rax, [rsi + 144]
    push rax
    mov rax, [rsi + 136]
    push rax
    mov rax, [rsi + 128]
    push rax
    mov rax, [rsi + 120]
    push rax
    mov r15, [rsi + 0]
    mov r14, [rsi + 8]
    mov r13, [rsi + 16]
    mov r12, [rsi + 24]
    mov r11, [rsi + 32]
    mov r10, [rsi + 40]
    mov r9, [rsi + 48]
    mov r8, [rsi + 56]
    mov rdi, [rsi + 64]
    mov rbp, [rsi + 80]
    mov rbx, [rsi + 88]
    mov rdx, [rsi + 96]
    mov rcx, [rsi + 104]
    mov rsi, [rsi + 72]
    mov rax, [rsp + 48]
    mov cr3, rax
    mov rax, [rsp + 40]
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
    install_user_elf(0, "gui", b"GUI     ELF", true);
    install_user_elf(1, "shell", b"SHELL   ELF", false);
    install_user_elf(2, "taskviewer", b"TASKVIEWELF", false);
    install_user_elf(3, "cat", b"CAT     ELF", false);
}

fn install_user_elf(index: usize, name: &'static str, fat_name: &[u8; 11], clear_image: bool) {
    if let Some(image) = crate::fat32::read_file(fat_name) {
        if clear_image {
            memory::clear_user_image();
        }
        if let Some(entry) = load_elf(image) {
            let stack_top = memory::user_stack_top(index);
            unsafe {
                (*USER_ADDRESS_SPACE.0.get()).install_task(entry, stack_top);
            }
            install_task_frame(index, name, entry, stack_top);
            serial::write_str("nk: ");
            serial::write_str(name);
            serial::write_line(" elf process installed");
        } else {
            serial::write_str("nk: ");
            serial::write_str(name);
            serial::write_line(" elf load failed");
        }
    } else {
        serial::write_str("nk: ");
        serial::write_str(name);
        serial::write_line(" elf missing on fat32");
    }
}

pub fn start_first_task() -> ! {
    let address_space = unsafe { &mut *USER_ADDRESS_SPACE.0.get() };
    let root = address_space.root().expect("user page-table root missing");
    let frame = scheduler::first_user_frame().expect("ring3 frame missing");

    serial::write_line("nk: entering ring3");
    unsafe {
        enter_ring3_frame(
            root.pml4_phys(),
            &frame,
            gdt::kernel_stack_top(),
        );
    }
}

fn install_task_frame(index: usize, name: &'static str, entry: VirtAddr, stack_top: VirtAddr) {
    let (code, data) = gdt::user_selectors();
    scheduler::install_user_task(
        index,
        name,
        TrapFrame {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rdi: 0,
            rsi: 0,
            rbp: 0,
            rbx: 0,
            rdx: 0,
            rcx: 0,
            rax: 0,
            rip: entry,
            cs: code as u64,
            rflags: 0x202,
            rsp: stack_top,
            ss: data as u64,
        },
    );
}

fn load_elf(image: &[u8]) -> Option<u64> {
    if image.len() < 64 || &image[0..4] != b"\x7fELF" {
        return None;
    }
    if image[4] != 2 || image[5] != 1 || read_u16(image, 16)? != 2 {
        return None;
    }
    if read_u16(image, 18)? != 0x3e {
        return None;
    }

    let entry = read_u64(image, 24)?;
    let phoff = read_u64(image, 32)? as usize;
    let phentsize = read_u16(image, 54)? as usize;
    let phnum = read_u16(image, 56)? as usize;
    if phentsize < 56 {
        return None;
    }

    for index in 0..phnum {
        let offset = phoff.checked_add(index.checked_mul(phentsize)?)?;
        if offset.checked_add(phentsize)? > image.len() {
            return None;
        }
        if read_u32(image, offset)? != 1 {
            continue;
        }

        let file_offset = read_u64(image, offset + 8)? as usize;
        let virt = read_u64(image, offset + 16)?;
        let file_size = read_u64(image, offset + 32)? as usize;
        let mem_size = read_u64(image, offset + 40)? as usize;
        let file_end = file_offset.checked_add(file_size)?;
        if file_end > image.len() {
            return None;
        }
        if !memory::copy_user_segment(virt, &image[file_offset..file_end], mem_size) {
            return None;
        }
    }

    Some(entry)
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let data = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([data[0], data[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let data = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let data = bytes.get(offset..offset + 8)?;
    Some(u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]))
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
