use core::arch::global_asm;

use crate::{arch, gdt, scheduler, serial};

const IDT_ENTRIES: usize = 256;
const TIMER_VECTOR: u8 = 32;
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

extern "C" {
    fn isr_default();
    fn isr_timer();
    fn isr_syscall();
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
        idt.add(TIMER_VECTOR as usize)
            .write(IdtEntry::new(isr_timer));
        idt.add(SYSCALL_VECTOR as usize)
            .write(IdtEntry::new_user(isr_syscall));
        load_idt();
        remap_pic();
        configure_pit(TIMER_HZ);
        unmask_irq(0);
    }

    arch::enable_interrupts();
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
extern "C" fn rust_timer_interrupt() {
    unsafe {
        TIMER_TICKS = TIMER_TICKS.wrapping_add(1);
        let scheduler_ticks = scheduler::tick();
        if TIMER_TICKS == 1 || TIMER_TICKS % 100 == 0 {
            serial::write_line("nk: timer interrupt");
        }
        let _ = scheduler_ticks;
        send_eoi(0);
    }
}

#[no_mangle]
extern "C" fn rust_unhandled_interrupt() {
    serial::write_line("nk: unhandled interrupt");
}

#[no_mangle]
extern "C" fn rust_syscall_interrupt() {
    serial::write_line("nk: syscall boundary crossed");
}
