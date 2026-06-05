#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_GUI_RECT: u64 = 17;
const SYS_GUI_TEXT: u64 = 18;
const SYS_READ_KEY: u64 = 19;
const SYS_SHUTDOWN: u64 = 32;

const SHADOW: u32 = 0x000d1117;
const WINDOW: u32 = 0x00e8edf2;
const TITLE: u32 = 0x00343d4a;
const PROMPT: u32 = 0x0000b894;
const LINE: u32 = 0x0088909c;

#[derive(Clone, Copy)]
enum Output {
    Ready,
    Version,
    Shutdown,
    Unknown,
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut input = [0u8; 32];
    let mut len = 0usize;
    let mut output = Output::Ready;

    draw_shell(&input, len, output);
    loop {
        let key = syscall0(SYS_READ_KEY) as u8;
        if key != 0 {
            match key {
                b'\n' => {
                    output = run_command(&input[..len]);
                    len = 0;
                }
                8 => {
                    len = len.saturating_sub(1);
                }
                b'a'..=b'z' | b'0'..=b'9' | b' ' => {
                    if len < input.len() {
                        input[len] = key;
                        len += 1;
                    }
                }
                _ => {}
            }
            draw_shell(&input, len, output);
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

fn draw_shell(input: &[u8; 32], len: usize, output: Output) {
    rect(168, 116, 560, 270, SHADOW);
    rect(160, 108, 560, 270, WINDOW);
    rect(160, 108, 560, 30, TITLE);
    rect(176, 120, 10, 10, 0x00ff605c);
    rect(194, 120, 10, 10, 0x00ffbd44);
    rect(212, 120, 10, 10, 0x0000ca4e);
    rect(176, 160, 6, 178, PROMPT);

    text(188, 166, b"nk shell");
    text(188, 204, b"type version or shutdown");
    text(188, 248, b"> ");
    text_bytes(212, 248, &input[..len]);
    rect(212 + len as u64 * 8, 264, 8, 3, LINE);

    match output {
        Output::Ready => text(188, 304, b"ready"),
        Output::Version => text(188, 304, b"nk 0.1.0"),
        Output::Shutdown => text(188, 304, b"shutting down"),
        Output::Unknown => text(188, 304, b"unknown command"),
    }
}

fn rect(x: u64, y: u64, width: u64, height: u64, color: u32) {
    syscall5(SYS_GUI_RECT, x, y, width, height, color as u64);
}

fn text(x: u64, y: u64, bytes: &'static [u8]) {
    text_bytes(x, y, bytes);
}

fn text_bytes(x: u64, y: u64, bytes: &[u8]) {
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
