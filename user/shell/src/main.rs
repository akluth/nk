#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_GUI_RECT: u64 = 17;
const SYS_GUI_TEXT_COLOR: u64 = 21;
const SYS_READ_KEY: u64 = 19;
const SYS_READ_MOUSE: u64 = 20;
const SYS_RUN_CAT: u64 = 24;
const SYS_SET_ACTIVE_WINDOW: u64 = 25;
const SYS_ACTIVE_WINDOW: u64 = 26;
const SYS_SHUTDOWN: u64 = 32;

const BG: u32 = 0x00191d24;
const PANEL: u32 = 0x00282f3a;
const ACCENT: u32 = 0x0000b894;
const ACTIVE_TAB: u32 = 0x005f6f86;
const INACTIVE_TAB: u32 = 0x00343d4a;
const SHADOW: u32 = 0x000d1117;
const WINDOW: u32 = 0x00f3f5f7;
const TITLE: u32 = 0x00343d4a;
const INK: u32 = 0x00101820;
const MUTED: u32 = 0x005f6b7a;
const LIGHT: u32 = 0x00f3f5f7;
const CURSOR: u32 = 0x00ffbd44;
const TASK_SHELL: u64 = 1;
const TASK_TASKVIEW: u64 = 2;
const TASK_CAT: u64 = 3;

#[derive(Clone, Copy)]
enum Output {
    Ready,
    Version,
    Cat,
    CatMissing,
    Shutdown,
    Unknown,
}

#[derive(Clone, Copy)]
struct Mouse {
    x: u64,
    y: u64,
    buttons: u8,
    seq: u8,
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut input = [0u8; 32];
    let mut len = 0usize;
    let mut output = Output::Ready;
    let mut x = 150u64;
    let mut y = 96u64;
    let mut dragging = false;
    let mut drag_dx = 0u64;
    let mut drag_dy = 0u64;
    let mut last_mouse = read_mouse();
    let mut last_mouse_seq = last_mouse.seq;

    redraw_if_active(x, y, &input, len, output, last_mouse);
    loop {
        let key = syscall0(SYS_READ_KEY) as u8;
        if key != 0 && active_window() == TASK_SHELL {
            match key {
                b'\n' => {
                    output = run_command(&input[..len]);
                    len = 0;
                }
                8 => len = len.saturating_sub(1),
                b'a'..=b'z' | b'0'..=b'9' | b' ' => {
                    if len < input.len() {
                        input[len] = key;
                        len += 1;
                    }
                }
                _ => {}
            }
            last_mouse = read_mouse();
            last_mouse_seq = last_mouse.seq;
            redraw_if_active(x, y, &input, len, output, last_mouse);
        }

        let mouse = read_mouse();
        if mouse.seq != last_mouse_seq {
            let previous = last_mouse;
            last_mouse = mouse;
            last_mouse_seq = mouse.seq;
            let down = mouse.buttons & 1 != 0;
            if down && hit_taskbar(mouse) {
                set_active_from_taskbar(mouse);
                redraw_if_active(x, y, &input, len, output, mouse);
                syscall0(SYS_YIELD);
                continue;
            }
            if down && !dragging && hit_title(mouse, x, y) {
                dragging = true;
                drag_dx = mouse.x.saturating_sub(x);
                drag_dy = mouse.y.saturating_sub(y);
            } else if !down {
                dragging = false;
            }

            if dragging {
                draw_desktop();
                x = mouse.x.saturating_sub(drag_dx).clamp(20, 720);
                y = mouse.y.saturating_sub(drag_dy).clamp(50, 420);
                redraw_if_active(x, y, &input, len, output, mouse);
            } else {
                repair_pointer(previous, mouse, x, y, &input, len, output);
            }
        }

        syscall0(SYS_YIELD);
    }
}

fn run_command(command: &[u8]) -> Output {
    if command == b"version" {
        Output::Version
    } else if command == b"cat" {
        if syscall1(SYS_RUN_CAT, TASK_CAT) == 0 {
            Output::Cat
        } else {
            Output::CatMissing
        }
    } else if command == b"shutdown" {
        syscall0(SYS_SHUTDOWN);
        Output::Shutdown
    } else {
        Output::Unknown
    }
}

fn redraw_if_active(x: u64, y: u64, input: &[u8; 32], len: usize, output: Output, mouse: Mouse) {
    draw_desktop();
    if active_window() == TASK_SHELL {
        redraw_window(x, y, input, len, output, mouse);
    } else {
        draw_pointer(mouse.x, mouse.y);
    }
}

fn redraw_window(x: u64, y: u64, input: &[u8; 32], len: usize, output: Output, mouse: Mouse) {
    draw_shell(x, y, input, len, output);
    draw_pointer(mouse.x, mouse.y);
}

fn draw_desktop() {
    rect(0, 36, 1280, 684, BG);
    draw_taskbar();
}

fn draw_taskbar() {
    rect(0, 0, 1280, 36, PANEL);
    rect(18, 10, 16, 16, ACCENT);
    let focus = active_window();
    taskbar_entry(52, b"shell", focus == TASK_SHELL);
    taskbar_entry(170, b"tasks", focus == TASK_TASKVIEW);
    taskbar_entry(288, b"cat", focus == TASK_CAT);
}

fn taskbar_entry(x: u64, label: &'static [u8], active: bool) {
    let bg = if active { ACTIVE_TAB } else { INACTIVE_TAB };
    let fg = if active { LIGHT } else { 0x00d2d8e2 };
    rect(x, 4, 108, 28, bg);
    rect(x, 4, 108, 2, fg);
    text(x + 12, 8, label, fg);
}

fn draw_shell(x: u64, y: u64, input: &[u8; 32], len: usize, output: Output) {
    rect(x, y, 680, 340, WINDOW);
    rect(x + 680, y + 8, 8, 340, SHADOW);
    rect(x + 8, y + 340, 680, 8, SHADOW);
    rect(x, y, 680, 40, TITLE);
    rect(x + 16, y + 13, 10, 10, 0x00ff605c);
    rect(x + 34, y + 13, 10, 10, 0x00ffbd44);
    rect(x + 52, y + 13, 10, 10, 0x0000ca4e);
    text(x + 84, y + 10, b"nk shell", LIGHT);

    text(x + 34, y + 76, b"type: version  cat  shutdown", MUTED);
    text(x + 34, y + 132, b">", ACCENT);
    text_bytes(x + 70, y + 132, &input[..len], INK);
    rect(x + 70 + len as u64 * 13, y + 154, 12, 3, ACCENT);

    match output {
        Output::Ready => text(x + 34, y + 210, b"ready", MUTED),
        Output::Version => text(x + 34, y + 210, b"nk 0.1.0", INK),
        Output::Cat => text(x + 34, y + 210, b"cat started", INK),
        Output::CatMissing => text(x + 34, y + 210, b"cat not found", INK),
        Output::Shutdown => text(x + 34, y + 210, b"shutting down", INK),
        Output::Unknown => text(x + 34, y + 210, b"unknown command", INK),
    }
}

fn repair_pointer(
    previous: Mouse,
    mouse: Mouse,
    x: u64,
    y: u64,
    input: &[u8; 32],
    len: usize,
    output: Output,
) {
    if pointer_hits_window(previous, x, y) || pointer_hits_window(mouse, x, y) {
        draw_shell(x, y, input, len, output);
    } else {
        restore_desktop_at(previous.x, previous.y);
    }
    draw_pointer(mouse.x, mouse.y);
}

fn restore_desktop_at(x: u64, y: u64) {
    if y < 36 {
        draw_taskbar();
    } else {
        rect(x.saturating_sub(2), y.saturating_sub(2), 22, 26, BG);
    }
}

fn draw_pointer(x: u64, y: u64) {
    rect(x, y, 3, 18, CURSOR);
    rect(x + 3, y + 3, 3, 12, CURSOR);
    rect(x + 6, y + 6, 3, 9, CURSOR);
    rect(x + 9, y + 9, 3, 6, CURSOR);
}

fn hit_title(mouse: Mouse, x: u64, y: u64) -> bool {
    active_window() == TASK_SHELL
        && mouse.x >= x
        && mouse.x < x + 680
        && mouse.y >= y
        && mouse.y < y + 40
}

fn pointer_hits_window(mouse: Mouse, x: u64, y: u64) -> bool {
    mouse.x + 14 >= x && mouse.x < x + 688 && mouse.y + 22 >= y && mouse.y < y + 348
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

fn active_window() -> u64 {
    syscall0(SYS_ACTIVE_WINDOW)
}

fn hit_taskbar(mouse: Mouse) -> bool {
    mouse.y < 36
}

fn set_active_from_taskbar(mouse: Mouse) {
    let id = if mouse.x >= 52 && mouse.x < 152 {
        TASK_SHELL
    } else if mouse.x >= 170 && mouse.x < 270 {
        TASK_TASKVIEW
    } else if mouse.x >= 288 && mouse.x < 388 {
        TASK_CAT
    } else {
        0
    };
    if id != 0 {
        syscall1(SYS_SET_ACTIVE_WINDOW, id);
    }
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
