#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo, ptr};

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_EXIT: u64 = 60;

const STDIN: i32 = 0;
const STDOUT: i32 = 1;
const STDERR: i32 = 2;

const O_WRONLY: u64 = 1;
const O_CREAT: u64 = 0x40;
const O_TRUNC: u64 = 0x200;

const BUFFER_CAP: usize = 32 * 1024;
const LINE_CAP: usize = 512;
const PATH_CAP: usize = 256;

static mut BUFFER: [u8; BUFFER_CAP] = [0; BUFFER_CAP];
static mut LINE: [u8; LINE_CAP] = [0; LINE_CAP];
static mut PATH: [u8; PATH_CAP] = [0; PATH_CAP];

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

    let mut editor = Editor { path, len: 0, dirty: false };
    editor.load();
    editor.banner();
    editor.print();

    loop {
        write_all(STDOUT, b"\nnkedit> ");
        let line = read_line();
        let input = trim(line);
        match input.first().copied() {
            Some(b'a') => editor.append_mode(),
            Some(b'c') => editor.clear(),
            Some(b'h') | Some(b'?') => editor.help(),
            Some(b'p') => editor.print(),
            Some(b'q') => {
                if editor.dirty {
                    write_all(STDOUT, b"unsaved changes; use Q to quit anyway or w to save\n");
                } else {
                    return 0;
                }
            }
            Some(b'Q') => return 0,
            Some(b'w') => editor.save(),
            Some(_) => editor.help(),
            None => {}
        }
    }
}

struct Editor {
    path: &'static [u8],
    len: usize,
    dirty: bool,
}

impl Editor {
    fn banner(&self) {
        write_all(STDOUT, b"nkedit 0.1 - ");
        write_all(STDOUT, self.display_path());
        write_all(STDOUT, b"\ncommands: a append, p print, w write, c clear, q quit, Q force quit, h help\n");
    }

    fn display_path(&self) -> &[u8] {
        self.path.strip_suffix(&[0]).unwrap_or(self.path)
    }

    fn help(&self) {
        write_all(STDOUT, b"commands:\n");
        write_all(STDOUT, b"  a  append lines; finish append with a single '.' line\n");
        write_all(STDOUT, b"  p  print buffer\n");
        write_all(STDOUT, b"  w  write file\n");
        write_all(STDOUT, b"  c  clear buffer\n");
        write_all(STDOUT, b"  q  quit if saved\n");
        write_all(STDOUT, b"  Q  quit without saving\n");
    }

    fn load(&mut self) {
        let fd = sys_open(self.path.as_ptr(), 0) as i64;
        if fd < 0 {
            write_all(STDOUT, b"new file\n");
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
        if self.len == BUFFER_CAP {
            write_all(STDOUT, b"nkedit: file truncated in editor buffer\n");
        }
    }

    fn print(&self) {
        write_all(STDOUT, b"----- buffer -----\n");
        if self.len == 0 {
            write_all(STDOUT, b"(empty)\n");
        } else {
            unsafe {
                write_all(STDOUT, &BUFFER[..self.len]);
                if BUFFER[self.len - 1] != b'\n' {
                    write_all(STDOUT, b"\n");
                }
            }
        }
        write_all(STDOUT, b"------------------\n");
    }

    fn append_mode(&mut self) {
        write_all(STDOUT, b"append mode; single '.' line ends append\n");
        loop {
            write_all(STDOUT, b"> ");
            let line = read_line();
            let trimmed = trim(line);
            if trimmed == b"." {
                break;
            }
            if self.len + line.len() + 1 > BUFFER_CAP {
                write_all(STDERR, b"nkedit: buffer full\n");
                break;
            }
            unsafe {
                BUFFER[self.len..self.len + line.len()].copy_from_slice(line);
                self.len += line.len();
                BUFFER[self.len] = b'\n';
                self.len += 1;
            }
            self.dirty = true;
        }
    }

    fn clear(&mut self) {
        self.len = 0;
        self.dirty = true;
        write_all(STDOUT, b"buffer cleared\n");
    }

    fn save(&mut self) {
        let fd = sys_open(self.path.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC) as i64;
        if fd < 0 {
            write_all(STDERR, b"nkedit: open for write failed\n");
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
                write_all(STDERR, b"nkedit: write failed\n");
                return;
            }
            written += count as usize;
        }
        sys_close(fd as i32);
        self.dirty = false;
        write_all(STDOUT, b"saved\n");
    }
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

fn read_line() -> &'static [u8] {
    let mut len = 0usize;
    loop {
        let mut byte = [0u8; 1];
        let read = sys_read(STDIN, byte.as_mut_ptr(), 1) as i64;
        if read <= 0 {
            continue;
        }
        let ch = byte[0];
        if ch == b'\n' || ch == b'\r' {
            break;
        }
        if len < LINE_CAP - 1 {
            unsafe {
                LINE[len] = ch;
            }
            len += 1;
        }
    }
    unsafe { &LINE[..len] }
}

fn trim(mut input: &[u8]) -> &[u8] {
    while input.first() == Some(&b' ') || input.first() == Some(&b'\t') {
        input = &input[1..];
    }
    while input.last() == Some(&b' ') || input.last() == Some(&b'\t') {
        input = &input[..input.len() - 1];
    }
    input
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
    exit(101)
}
