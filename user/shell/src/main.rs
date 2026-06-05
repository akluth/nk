#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_GUI_CLEAR: u64 = 16;
const SYS_GUI_RECT: u64 = 17;
const SYS_GUI_TEXT_COLOR: u64 = 21;
const SYS_READ_KEY: u64 = 19;
const SYS_READ_MOUSE: u64 = 20;
const SYS_SHUTDOWN: u64 = 32;

const BG: u32 = 0x00191d24;
const PANEL: u32 = 0x00282f3a;
const ACCENT: u32 = 0x0000b894;
const SHADOW: u32 = 0x000d1117;
const WINDOW: u32 = 0x00f3f5f7;
const TITLE: u32 = 0x00343d4a;
const INK: u32 = 0x00101820;
const MUTED: u32 = 0x005f6b7a;
const LIGHT: u32 = 0x00f3f5f7;
const CURSOR: u32 = 0x00ffbd44;

#[derive(Clone, Copy)]
enum Output {
    Ready,
    Version,
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
    let mut last_mouse_seq = 0xff;

    redraw(x, y, &input, len, output, read_mouse());
    loop {
        let key = syscall0(SYS_READ_KEY) as u8;
        if key != 0 {
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
            redraw(x, y, &input, len, output, read_mouse());
        }

        let mouse = read_mouse();
        if mouse.seq != last_mouse_seq {
            last_mouse_seq = mouse.seq;
            let down = mouse.buttons & 1 != 0;
            if down && !dragging && hit_title(mouse, x, y) {
                dragging = true;
                drag_dx = mouse.x.saturating_sub(x);
                drag_dy = mouse.y.saturating_sub(y);
            } else if !down {
                dragging = false;
            }

            if dragging {
                x = mouse.x.saturating_sub(drag_dx).clamp(20, 720);
                y = mouse.y.saturating_sub(drag_dy).clamp(50, 420);
            }
            redraw(x, y, &input, len, output, mouse);
        }

        syscall0(SYS_YIELD);
    }
}

fn run_command(command: &[u8]) -> Output {
    if command == b"version" {
        Output::Version
    } else if command == b"shutdown" {
        syscall0(SYS_SHUTDOWN);
        Output::Shutdown
    } else {
        Output::Unknown
    }
}

fn redraw(x: u64, y: u64, input: &[u8; 32], len: usize, output: Output, mouse: Mouse) {
    draw_desktop();
    draw_shell(x, y, input, len, output);
    draw_pointer(mouse.x, mouse.y);
}

fn draw_desktop() {
    syscall1(SYS_GUI_CLEAR, BG as u64);
    rect(0, 0, 1280, 36, PANEL);
    rect(18, 10, 16, 16, ACCENT);
    rect(46, 14, 160, 8, 0x00aab2bd);
    text(64, 68, b"nk desktop", MUTED);
}

fn draw_shell(x: u64, y: u64, input: &[u8; 32], len: usize, output: Output) {
    rect(x + 8, y + 8, 620, 320, SHADOW);
    rect(x, y, 620, 320, WINDOW);
    rect(x, y, 620, 36, TITLE);
    rect(x + 16, y + 13, 10, 10, 0x00ff605c);
    rect(x + 34, y + 13, 10, 10, 0x00ffbd44);
    rect(x + 52, y + 13, 10, 10, 0x0000ca4e);
    text(x + 84, y + 12, b"nk shell", LIGHT);

    text(x + 34, y + 70, b"type: version  or  shutdown", MUTED);
    text(x + 34, y + 120, b">", ACCENT);
    text_bytes(x + 58, y + 120, &input[..len], INK);
    rect(x + 58 + len as u64 * 12, y + 138, 10, 3, ACCENT);

    match output {
        Output::Ready => text(x + 34, y + 180, b"ready", MUTED),
        Output::Version => text(x + 34, y + 180, b"nk 0.1.0", INK),
        Output::Shutdown => text(x + 34, y + 180, b"shutting down", INK),
        Output::Unknown => text(x + 34, y + 180, b"unknown command", INK),
    }
}

fn draw_pointer(x: u64, y: u64) {
    rect(x, y, 3, 18, CURSOR);
    rect(x + 3, y + 3, 3, 12, CURSOR);
    rect(x + 6, y + 6, 3, 9, CURSOR);
    rect(x + 9, y + 9, 3, 6, CURSOR);
}

fn hit_title(mouse: Mouse, x: u64, y: u64) -> bool {
    mouse.x >= x && mouse.x < x + 620 && mouse.y >= y && mouse.y < y + 36
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
