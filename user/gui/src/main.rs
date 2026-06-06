#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_GUI_CLEAR: u64 = 16;
const SYS_GUI_RECT: u64 = 17;
const SYS_GUI_TEXT_COLOR: u64 = 21;
const SYS_READ_MOUSE: u64 = 20;
const SYS_TASK_COUNT: u64 = 22;
const SYS_TASK_INFO: u64 = 23;
const SYS_SET_FOCUS: u64 = 25;
const SYS_FOCUS: u64 = 26;
const SYS_CONSOLE_SEQ: u64 = 33;
const SYS_CONSOLE_LEN: u64 = 34;
const SYS_CONSOLE_BYTE: u64 = 35;

const TASK_BASH: u64 = 1;
const TASK_TASKVIEW: u64 = 2;

const SCREEN_W: u64 = 1280;
const SCREEN_H: u64 = 720;
const TOP_H: u64 = 30;
const BOTTOM_H: u64 = 24;

const DESKTOP: u32 = 0x002c3333;
const PANEL: u32 = 0x00e8ebe7;
const PANEL_DARK: u32 = 0x00b7beb8;
const BUTTON: u32 = 0x00d8ded8;
const BUTTON_ACTIVE: u32 = 0x004e9a06;
const WINDOW: u32 = 0x00f5f7f8;
const TITLE: u32 = 0x0035414f;
const TITLE_ACTIVE: u32 = 0x002f5d62;
const SHADOW: u32 = 0x0012171c;
const INK: u32 = 0x0010161c;
const MUTED: u32 = 0x0062707d;
const LIGHT: u32 = 0x00ffffff;
const TERM_BG: u32 = 0x0010161c;
const TERM_FG: u32 = 0x00d7e1d8;
const ACCENT: u32 = 0x0000b894;

#[derive(Clone, Copy)]
struct Mouse {
    x: u64,
    y: u64,
    buttons: u8,
    seq: u8,
}

#[derive(Clone, Copy)]
struct Window {
    x: u64,
    y: u64,
    w: u64,
    h: u64,
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut terminal = Window {
        x: 92,
        y: 74,
        w: 850,
        h: 540,
    };
    let tasks = Window {
        x: 870,
        y: 86,
        w: 340,
        h: 360,
    };
    let mut show_tasks = false;
    let mut dragging_terminal = false;
    let mut drag_dx = 0;
    let mut drag_dy = 0;
    let mut last_mouse = read_mouse();
    let mut last_mouse_seq = last_mouse.seq;
    let mut last_console_seq = u64::MAX;
    let mut last_focus = u64::MAX;

    syscall1(SYS_SET_FOCUS, TASK_BASH);
    redraw(terminal, tasks, show_tasks, last_mouse);

    loop {
        let mouse = read_mouse();
        let console_seq = syscall0(SYS_CONSOLE_SEQ);
        let focus = syscall0(SYS_FOCUS);
        if focus == TASK_TASKVIEW {
            show_tasks = true;
        }
        let mut needs_redraw = console_seq != last_console_seq || focus != last_focus;

        if mouse.seq != last_mouse_seq {
            let pressed = mouse.buttons & 1 != 0;
            let was_pressed = last_mouse.buttons & 1 != 0;
            let mut mouse_needs_redraw = false;

            if pressed && !was_pressed {
                if hit_task_button(mouse) {
                    show_tasks = true;
                    syscall1(SYS_SET_FOCUS, TASK_TASKVIEW);
                    mouse_needs_redraw = true;
                } else if hit_window_title(mouse, terminal) {
                    syscall1(SYS_SET_FOCUS, TASK_BASH);
                    dragging_terminal = true;
                    drag_dx = mouse.x.saturating_sub(terminal.x);
                    drag_dy = mouse.y.saturating_sub(terminal.y);
                    mouse_needs_redraw = true;
                } else if hit_window(mouse, terminal) {
                    syscall1(SYS_SET_FOCUS, TASK_BASH);
                    mouse_needs_redraw = true;
                } else if show_tasks && hit_window(mouse, tasks) {
                    syscall1(SYS_SET_FOCUS, TASK_TASKVIEW);
                    mouse_needs_redraw = true;
                }
            }

            if !pressed {
                mouse_needs_redraw |= dragging_terminal;
                dragging_terminal = false;
            }

            if dragging_terminal {
                terminal.x = mouse.x.saturating_sub(drag_dx).clamp(16, SCREEN_W - terminal.w - 16);
                terminal.y = mouse
                    .y
                    .saturating_sub(drag_dy)
                    .clamp(TOP_H + 10, SCREEN_H - BOTTOM_H - terminal.h - 18);
                mouse_needs_redraw = true;
            }

            last_mouse = mouse;
            last_mouse_seq = mouse.seq;
            needs_redraw |= mouse_needs_redraw;
        }

        if needs_redraw {
            redraw(terminal, tasks, show_tasks, mouse);
            last_console_seq = console_seq;
            last_focus = focus;
        }

        for _ in 0..8 {
            syscall0(SYS_YIELD);
        }
    }
}

fn redraw(terminal: Window, tasks: Window, show_tasks: bool, mouse: Mouse) {
    let _ = mouse;
    syscall1(SYS_GUI_CLEAR, DESKTOP as u64);
    draw_top_bar(show_tasks);
    draw_bottom_bar();
    draw_terminal(terminal, syscall0(SYS_FOCUS) == TASK_BASH);
    if show_tasks {
        draw_taskviewer(tasks, syscall0(SYS_FOCUS) == TASK_TASKVIEW);
    }
}

fn draw_top_bar(show_tasks: bool) {
    rect(0, 0, SCREEN_W, TOP_H, PANEL);
    rect(0, TOP_H - 1, SCREEN_W, 1, PANEL_DARK);
    rect(12, 8, 14, 14, ACCENT);
    text(36, 6, b"nk", INK);
    text(72, 6, b"Terminal", MUTED);
    rect(1144, 4, 92, 22, if show_tasks { BUTTON_ACTIVE } else { BUTTON });
    text(1160, 6, b"Tasks", if show_tasks { LIGHT } else { INK });
}

fn draw_bottom_bar() {
    let y = SCREEN_H - BOTTOM_H;
    rect(0, y, SCREEN_W, BOTTOM_H, PANEL);
    rect(0, y, SCREEN_W, 1, PANEL_DARK);
    let focus = syscall0(SYS_FOCUS);
    bottom_entry(12, b"bash", focus == TASK_BASH);
    bottom_entry(128, b"taskviewer", focus == TASK_TASKVIEW);
}

fn bottom_entry(x: u64, label: &'static [u8], active: bool) {
    rect(x, SCREEN_H - BOTTOM_H + 4, 102, 16, if active { BUTTON_ACTIVE } else { BUTTON });
    text(x + 10, SCREEN_H - BOTTOM_H + 5, label, if active { LIGHT } else { INK });
}

fn draw_terminal(win: Window, active: bool) {
    draw_window_frame(win, b"bash", active);
    rect(win.x + 18, win.y + 58, win.w - 36, win.h - 76, TERM_BG);
    draw_console(win.x + 34, win.y + 76, win.w - 68, win.h - 108);
}

fn draw_taskviewer(win: Window, active: bool) {
    draw_window_frame(win, b"taskviewer", active);
    text(win.x + 24, win.y + 64, b"name", MUTED);
    text(win.x + 182, win.y + 64, b"state", MUTED);
    rect(win.x + 24, win.y + 90, win.w - 48, 1, PANEL_DARK);
    let count = syscall0(SYS_TASK_COUNT).min(6);
    let mut row = win.y + 112;
    for index in 0..count {
        let info = syscall1(SYS_TASK_INFO, index);
        draw_task_row(win.x + 24, row, info);
        row += 34;
    }
}

fn draw_window_frame(win: Window, title: &'static [u8], active: bool) {
    rect(win.x + 8, win.y + 8, win.w, win.h, SHADOW);
    rect(win.x, win.y, win.w, win.h, WINDOW);
    rect(win.x, win.y, win.w, 42, if active { TITLE_ACTIVE } else { TITLE });
    rect(win.x + 16, win.y + 14, 10, 10, 0x00ff605c);
    rect(win.x + 34, win.y + 14, 10, 10, 0x00ffbd44);
    rect(win.x + 52, win.y + 14, 10, 10, 0x0000ca4e);
    text_bytes(win.x + 82, win.y + 11, title, LIGHT);
}

fn draw_console(x: u64, y: u64, width: u64, height: u64) {
    let cols = (width / 9).max(1) as usize;
    let rows = (height / 16).max(1) as usize;
    let len = syscall0(SYS_CONSOLE_LEN) as usize;
    let mut lines = [[0u8; 96]; 28];
    let mut line_lens = [0usize; 28];
    let mut row = 0usize;
    let mut col = 0usize;
    let start = len.saturating_sub(cols * rows);

    for index in start..len {
        let byte = syscall1(SYS_CONSOLE_BYTE, index as u64) as u8;
        if byte == b'\r' {
            continue;
        }
        if byte == b'\n' {
            row = (row + 1).min(rows.saturating_sub(1));
            col = 0;
            continue;
        }
        if row < rows && col < cols.min(96) {
            lines[row][col] = byte;
            line_lens[row] = line_lens[row].max(col + 1);
            col += 1;
        } else if row + 1 < rows {
            row += 1;
            col = 0;
        }
    }

    for line in 0..rows.min(28) {
        text_bytes(x, y + line as u64 * 16, &lines[line][..line_lens[line]], TERM_FG);
    }
}

fn draw_task_row(x: u64, y: u64, info: u64) {
    let name_id = info & 0xff;
    let flags = (info >> 8) & 0xff;
    let active = flags & 1 != 0;
    let current = flags & 2 != 0;
    let name = match name_id {
        1 => b"gui" as &[u8],
        2 => b"bash",
        3 => b"taskviewer",
        4 => b"child/cat",
        _ => b"unknown",
    };
    let state = if current {
        b"running" as &[u8]
    } else if active {
        b"ready"
    } else {
        b"sleep"
    };
    let color = if current { ACCENT } else { INK };
    text_bytes(x, y, name, color);
    text_bytes(x + 158, y, state, color);
}

fn hit_task_button(mouse: Mouse) -> bool {
    mouse.x >= 1144 && mouse.x < 1236 && mouse.y >= 4 && mouse.y < 26
}

fn hit_window(mouse: Mouse, win: Window) -> bool {
    mouse.x >= win.x && mouse.x < win.x + win.w && mouse.y >= win.y && mouse.y < win.y + win.h
}

fn hit_window_title(mouse: Mouse, win: Window) -> bool {
    hit_window(mouse, win) && mouse.y < win.y + 42
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
    text_bytes(x, y, bytes, color);
}

fn text_bytes(x: u64, y: u64, bytes: &[u8], color: u32) {
    syscall5(
        SYS_GUI_TEXT_COLOR,
        x,
        y,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        color as u64,
    );
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
