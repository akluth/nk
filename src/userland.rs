use core::{
    arch::{asm, global_asm},
    cell::UnsafeCell,
};

use crate::{
    gdt,
    memory::{self, PageTableRoot},
    scheduler::{self, TrapFrame, UserAbi},
    serial,
};

pub type VirtAddr = u64;
pub type PhysAddr = u64;

#[derive(Clone, Copy)]
struct LoadSegment {
    file_offset: usize,
    virt: u64,
    file_size: usize,
}

impl LoadSegment {
    const fn empty() -> Self {
        Self {
            file_offset: 0,
            virt: 0,
            file_size: 0,
        }
    }
}

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
    roots: [Option<PageTableRoot>; scheduler::USER_TASKS],
    entry: VirtAddr,
    stack_top: VirtAddr,
}

impl AddressSpace {
    pub const fn new() -> Self {
        Self {
            mappings: [None; 16],
            roots: [None; scheduler::USER_TASKS],
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

        for root in &self.roots {
            if let Some(root) = root {
                token ^= root.pml4_phys();
            }
        }

        token
    }

    pub fn install_roots(&mut self, roots: [Option<PageTableRoot>; scheduler::USER_TASKS]) {
        self.roots = roots;
    }

    pub fn install_task(&mut self, entry: VirtAddr, stack_top: VirtAddr) {
        self.entry = entry;
        self.stack_top = stack_top;
    }

    pub fn root(&self, index: usize) -> Option<PageTableRoot> {
        if index >= scheduler::USER_TASKS {
            None
        } else {
            self.roots[index]
        }
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

pub fn install_page_table_roots(roots: [Option<PageTableRoot>; scheduler::USER_TASKS]) {
    unsafe {
        (*USER_ADDRESS_SPACE.0.get()).install_roots(roots);
    }
    let (user_code, user_data) = gdt::user_selectors();
    let _ = (user_code, user_data);
    serial::write_line("nk: user page-table roots installed");
}

pub fn install_first_task() {
    install_user_elf(0, "gui", UserAbi::Native, b"GUI     ELF");
    if !install_user_elf(1, "bash", UserAbi::Linux, b"BASH    ELF") {
        serial::write_line("nk: bash elf missing; no terminal process installed");
    }
    install_user_elf(2, "taskviewer", UserAbi::Native, b"TASKVIEWELF");
    serial::write_line("nk: coreutils elfs available on fat32 for on-demand exec");
}

pub fn task_pml4(index: usize) -> Option<u64> {
    unsafe { (*USER_ADDRESS_SPACE.0.get()).root(index).map(|root| root.pml4_phys()) }
}

fn install_user_elf(index: usize, name: &'static str, abi: UserAbi, fat_name: &[u8; 11]) -> bool {
    if let Some(image) = crate::fat32::read_file(fat_name) {
        if !memory::clear_user_image(index) {
            return false;
        }
        if let Some(entry) = load_elf(index, image) {
            let stack_top = if matches!(abi, UserAbi::Linux) {
                let args: [&[u8]; 4] = [name.as_bytes(), b"--noprofile", b"--norc", b"-i"];
                linux_stack_top(index, &args).unwrap_or_else(|| memory::user_stack_top(index))
            } else {
                memory::user_stack_top(index)
            };
            unsafe {
                (*USER_ADDRESS_SPACE.0.get()).install_task(entry, stack_top);
            }
            if let Some(root) = unsafe { (*USER_ADDRESS_SPACE.0.get()).root(index) } {
                install_task_frame(index, name, abi, root.pml4_phys(), entry, stack_top);
                serial::write_str("nk: ");
                serial::write_str(name);
                serial::write_line(" elf process installed");
                true
            } else {
                serial::write_str("nk: ");
                serial::write_str(name);
                serial::write_line(" page-table root missing");
                false
            }
        } else {
            serial::write_str("nk: ");
            serial::write_str(name);
            serial::write_line(" elf load failed");
            false
        }
    } else {
        serial::write_str("nk: ");
        serial::write_str(name);
        serial::write_line(" elf missing on fat32");
        false
    }
}

pub fn exec_linux_elf(
    index: usize,
    task_name: &'static str,
    fat_name: &[u8; 11],
    argv: &[&[u8]],
    frame: &mut TrapFrame,
) -> bool {
    let Some(image) = crate::fat32::read_file(fat_name) else {
        return false;
    };
    if !memory::clear_user_image(index) {
        return false;
    }
    let Some(entry) = load_elf(index, image) else {
        return false;
    };
    let Some(stack_top) = linux_stack_top(index, argv) else {
        return false;
    };

    let new_frame = new_task_frame(UserAbi::Linux, entry, stack_top);
    scheduler::replace_user_task_frame(index, task_name, UserAbi::Linux, new_frame);
    *frame = new_frame;
    true
}

fn linux_stack_top(index: usize, argv: &[&[u8]]) -> Option<VirtAddr> {
    const AT_NULL: u64 = 0;
    const AT_PAGESZ: u64 = 6;
    const AT_UID: u64 = 11;
    const AT_EUID: u64 = 12;
    const AT_GID: u64 = 13;
    const AT_EGID: u64 = 14;
    const AT_SECURE: u64 = 23;

    let stack_base = memory::user_stack_top(index) - memory::USER_STACK_SIZE as u64;
    let env = [b"TERM=vt100".as_slice(), b"PATH=/".as_slice()];

    if !memory::clear_user_stack(index) {
        return None;
    }

    let mut cursor = memory::USER_STACK_SIZE;
    let mut argv_addrs = [0u64; 8];
    if argv.len() > argv_addrs.len() {
        return None;
    }
    for (arg_index, arg) in argv.iter().enumerate().rev() {
        cursor = align_down(cursor.checked_sub(arg.len() + 1)?, 8);
        argv_addrs[arg_index] = stack_base + cursor as u64;
        if !memory::write_user_stack(index, cursor, arg)
            || !memory::write_user_stack(index, cursor + arg.len(), &[0])
        {
            return None;
        }
    }

    let mut env_addrs = [0u64; 2];
    for (env_index, env_value) in env.iter().enumerate().rev() {
        cursor = align_down(cursor.checked_sub(env_value.len() + 1)?, 8);
        env_addrs[env_index] = stack_base + cursor as u64;
        if !memory::write_user_stack(index, cursor, env_value)
            || !memory::write_user_stack(index, cursor + env_value.len(), &[0])
        {
            return None;
        }
    }

    cursor = align_down(cursor.checked_sub(16)?, 16);
    let random_addr = stack_base + cursor as u64;
    if !memory::write_user_stack(
        index,
        cursor,
        &[
            0x31, 0x41, 0x59, 0x26, 0x53, 0x58, 0x97, 0x93, 0x23, 0x84, 0x62, 0x64, 0x33, 0x83,
            0x27, 0x95,
        ],
    ) {
        return None;
    }

    let mut words = [0u64; 32];
    let mut word_count = 0usize;
    words[word_count] = argv.len() as u64;
    word_count += 1;
    for arg_addr in &argv_addrs[..argv.len()] {
        words[word_count] = *arg_addr;
        word_count += 1;
    }
    words[word_count] = 0;
    word_count += 1;
    for env_addr in env_addrs {
        words[word_count] = env_addr;
        word_count += 1;
    }
    words[word_count] = 0;
    word_count += 1;
    let auxv = [
        AT_PAGESZ,
        4096,
        AT_UID,
        0,
        AT_EUID,
        0,
        AT_GID,
        0,
        AT_EGID,
        0,
        AT_SECURE,
        0,
        25,
        random_addr,
        AT_NULL,
        0,
    ];
    for word in auxv {
        words[word_count] = word;
        word_count += 1;
    }

    cursor = align_down(cursor.checked_sub(word_count * 8)?, 16);
    for (word_index, word) in words[..word_count].iter().enumerate() {
        if !memory::write_user_stack(index, cursor + word_index * 8, &word.to_le_bytes()) {
            return None;
        }
    }

    Some(stack_base + cursor as u64)
}

const fn align_down(value: usize, align: usize) -> usize {
    value & !(align - 1)
}

pub fn start_first_task() -> ! {
    let pml4 = scheduler::first_user_pml4().expect("user page-table root missing");
    let frame = scheduler::first_user_frame().expect("ring3 frame missing");

    serial::write_line("nk: entering ring3");
    unsafe {
        enter_ring3_frame(pml4, &frame, gdt::kernel_stack_top());
    }
}

fn install_task_frame(
    index: usize,
    name: &'static str,
    abi: UserAbi,
    pml4_phys: u64,
    entry: VirtAddr,
    stack_top: VirtAddr,
) {
    let frame = new_task_frame(abi, entry, stack_top);
    scheduler::install_user_task(index, name, abi, pml4_phys, frame);
}

fn new_task_frame(abi: UserAbi, entry: VirtAddr, stack_top: VirtAddr) -> TrapFrame {
    let (code, data) = gdt::user_selectors();
    let _ = abi;
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
    }
}

fn load_elf(task_index: usize, image: &[u8]) -> Option<u64> {
    if image.len() < 64 || &image[0..4] != b"\x7fELF" {
        return None;
    }
    if image[4] != 2 || image[5] != 1 {
        return None;
    }
    let elf_type = read_u16(image, 16)?;
    if elf_type != 2 && elf_type != 3 {
        return None;
    }
    if read_u16(image, 18)? != 0x3e {
        return None;
    }

    let load_bias = if elf_type == 3 {
        memory::USER_IMAGE_BASE
    } else {
        0
    };
    let entry = read_u64(image, 24)?.checked_add(load_bias)?;
    let phoff = read_u64(image, 32)? as usize;
    let phentsize = read_u16(image, 54)? as usize;
    let phnum = read_u16(image, 56)? as usize;
    if phentsize < 56 {
        return None;
    }

    let mut loads = [LoadSegment::empty(); 8];
    let mut load_count = 0usize;
    let mut dynamic_offset = 0usize;
    let mut dynamic_size = 0usize;
    for index in 0..phnum {
        let offset = phoff.checked_add(index.checked_mul(phentsize)?)?;
        if offset.checked_add(phentsize)? > image.len() {
            return None;
        }
        let ph_type = read_u32(image, offset)?;

        let file_offset = read_u64(image, offset + 8)? as usize;
        let raw_virt = read_u64(image, offset + 16)?;
        let file_size = read_u64(image, offset + 32)? as usize;
        let mem_size = read_u64(image, offset + 40)? as usize;
        let file_end = file_offset.checked_add(file_size)?;
        if file_end > image.len() {
            return None;
        }

        if ph_type == 1 {
            if load_count >= loads.len() {
                return None;
            }
            loads[load_count] = LoadSegment {
                file_offset,
                virt: raw_virt,
                file_size,
            };
            load_count += 1;

            let virt = raw_virt.checked_add(load_bias)?;
            if !memory::copy_user_segment(
                task_index,
                virt,
                &image[file_offset..file_end],
                mem_size,
            ) {
                return None;
            }
        } else if ph_type == 2 {
            dynamic_offset = file_offset;
            dynamic_size = file_size;
        }
    }

    if elf_type == 3
        && !apply_relative_relocations(
            task_index,
            image,
            load_bias,
            &loads[..load_count],
            dynamic_offset,
            dynamic_size,
        )
    {
        return None;
    }

    Some(entry)
}

fn apply_relative_relocations(
    task_index: usize,
    image: &[u8],
    load_bias: u64,
    loads: &[LoadSegment],
    dynamic_offset: usize,
    dynamic_size: usize,
) -> bool {
    const DT_NULL: u64 = 0;
    const DT_RELA: u64 = 7;
    const DT_RELASZ: u64 = 8;
    const DT_RELAENT: u64 = 9;
    const R_X86_64_RELATIVE: u64 = 8;

    if dynamic_size == 0 {
        return true;
    }

    let mut rela_addr = 0u64;
    let mut rela_size = 0usize;
    let mut rela_ent = 24usize;
    let mut offset = dynamic_offset;
    let dynamic_end = dynamic_offset.saturating_add(dynamic_size);
    while offset + 16 <= image.len() && offset < dynamic_end {
        let tag = match read_u64(image, offset) {
            Some(value) => value,
            None => return false,
        };
        let value = match read_u64(image, offset + 8) {
            Some(value) => value,
            None => return false,
        };
        if tag == DT_NULL {
            break;
        } else if tag == DT_RELA {
            rela_addr = value;
        } else if tag == DT_RELASZ {
            rela_size = value as usize;
        } else if tag == DT_RELAENT {
            rela_ent = value as usize;
        }
        offset += 16;
    }

    if rela_addr == 0 || rela_size == 0 || rela_ent < 24 {
        return true;
    }
    let Some(rela_file_offset) = virt_to_file_offset(loads, rela_addr) else {
        return false;
    };

    let count = rela_size / rela_ent;
    for index in 0..count {
        let offset = rela_file_offset + index * rela_ent;
        let Some(reloc_offset) = read_u64(image, offset) else {
            return false;
        };
        let Some(info) = read_u64(image, offset + 8) else {
            return false;
        };
        let Some(addend) = read_i64(image, offset + 16) else {
            return false;
        };
        if info & 0xffff_ffff != R_X86_64_RELATIVE {
            return false;
        }

        let target = match reloc_offset.checked_add(load_bias) {
            Some(value) => value,
            None => return false,
        };
        let value = (load_bias as i64).wrapping_add(addend) as u64;
        if !memory::copy_user_segment(task_index, target, &value.to_le_bytes(), 8) {
            return false;
        }
    }

    true
}

fn virt_to_file_offset(loads: &[LoadSegment], virt: u64) -> Option<usize> {
    for load in loads {
        let start = load.virt;
        let end = load.virt.checked_add(load.file_size as u64)?;
        if virt >= start && virt < end {
            return Some(load.file_offset + (virt - start) as usize);
        }
    }
    None
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

fn read_i64(bytes: &[u8], offset: usize) -> Option<i64> {
    let data = bytes.get(offset..offset + 8)?;
    Some(i64::from_le_bytes([
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
