#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo, ptr};

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_IOCTL: u64 = 16;
const SYS_EXIT: u64 = 60;

const STDIN: i32 = 0;
const STDOUT: i32 = 1;
const STDERR: i32 = 2;

const TCGETS: u64 = 0x5401;
const TCSETS: u64 = 0x5402;

const O_WRONLY: u64 = 1;
const O_CREAT: u64 = 0x40;
const O_TRUNC: u64 = 0x200;

const CTRL_S: u8 = 19;
const CTRL_X: u8 = 24;
const BACKSPACE: u8 = 8;
const DELETE: u8 = 127;

const BUFFER_CAP: usize = 32 * 1024;
const PATH_CAP: usize = 256;
const SCREEN_COLS: usize = 80;
const SCREEN_ROWS: usize = 24;
const TEXT_ROWS: usize = 20;

static mut BUFFER: [u8; BUFFER_CAP] = [0; BUFFER_CAP];
static mut PATH: [u8; PATH_CAP] = [0; PATH_CAP];
static mut ORIGINAL_TERMIOS: [u8; 64] = [0; 64];
static mut RAW_TERMIOS: [u8; 64] = [0; 64];

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let stack: u64;
    unsafe {
        asm!("mov {}, rsp", out(reg) stack, options(nomem, nostack, preserves_flags));
    }
    let code = main(stack as *const u64);
    exit(code)
}

fn main(stack: *const u64) -> i32 {
    let Some(path) = first_arg(stack) else {
        write_all(STDERR, b"usage: nkedit PATH\n");
        return 2;
    };
    let Some(path) = copy_path(path) else {
        write_all(STDERR, b"nkedit: path too long\n");
        return 2;
    };

    enable_raw_mode();
    let mut editor = Editor {
        path,
        len: 0,
        cursor: 0,
        top_line: 0,
        dirty: false,
        status: b"^ means Ctrl/Strg",
    };
    editor.load();
    editor.render();

    loop {
        let key = read_key();
        match key {
            CTRL_S => editor.save(),
            CTRL_X => {
                if editor.dirty {
                    editor.status = b"Unsaved changes. ^S saves, ^X again quits";
                    editor.dirty = false;
                    editor.render();
                } else {
                    disable_raw_mode();
                    clear_screen();
                    return 0;
                }
            }
            BACKSPACE | DELETE => editor.backspace(),
            b'\r' | b'\n' => editor.insert(b'\n'),
            byte if byte >= 0x20 => editor.insert(byte),
            _ => {}
        }
        editor.render();
    }
}

struct Editor {
    path: &'static [u8],
    len: usize,
    cursor: usize,
    top_line: usize,
    dirty: bool,
    status: &'static [u8],
}

impl Editor {
    fn load(&mut self) {
        let fd = sys_open(self.path.as_ptr(), 0) as i64;
        if fd < 0 {
            self.status = b"New file";
            return;
        }
        while self.len < BUFFER_CAP {
            let read = unsafe {
                sys_read(
                    fd as i32,
                    ptr::addr_of_mut!(BUFFER).cast::<u8>().add(self.len),
                    BUFFER_CAP - self.len,
                ) as i64
            };
            if read <= 0 {
                break;
            }
            self.len += read as usize;
        }
        sys_close(fd as i32);
        self.status = if self.len == BUFFER_CAP {
            b"File truncated to editor buffer"
        } else {
            b"Loaded"
        };
    }

    fn render(&mut self) {
        self.keep_cursor_visible();
        write_all(STDOUT, b"\x1b[2J\x1b[H");
        write_all(STDOUT, b"nkedit ");
        write_all(STDOUT, self.display_path());
        if self.dirty {
            write_all(STDOUT, b" *");
        }
        write_all(STDOUT, b"\x1b[K");

        for row in 0..TEXT_ROWS {
            move_cursor(row + 2, 1);
            write_all(STDOUT, b"\x1b[K");
            if let Some((start, end)) = self.line_bounds(self.top_line + row) {
                let count = (end - start).min(SCREEN_COLS);
                unsafe {
                    write_all(STDOUT, &BUFFER[start..start + count]);
                }
            } else {
                write_all(STDOUT, b"~");
            }
        }

        move_cursor(SCREEN_ROWS - 1, 1);
        write_all(STDOUT, b"^S Speichern    ^X Beenden    Backspace Loeschen    Enter Neue Zeile");
        write_all(STDOUT, b"\x1b[K");

        move_cursor(SCREEN_ROWS, 1);
        write_all(STDOUT, b"Info: ^ bedeutet Ctrl/Strg.  ");
        write_all(STDOUT, self.status);
        write_all(STDOUT, b"\x1b[K");

        let (line, col) = self.cursor_line_col();
        let screen_row = line.saturating_sub(self.top_line).min(TEXT_ROWS - 1) + 2;
        let screen_col = col.min(SCREEN_COLS - 1) + 1;
        move_cursor(screen_row, screen_col);
    }

    fn insert(&mut self, byte: u8) {
        if self.len >= BUFFER_CAP {
            self.status = b"Buffer full";
            return;
        }
        unsafe {
            let buffer = ptr::addr_of_mut!(BUFFER).cast::<u8>();
            let mut index = self.len;
            while index > self.cursor {
                *buffer.add(index) = *buffer.add(index - 1);
                index -= 1;
            }
            *buffer.add(self.cursor) = byte;
        }
        self.cursor += 1;
        self.len += 1;
        self.dirty = true;
        self.status = b"Modified";
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        unsafe {
            let buffer = ptr::addr_of_mut!(BUFFER).cast::<u8>();
            let mut index = self.cursor - 1;
            while index + 1 < self.len {
                *buffer.add(index) = *buffer.add(index + 1);
                index += 1;
            }
        }
        self.cursor -= 1;
        self.len -= 1;
        self.dirty = true;
        self.status = b"Modified";
    }

    fn save(&mut self) {
        let fd = sys_open(self.path.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC) as i64;
        if fd < 0 {
            self.status = b"Save failed: open";
            return;
        }
        let mut written = 0usize;
        while written < self.len {
            let count = unsafe {
                sys_write(
                    fd as i32,
                    ptr::addr_of!(BUFFER).cast::<u8>().add(written),
                    self.len - written,
                ) as i64
            };
            if count <= 0 {
                sys_close(fd as i32);
                self.status = b"Save failed: write";
                return;
            }
            written += count as usize;
        }
        sys_close(fd as i32);
        self.dirty = false;
        self.status = b"Saved";
    }

    fn display_path(&self) -> &[u8] {
        self.path.strip_suffix(&[0]).unwrap_or(self.path)
    }

    fn line_bounds(&self, target: usize) -> Option<(usize, usize)> {
        let mut line = 0usize;
        let mut start = 0usize;
        let mut index = 0usize;
        unsafe {
            while index <= self.len {
                if index == self.len || BUFFER[index] == b'\n' {
                    if line == target {
                        return Some((start, index));
                    }
                    line += 1;
                    index += 1;
                    start = index;
                } else {
                    index += 1;
                }
            }
        }
        None
    }

    fn cursor_line_col(&self) -> (usize, usize) {
        let mut line = 0usize;
        let mut col = 0usize;
        unsafe {
            for index in 0..self.cursor.min(self.len) {
                if BUFFER[index] == b'\n' {
                    line += 1;
                    col = 0;
                } else {
                    col += 1;
                }
            }
        }
        (line, col)
    }

    fn keep_cursor_visible(&mut self) {
        let (line, _) = self.cursor_line_col();
        if line < self.top_line {
            self.top_line = line;
        } else if line >= self.top_line + TEXT_ROWS {
            self.top_line = line - TEXT_ROWS + 1;
        }
    }
}

fn enable_raw_mode() {
    unsafe {
        let original = ptr::addr_of_mut!(ORIGINAL_TERMIOS).cast::<u8>();
        if sys_ioctl(STDIN, TCGETS, original) != 0 {
            return;
        }
        let raw = ptr::addr_of_mut!(RAW_TERMIOS).cast::<u8>();
        for index in 0..64 {
            *raw.add(index) = *original.add(index);
        }
        for index in 12..16 {
            *raw.add(index) = 0;
        }
        let _ = sys_ioctl(STDIN, TCSETS, raw);
    }
}

fn disable_raw_mode() {
    let _ = sys_ioctl(STDIN, TCSETS, ptr::addr_of_mut!(ORIGINAL_TERMIOS).cast::<u8>());
}

fn clear_screen() {
    write_all(STDOUT, b"\x1b[2J\x1b[H");
}

fn move_cursor(row: usize, col: usize) {
    let mut bytes = [0u8; 24];
    let mut len = 0usize;
    bytes[len] = 0x1b;
    len += 1;
    bytes[len] = b'[';
    len += 1;
    len += push_dec(row, &mut bytes[len..]);
    bytes[len] = b';';
    len += 1;
    len += push_dec(col, &mut bytes[len..]);
    bytes[len] = b'H';
    len += 1;
    write_all(STDOUT, &bytes[..len]);
}

fn push_dec(mut value: usize, out: &mut [u8]) -> usize {
    if value == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 20];
    let mut len = 0usize;
    while value > 0 {
        tmp[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    for index in 0..len {
        out[index] = tmp[len - index - 1];
    }
    len
}

fn first_arg(stack: *const u64) -> Option<&'static [u8]> {
    if stack.is_null() {
        return None;
    }
    let argc = unsafe { *stack as usize };
    if argc < 2 {
        return None;
    }
    let ptr = unsafe { *stack.add(2) as *const u8 };
    cstr(ptr)
}

fn copy_path(path: &[u8]) -> Option<&'static [u8]> {
    if path.is_empty() || path.len() >= PATH_CAP {
        return None;
    }
    unsafe {
        PATH[..path.len()].copy_from_slice(path);
        PATH[path.len()] = 0;
        Some(&PATH[..path.len() + 1])
    }
}

fn cstr(ptr: *const u8) -> Option<&'static [u8]> {
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while len < PATH_CAP - 1 && *ptr.add(len) != 0 {
            len += 1;
        }
        if len == 0 || len >= PATH_CAP - 1 {
            return None;
        }
        Some(core::slice::from_raw_parts(ptr, len))
    }
}

fn read_key() -> u8 {
    let mut byte = [0u8; 1];
    loop {
        let read = sys_read(STDIN, byte.as_mut_ptr(), 1) as i64;
        if read == 1 {
            return byte[0];
        }
    }
}

fn write_all(fd: i32, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let count = sys_write(fd, bytes.as_ptr(), bytes.len()) as i64;
        if count <= 0 {
            return;
        }
        bytes = &bytes[count as usize..];
    }
}

fn sys_read(fd: i32, buffer: *mut u8, len: usize) -> u64 {
    syscall3(SYS_READ, fd as u64, buffer as u64, len as u64)
}

fn sys_write(fd: i32, buffer: *const u8, len: usize) -> u64 {
    syscall3(SYS_WRITE, fd as u64, buffer as u64, len as u64)
}

fn sys_open(path: *const u8, flags: u64) -> u64 {
    syscall2(SYS_OPEN, path as u64, flags)
}

fn sys_close(fd: i32) -> u64 {
    syscall1(SYS_CLOSE, fd as u64)
}

fn sys_ioctl(fd: i32, request: u64, argp: *mut u8) -> i64 {
    syscall3(SYS_IOCTL, fd as u64, request, argp as u64) as i64
}

fn exit(code: i32) -> ! {
    let _ = syscall1(SYS_EXIT, code as u64);
    loop {}
}

fn syscall1(id: u64, a: u64) -> u64 {
    let out;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") id => out,
            in("rdi") a,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    out
}

fn syscall2(id: u64, a: u64, b: u64) -> u64 {
    let out;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") id => out,
            in("rdi") a,
            in("rsi") b,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    out
}

fn syscall3(id: u64, a: u64, b: u64, c: u64) -> u64 {
    let out;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") id => out,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    out
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    disable_raw_mode();
    exit(101)
}
