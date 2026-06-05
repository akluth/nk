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
const SHADOW: u32 = 0x000d1117;
const WINDOW: u32 = 0x00e8edf2;
const TITLE: u32 = 0x003b4252;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut x = 96u64;
    let mut y = 88u64;
    let mut dx = 2i64;
    let mut dy = 1i64;

    draw_background();
    loop {
        draw_window(x, y);

        for _ in 0..10 {
            syscall0(SYS_YIELD);
        }

        let nx = x as i64 + dx;
        let ny = y as i64 + dy;
        if !(56..=620).contains(&nx) {
            dx = -dx;
        }
        if !(64..=310).contains(&ny) {
            dy = -dy;
        }
        x = (x as i64 + dx) as u64;
        y = (y as i64 + dy) as u64;
    }
}

fn draw_background() {
    syscall1(SYS_GUI_CLEAR, BG as u64);
    rect(0, 0, 1280, 36, PANEL);
    rect(18, 10, 16, 16, ACCENT);
    rect(46, 14, 160, 8, 0x00aab2bd);
}

fn draw_window(x: u64, y: u64) {
    rect(x + 8, y + 8, 420, 210, SHADOW);
    rect(x, y, 420, 210, WINDOW);
    rect(x, y, 420, 34, TITLE);
    rect(x + 16, y + 12, 10, 10, 0x00ff605c);
    rect(x + 34, y + 12, 10, 10, 0x00ffbd44);
    rect(x + 52, y + 12, 10, 10, 0x0000ca4e);
    text(x + 34, y + 86, b"Hallo Welt!");
    rect(x + 34, y + 126, 300, 12, 0x00d0d7de);
    rect(x + 34, y + 154, 220, 10, 0x0088909c);
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
