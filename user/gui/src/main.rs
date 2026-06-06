#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_GUI_CLEAR: u64 = 16;
const SYS_GUI_RECT: u64 = 17;
const SYS_GUI_TEXT_COLOR: u64 = 21;
const SYS_READ_MOUSE: u64 = 20;
const SYS_SET_FOCUS: u64 = 25;
const SYS_FOCUS: u64 = 26;
const SYS_TASK_COUNT: u64 = 22;

const BG: u32 = 0x00191d24;
const PANEL: u32 = 0x00282f3a;
const ACTIVE: u32 = 0x005f6f86;
const INACTIVE: u32 = 0x00343d4a;
const LIGHT: u32 = 0x00f3f5f7;
const ACCENT: u32 = 0x0000b894;

#[derive(Clone, Copy)]
struct Mouse {
    x: u64,
    y: u64,
    buttons: u8,
    seq: u8,
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    draw_background();
    let mut last_seq = read_mouse().seq;
    loop {
        let mouse = read_mouse();
        if mouse.seq != last_seq {
            last_seq = mouse.seq;
            if mouse.buttons & 1 != 0 && mouse.y < 36 {
                click_taskbar(mouse.x);
            }
        }
        draw_taskbar();
        for _ in 0..20 {
            syscall0(SYS_YIELD);
        }
    }
}

fn draw_background() {
    syscall1(SYS_GUI_CLEAR, BG as u64);
    draw_taskbar();
}

fn draw_taskbar() {
    rect(0, 0, 1280, 36, PANEL);
    rect(18, 10, 16, 16, ACCENT);
    let focus = syscall0(SYS_FOCUS);
    let _count = syscall0(SYS_TASK_COUNT);
    taskbar_entry(52, 1, focus == 1);
    taskbar_entry(170, 2, focus == 2);
    taskbar_entry(288, 3, focus == 3);
}

fn taskbar_entry(x: u64, index: u64, active: bool) {
    let bg = if active { ACTIVE } else { INACTIVE };
    let fg = if active { LIGHT } else { 0x00d2d8e2 };
    rect(x, 6, 108, 24, bg);
    text(x + 12, 8, label(index), fg);
}

fn label(index: u64) -> &'static [u8] {
    match index {
        1 => b"shell",
        2 => b"tasks",
        3 => b"cat",
        _ => b"gui",
    }
}

fn click_taskbar(mouse_x: u64) {
    let id = if mouse_x >= 52 && mouse_x < 160 {
        1
    } else if mouse_x >= 170 && mouse_x < 278 {
        2
    } else if mouse_x >= 288 && mouse_x < 396 {
        3
    } else {
        0
    };
    if id != 0 {
        syscall1(SYS_SET_FOCUS, id);
    }
}

fn read_mouse() -> Mouse {
    let packed = syscall0(SYS_READ_MOUSE);
    Mouse {
        x: packed & 0xffff,
        y: (packed >> 16) & 0xffff,
        buttons: ((packed >> 32) & 0xff) as u8,
        seq: ((packed >> 40) & 0xff) as u8,
    }
}

fn rect(x: u64, y: u64, width: u64, height: u64, color: u32) {
    syscall5(SYS_GUI_RECT, x, y, width, height, color as u64);
}

fn text(x: u64, y: u64, bytes: &'static [u8], color: u32) {
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
        asm!(
            "int 0x80",
            inlateout("rax") id => out,
            in("rdi") a,
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
