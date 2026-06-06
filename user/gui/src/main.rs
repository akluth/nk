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

const BG: u32 = 0x002e3436;
const PANEL: u32 = 0x00eeeeec;
const PANEL_DARK: u32 = 0x00d3d7cf;
const ACTIVE: u32 = 0x0087a556;
const INACTIVE: u32 = 0x00babdb6;
const INK: u32 = 0x002e3436;
const LIGHT: u32 = 0x00ffffff;
const ACCENT: u32 = 0x004e9a06;

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
            if mouse.buttons & 1 != 0 && mouse.y >= 684 {
                click_taskbar(mouse.x);
            }
        }
        draw_panels();
        for _ in 0..20 {
            syscall0(SYS_YIELD);
        }
    }
}

fn draw_background() {
    syscall1(SYS_GUI_CLEAR, BG as u64);
    draw_panels();
}

fn draw_panels() {
    rect(0, 0, 1280, 28, PANEL);
    rect(0, 27, 1280, 1, PANEL_DARK);
    rect(10, 7, 14, 14, ACCENT);
    text(34, 5, b"Applications", INK);
    text(164, 5, b"Places", INK);
    text(244, 5, b"System", INK);
    text(1132, 5, b"nk desktop", INK);

    rect(0, 684, 1280, 36, PANEL);
    rect(0, 684, 1280, 1, PANEL_DARK);
    let focus = syscall0(SYS_FOCUS);
    let _count = syscall0(SYS_TASK_COUNT);
    taskbar_entry(10, 1, focus == 1);
    taskbar_entry(160, 2, focus == 2);
    taskbar_entry(310, 3, focus == 3);
}

fn taskbar_entry(x: u64, index: u64, active: bool) {
    let bg = if active { ACTIVE } else { INACTIVE };
    let fg = if active { LIGHT } else { INK };
    rect(x, 690, 138, 24, bg);
    text(x + 12, 692, label(index), fg);
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
    let id = if mouse_x >= 10 && mouse_x < 148 {
        1
    } else if mouse_x >= 160 && mouse_x < 298 {
        2
    } else if mouse_x >= 310 && mouse_x < 448 {
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
