#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_GUI_CLEAR: u64 = 16;
const SYS_GUI_RECT: u64 = 17;
const SYS_GUI_TEXT: u64 = 18;

const BG: u32 = 0x00191d24;
const PANEL: u32 = 0x00282f3a;
const ACCENT: u32 = 0x0000b894;
#[no_mangle]
pub extern "C" fn _start() -> ! {
    draw_background();
    loop {
        for _ in 0..120 {
            syscall0(SYS_YIELD);
        }
    }
}

fn draw_background() {
    syscall1(SYS_GUI_CLEAR, BG as u64);
    rect(0, 0, 1280, 36, PANEL);
    rect(18, 10, 16, 16, ACCENT);
    rect(46, 14, 160, 8, 0x00aab2bd);
    text(64, 72, b"nk desktop");
}

fn rect(x: u64, y: u64, width: u64, height: u64, color: u32) {
    syscall5(SYS_GUI_RECT, x, y, width, height, color as u64);
}

fn text(x: u64, y: u64, bytes: &'static [u8]) {
    syscall4(SYS_GUI_TEXT, x, y, bytes.as_ptr() as u64, bytes.len() as u64);
}

fn syscall0(id: u64) -> u64 {
    let out;
    unsafe {
        asm!("int 0x80", inlateout("rax") id => out, options(nostack, preserves_flags));
    }
    out
}

fn syscall1(id: u64, a: u64) -> u64 {
    let out;
    unsafe {
        asm!(
            "int 0x80",
            inlateout("rax") id => out,
            in("rdi") a,
            options(nostack, preserves_flags),
        );
    }
    out
}

fn syscall4(id: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let out;
    unsafe {
        asm!(
            "int 0x80",
            inlateout("rax") id => out,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            in("r10") d,
            options(nostack, preserves_flags),
        );
    }
    out
}

fn syscall5(id: u64, a: u64, b: u64, c: u64, d: u64, e: u64) -> u64 {
    let out;
    unsafe {
        asm!(
            "int 0x80",
            inlateout("rax") id => out,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            in("r10") d,
            in("r8") e,
            options(nostack, preserves_flags),
        );
    }
    out
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        syscall0(SYS_YIELD);
    }
}
