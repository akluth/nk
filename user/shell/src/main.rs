#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_GUI_RECT: u64 = 17;
const SYS_GUI_TEXT: u64 = 18;

const SHADOW: u32 = 0x000d1117;
const WINDOW: u32 = 0x00111117;
const TITLE: u32 = 0x00343d4a;
const PROMPT: u32 = 0x0000b894;
const LINE: u32 = 0x00d0d7de;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    loop {
        draw_shell();
        for _ in 0..60 {
            syscall0(SYS_YIELD);
        }
    }
}

fn draw_shell() {
    rect(168, 356, 520, 214, SHADOW);
    rect(160, 348, 520, 214, WINDOW);
    rect(160, 348, 520, 30, TITLE);
    rect(176, 360, 10, 10, 0x00ff605c);
    rect(194, 360, 10, 10, 0x00ffbd44);
    rect(212, 360, 10, 10, 0x0000ca4e);
    text(188, 402, b"> version");
    text(188, 430, b"nk 0.1.0");
    text(188, 470, b"> shutdown");
    text(188, 498, b"available: syscall shutdown");
    rect(176, 536, 472, 8, LINE);
    rect(176, 396, 6, 100, PROMPT);
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
