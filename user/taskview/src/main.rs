#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_GUI_RECT: u64 = 17;
const SYS_GUI_TEXT_COLOR: u64 = 21;
const SYS_TASK_COUNT: u64 = 22;
const SYS_TASK_INFO: u64 = 23;

const SHADOW: u32 = 0x000d1117;
const WINDOW: u32 = 0x00f3f5f7;
const TITLE: u32 = 0x00343d4a;
const INK: u32 = 0x00101820;
const MUTED: u32 = 0x005f6b7a;
const LIGHT: u32 = 0x00f3f5f7;
const ACCENT: u32 = 0x0000b894;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    draw();
    loop {
        syscall0(SYS_YIELD);
    }
}

fn draw() {
    let x = 860;
    let y = 96;
    rect(x, y, 360, 300, WINDOW);
    rect(x + 360, y + 8, 8, 300, SHADOW);
    rect(x + 8, y + 300, 360, 8, SHADOW);
    rect(x, y, 360, 40, TITLE);
    rect(x + 16, y + 13, 10, 10, 0x00ff605c);
    rect(x + 34, y + 13, 10, 10, 0x00ffbd44);
    rect(x + 52, y + 13, 10, 10, 0x0000ca4e);
    text(x + 84, y + 10, b"tasks", LIGHT);

    text(x + 28, y + 72, b"name", MUTED);
    text(x + 190, y + 72, b"state", MUTED);
    text(x + 28, y + 104, b"----------------", MUTED);

    let count = syscall0(SYS_TASK_COUNT).min(6);
    let mut row = y + 138;
    for index in 0..count {
        let info = syscall1(SYS_TASK_INFO, index);
        draw_task_row(x + 28, row, info);
        row += 36;
    }
}

fn draw_task_row(x: u64, y: u64, info: u64) {
    let name_id = info & 0xff;
    let flags = (info >> 8) & 0xff;
    let current = flags & 2 != 0;
    let name = match name_id {
        1 => b"gui" as &[u8],
        2 => b"shell",
        3 => b"taskview",
        _ => b"unknown",
    };
    let state = if current { b"running" as &[u8] } else { b"ready" };
    let color = if current { ACCENT } else { INK };
    text_bytes(x, y, name, color);
    text_bytes(x + 162, y, state, color);
}

fn rect(x: u64, y: u64, width: u64, height: u64, color: u32) {
    syscall5(SYS_GUI_RECT, x, y, width, height, color as u64);
}

fn text(x: u64, y: u64, bytes: &'static [u8], color: u32) {
    text_bytes(x, y, bytes, color);
}

fn text_bytes(x: u64, y: u64, bytes: &[u8], color: u32) {
    syscall5(SYS_GUI_TEXT_COLOR, x, y, bytes.as_ptr() as u64, bytes.len() as u64, color as u64);
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
        asm!("int 0x80", inlateout("rax") id => out, in("rdi") a, options(nostack, preserves_flags));
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
