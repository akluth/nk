use core::arch::global_asm;

use crate::{
    arch, gdt, keyboard, linux_abi, mouse,
    scheduler::{self, UserAbi},
    serial, services,
};

const IDT_ENTRIES: usize = 256;
const GENERAL_PROTECTION_VECTOR: u8 = 13;
const PAGE_FAULT_VECTOR: u8 = 14;
const TIMER_VECTOR: u8 = 32;
const KEYBOARD_VECTOR: u8 = 33;
const MOUSE_VECTOR: u8 = 44;
const SYSCALL_VECTOR: u8 = 0x80;

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xa0;
const PIC2_DATA: u16 = 0xa1;
const PIC_EOI: u8 = 0x20;

const PIT_COMMAND: u16 = 0x43;
const PIT_CHANNEL0: u16 = 0x40;
const PIT_FREQUENCY: u32 = 1_193_182;
const TIMER_HZ: u32 = 100;
const IA32_EFER: u32 = 0xc000_0080;
const IA32_STAR: u32 = 0xc000_0081;
const IA32_LSTAR: u32 = 0xc000_0082;
const IA32_FMASK: u32 = 0xc000_0084;
const EFER_SYSCALL_ENABLE: u64 = 1;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    options: u16,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            options: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    fn new(handler: unsafe extern "C" fn()) -> Self {
        Self::with_options(handler, 0x8e00)
    }

    fn new_user(handler: unsafe extern "C" fn()) -> Self {
        Self::with_options(handler, 0xee00)
    }

    fn with_options(handler: unsafe extern "C" fn(), options: u16) -> Self {
        let address = handler as usize as u64;
        Self {
            offset_low: address as u16,
            selector: gdt::KERNEL_CODE_SELECTOR,
            options,
            offset_mid: (address >> 16) as u16,
            offset_high: (address >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::missing(); IDT_ENTRIES];
static mut TIMER_TICKS: u64 = 0;
static mut USER_SWITCH_LOGS: u64 = 0;

#[no_mangle]
static mut SYSCALL_STACK_TOP: u64 = 0;

#[no_mangle]
static mut SYSCALL_USER_RSP: u64 = 0;

extern "C" {
    fn isr_default();
    fn isr_general_protection();
    fn isr_page_fault();
    fn isr_timer();
    fn isr_keyboard();
    fn isr_mouse();
    fn isr_syscall();
    fn syscall_entry();
}

global_asm!(
    r#"
    .global isr_default
isr_default:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8
    cld
    lea rdi, [rsp + 8]
    call rust_unhandled_interrupt
    add rsp, 8
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    iretq

    .global isr_general_protection
isr_general_protection:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8
    cld
    mov rdi, 13
    mov rsi, [rsp + 128]
    mov rdx, [rsp + 136]
    xor rcx, rcx
    call rust_fatal_exception
1:
    hlt
    jmp 1b

    .global isr_page_fault
isr_page_fault:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8
    cld
    mov rdi, 14
    mov rsi, [rsp + 128]
    mov rdx, [rsp + 136]
    mov rcx, cr2
    call rust_fatal_exception
1:
    hlt
    jmp 1b

    .global isr_timer
isr_timer:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8
    cld
    lea rdi, [rsp + 8]
    call rust_timer_interrupt
    add rsp, 8
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    iretq

    .global isr_keyboard
isr_keyboard:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8
    cld
    call rust_keyboard_interrupt
    add rsp, 8
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    iretq

    .global isr_mouse
isr_mouse:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8
    cld
    call rust_mouse_interrupt
    add rsp, 8
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    iretq

    .global isr_syscall
isr_syscall:
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8
    cld
    lea rdi, [rsp + 8]
    call rust_syscall_interrupt
    add rsp, 8
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    iretq

    .global syscall_entry
syscall_entry:
    mov [rip + SYSCALL_USER_RSP], rsp
    mov rsp, [rip + SYSCALL_STACK_TOP]
    push 0x1b
    push qword ptr [rip + SYSCALL_USER_RSP]
    push r11
    push 0x23
    push rcx
    push rax
    push rcx
    push rdx
    push rbx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8
    cld
    lea rdi, [rsp + 8]
    call rust_syscall_interrupt
    add rsp, 8
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rbx
    pop rdx
    pop rcx
    pop rax
    iretq
"#
);

pub fn init() {
    arch::disable_interrupts();

    unsafe {
        let idt = core::ptr::addr_of_mut!(IDT) as *mut IdtEntry;
        for index in 0..IDT_ENTRIES {
            idt.add(index).write(IdtEntry::new(isr_default));
        }
        idt.add(GENERAL_PROTECTION_VECTOR as usize)
            .write(IdtEntry::new(isr_general_protection));
        idt.add(PAGE_FAULT_VECTOR as usize)
            .write(IdtEntry::new(isr_page_fault));
        idt.add(TIMER_VECTOR as usize)
            .write(IdtEntry::new(isr_timer));
        idt.add(KEYBOARD_VECTOR as usize)
            .write(IdtEntry::new(isr_keyboard));
        idt.add(MOUSE_VECTOR as usize)
            .write(IdtEntry::new(isr_mouse));
        idt.add(SYSCALL_VECTOR as usize)
            .write(IdtEntry::new_user(isr_syscall));
        load_idt();
        remap_pic();
        configure_pit(TIMER_HZ);
        configure_syscall_instruction();
        mouse::init();
        unmask_irq(0);
        unmask_irq(1);
        unmask_irq(2);
        unmask_irq(12);
    }

    arch::enable_interrupts();
}

unsafe fn configure_syscall_instruction() {
    SYSCALL_STACK_TOP = gdt::kernel_stack_top();
    let efer = arch::rdmsr(IA32_EFER);
    arch::wrmsr(IA32_EFER, efer | EFER_SYSCALL_ENABLE);

    let kernel_code = gdt::KERNEL_CODE_SELECTOR as u64;
    let user_star = (gdt::USER_DATA_SELECTOR as u64).wrapping_sub(8);
    arch::wrmsr(IA32_STAR, (user_star << 48) | (kernel_code << 32));
    arch::wrmsr(IA32_LSTAR, syscall_entry as *const () as usize as u64);
    arch::wrmsr(IA32_FMASK, 0x200);
    serial::write_line("nk: syscall instruction enabled");
}

unsafe fn load_idt() {
    let pointer = IdtPointer {
        limit: (core::mem::size_of::<[IdtEntry; IDT_ENTRIES]>() - 1) as u16,
        base: core::ptr::addr_of!(IDT) as u64,
    };

    core::arch::asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
}

unsafe fn remap_pic() {
    let mask1 = arch::inb(PIC1_DATA);
    let mask2 = arch::inb(PIC2_DATA);

    arch::outb(PIC1_COMMAND, 0x11);
    io_wait();
    arch::outb(PIC2_COMMAND, 0x11);
    io_wait();

    arch::outb(PIC1_DATA, 0x20);
    io_wait();
    arch::outb(PIC2_DATA, 0x28);
    io_wait();

    arch::outb(PIC1_DATA, 0x04);
    io_wait();
    arch::outb(PIC2_DATA, 0x02);
    io_wait();

    arch::outb(PIC1_DATA, 0x01);
    io_wait();
    arch::outb(PIC2_DATA, 0x01);
    io_wait();

    arch::outb(PIC1_DATA, mask1);
    arch::outb(PIC2_DATA, mask2);
}

unsafe fn configure_pit(hz: u32) {
    let divisor = (PIT_FREQUENCY / hz) as u16;
    arch::outb(PIT_COMMAND, 0x36);
    arch::outb(PIT_CHANNEL0, divisor as u8);
    arch::outb(PIT_CHANNEL0, (divisor >> 8) as u8);
}

unsafe fn unmask_irq(irq: u8) {
    let port = if irq < 8 { PIC1_DATA } else { PIC2_DATA };
    let irq_line = if irq < 8 { irq } else { irq - 8 };
    let mask = arch::inb(port) & !(1 << irq_line);
    arch::outb(port, mask);
}

unsafe fn send_eoi(irq: u8) {
    if irq >= 8 {
        arch::outb(PIC2_COMMAND, PIC_EOI);
    }
    arch::outb(PIC1_COMMAND, PIC_EOI);
}

unsafe fn io_wait() {
    arch::outb(0x80, 0);
}

#[no_mangle]
extern "C" fn rust_timer_interrupt(frame: *mut scheduler::TrapFrame) {
    unsafe {
        TIMER_TICKS = TIMER_TICKS.wrapping_add(1);
        let scheduler_ticks = scheduler::tick();
        if TIMER_TICKS == 1 || TIMER_TICKS % 100 == 0 {
            serial::write_line("nk: timer interrupt");
        }
        if let Some(task_switch) = scheduler::schedule_user(&mut *frame) {
            arch::load_cr3(task_switch.pml4_phys);
            USER_SWITCH_LOGS = USER_SWITCH_LOGS.wrapping_add(1);
            if USER_SWITCH_LOGS > 16 && USER_SWITCH_LOGS % 100 != 0 {
                send_eoi(0);
                return;
            }
            serial::write_str("nk: switched to ");
            serial::write_line(task_switch.name);
        }
        let _ = scheduler_ticks;
        send_eoi(0);
    }
}

#[no_mangle]
extern "C" fn rust_keyboard_interrupt() {
    let scancode = unsafe { arch::inb(0x60) };
    keyboard::push_scancode(scancode);
    unsafe {
        send_eoi(1);
    }
}

#[no_mangle]
extern "C" fn rust_mouse_interrupt() {
    let byte = unsafe { arch::inb(0x60) };
    mouse::push_byte(byte);
    unsafe {
        send_eoi(12);
    }
}

#[no_mangle]
extern "C" fn rust_unhandled_interrupt(frame: *mut scheduler::TrapFrame) {
    let frame = unsafe { &*frame };
    serial::write_str("nk: unhandled interrupt rip=");
    serial::write_hex_u64(frame.rip);
    serial::write_str(" cs=");
    serial::write_hex_u64(frame.cs);
    serial::write_str(" rax=");
    serial::write_hex_u64(frame.rax);
    serial::write_line("");
    loop {
        arch::halt();
    }
}

#[no_mangle]
extern "C" fn rust_syscall_interrupt(frame: *mut scheduler::TrapFrame) {
    let frame = unsafe { &mut *frame };
    if matches!(scheduler::current_user_abi(), Some(UserAbi::Linux))
        && linux_abi::handle_syscall(frame)
    {
        return;
    }

    match frame.rax {
        0 => {}
        16 => services::gui::clear(frame.rdi as u32),
        17 => services::gui::rect(
            frame.rdi as usize,
            frame.rsi as usize,
            frame.rdx as usize,
            frame.r10 as usize,
            frame.r8 as u32,
        ),
        18 => services::gui::text(
            frame.rdi as usize,
            frame.rsi as usize,
            frame.rdx as *const u8,
            frame.r10 as usize,
            0x001a202c,
        ),
        19 => {
            frame.rax = keyboard::pop_key().unwrap_or(0) as u64;
            return;
        }
        20 => {
            frame.rax = mouse::packed_state();
            return;
        }
        21 => services::gui::text(
            frame.rdi as usize,
            frame.rsi as usize,
            frame.rdx as *const u8,
            frame.r10 as usize,
            frame.r8 as u32,
        ),
        22 => {
            frame.rax = scheduler::user_task_count() as u64;
            return;
        }
        23 => {
            frame.rax = packed_task_info(frame.rdi as usize);
            return;
        }
        24 => {
            services::gui::reset_console();
            frame.rax = if scheduler::restart_user_task(frame.rdi as usize) {
                0
            } else {
                1
            };
            return;
        }
        25 => {
            scheduler::set_focus(frame.rdi as usize);
            frame.rax = 0;
            return;
        }
        26 => {
            frame.rax = scheduler::focus() as u64;
            return;
        }
        32 => unsafe {
            serial::write_line("nk: shutdown requested");
            arch::outw(0x604, 0x2000);
            arch::outw(0xb004, 0x2000);
        },
        id => {
            serial::write_str("nk: unknown syscall id=");
            serial::write_dec_u8(id as u8);
            serial::write_line("");
        }
    }
    frame.rax = 0;
}

fn packed_task_info(index: usize) -> u64 {
    let Some(info) = scheduler::user_task_info(index) else {
        return 0;
    };
    let name_id = index as u64 + 1;
    let mut flags = 0u64;
    if info.active {
        flags |= 1;
    }
    if info.current {
        flags |= 2;
    }

    name_id | (flags << 8) | ((info.ticks & 0x0000_ffff_ffff) << 16)
}

#[no_mangle]
extern "C" fn rust_fatal_exception(vector: u64, error: u64, rip: u64, address: u64) -> ! {
    serial::write_str("nk: fatal exception vector=");
    serial::write_dec_u8(vector as u8);
    serial::write_str(" error=");
    serial::write_hex_u64(error);
    serial::write_str(" rip=");
    serial::write_hex_u64(rip);
    serial::write_str(" addr=");
    serial::write_hex_u64(address);
    serial::write_line("");
    loop {
        arch::halt();
    }
}
