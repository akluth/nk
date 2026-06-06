use core::arch::asm;

#[inline(always)]
pub fn halt() {
    unsafe {
        asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

#[inline(always)]
pub fn enable_interrupts() {
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

#[inline(always)]
pub fn disable_interrupts() {
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

#[inline(always)]
pub unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn outw(port: u16, value: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

#[inline(always)]
pub unsafe fn inw(port: u16) -> u16 {
    let value: u16;
    asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

#[inline(always)]
pub unsafe fn outl(port: u16, value: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    asm!("in eax, dx", out("eax") value, in("dx") port, options(nomem, nostack, preserves_flags));
    value
}

#[inline(always)]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nomem, nostack, preserves_flags)
    );
    ((high as u64) << 32) | low as u64
}

#[inline(always)]
pub unsafe fn wrmsr(msr: u32, value: u64) {
    asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") value as u32,
        in("edx") (value >> 32) as u32,
        options(nomem, nostack, preserves_flags)
    );
}

#[inline(always)]
pub unsafe fn load_cr3(pml4_phys: u64) {
    asm!("mov cr3, {}", in(reg) pml4_phys, options(nostack, preserves_flags));
}

#[inline(always)]
pub unsafe fn read_cr3() -> u64 {
    let pml4_phys: u64;
    asm!("mov {}, cr3", out(reg) pml4_phys, options(nostack, preserves_flags));
    pml4_phys
}

pub unsafe fn enable_sse() {
    asm!(
        "mov rax, cr0",
        "and rax, ~(1 << 2)",
        "or rax, 1 << 1",
        "mov cr0, rax",
        "mov rax, cr4",
        "or rax, (1 << 9) | (1 << 10)",
        "mov cr4, rax",
        out("rax") _,
        options(nostack, preserves_flags)
    );
}
