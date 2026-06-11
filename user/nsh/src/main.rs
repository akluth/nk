#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_READ_KEY: u64 = 19;
const SYS_SHUTDOWN: u64 = 32;
const SYS_WRITE: u64 = 40;
const SYS_LS: u64 = 41;
const SYS_CAT: u64 = 42;
const SYS_IS_DIR: u64 = 43;

const LINE_CAP: usize = 256;
const CWD_CAP: usize = 256;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut shell = Shell::new();
    shell.write(b"nk shell ready\n");
    shell.write(b"type: ls, ls /bin, cat /hello.txt, cd /bin, pwd, version, shutdown\n");
    loop {
        shell.prompt();
        shell.read_line();
        shell.run_line();
    }
}

struct Shell {
    line: [u8; LINE_CAP],
    line_len: usize,
    cwd: [u8; CWD_CAP],
    cwd_len: usize,
}

impl Shell {
    const fn new() -> Self {
        let mut cwd = [0u8; CWD_CAP];
        cwd[0] = b'/';
        Self {
            line: [0; LINE_CAP],
            line_len: 0,
            cwd,
            cwd_len: 1,
        }
    }

    fn prompt(&self) {
        self.write(b"# ");
    }

    fn read_line(&mut self) {
        self.line_len = 0;
        loop {
            let key = syscall0(SYS_READ_KEY) as u8;
            if key == 0 {
                syscall0(SYS_YIELD);
                continue;
            }
            match key {
                b'\n' | b'\r' => {
                    self.write(b"\n");
                    return;
                }
                8 | 127 => {
                    if self.line_len > 0 {
                        self.line_len -= 1;
                        self.write(&[8]);
                    }
                }
                byte if byte >= 0x20 && self.line_len < LINE_CAP - 1 => {
                    self.line[self.line_len] = byte;
                    self.line_len += 1;
                    self.write(&[byte]);
                }
                _ => {}
            }
        }
    }

    fn run_line(&mut self) {
        let line = trim(&self.line[..self.line_len]);
        if line.is_empty() {
            return;
        }
        let (cmd, arg) = split_first(line);
        match cmd {
            b"help" => self.write(b"commands: ls cat cd pwd version shutdown bash\n"),
            b"version" => self.write(b"nk userspace shell 0.1\n"),
            b"pwd" => {
                self.write(&self.cwd[..self.cwd_len]);
                self.write(b"\n");
            }
            b"ls" => {
                let path = self.resolve_arg(arg.unwrap_or(b"."));
                if syscall2(SYS_LS, path.as_ptr() as u64, path.len() as u64) != 0 {
                    self.write(b"ls failed\n");
                }
            }
            b"cat" => {
                let Some(arg) = arg else {
                    self.write(b"cat: missing operand\n");
                    return;
                };
                let path = self.resolve_arg(arg);
                if syscall2(SYS_CAT, path.as_ptr() as u64, path.len() as u64) != 0 {
                    self.write(b"cat failed\n");
                }
            }
            b"cd" => {
                let path = self.resolve_arg(arg.unwrap_or(b"/"));
                if syscall2(SYS_IS_DIR, path.as_ptr() as u64, path.len() as u64) == 0 {
                    self.cwd[..path.len()].copy_from_slice(path.as_slice());
                    self.cwd_len = path.len();
                } else {
                    self.write(b"cd: no such directory\n");
                }
            }
            b"shutdown" => {
                syscall0(SYS_SHUTDOWN);
            }
            b"bash" => self.write(b"bash is available at /bin/bash but not the default shell now\n"),
            _ => self.write(b"unknown command\n"),
        }
    }

    fn resolve_arg(&self, arg: &[u8]) -> Path {
        let mut out = Path::new();
        if arg == b"." {
            out.push(&self.cwd[..self.cwd_len]);
        } else if arg.starts_with(b"/") {
            out.push(arg);
        } else {
            out.push(&self.cwd[..self.cwd_len]);
            if self.cwd_len > 1 {
                out.push(b"/");
            }
            out.push(arg);
        }
        out.canonicalize();
        out
    }

    fn write(&self, bytes: &[u8]) {
        syscall2(SYS_WRITE, bytes.as_ptr() as u64, bytes.len() as u64);
    }
}

struct Path {
    bytes: [u8; CWD_CAP],
    len: usize,
}

impl Path {
    const fn new() -> Self {
        Self {
            bytes: [0; CWD_CAP],
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        let count = bytes.len().min(CWD_CAP.saturating_sub(self.len));
        self.bytes[self.len..self.len + count].copy_from_slice(&bytes[..count]);
        self.len += count;
    }

    fn canonicalize(&mut self) {
        if self.len == 0 {
            self.bytes[0] = b'/';
            self.len = 1;
            return;
        }
        while self.len > 1 && self.bytes[self.len - 1] == b'/' {
            self.len -= 1;
        }
    }

    fn as_ptr(&self) -> *const u8 {
        self.bytes.as_ptr()
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    fn len(&self) -> usize {
        self.len
    }
}

fn trim(mut input: &[u8]) -> &[u8] {
    while input.first() == Some(&b' ') {
        input = &input[1..];
    }
    while input.last() == Some(&b' ') {
        input = &input[..input.len() - 1];
    }
    input
}

fn split_first(input: &[u8]) -> (&[u8], Option<&[u8]>) {
    for index in 0..input.len() {
        if input[index] == b' ' {
            let cmd = trim(&input[..index]);
            let arg = trim(&input[index + 1..]);
            return (cmd, if arg.is_empty() { None } else { Some(arg) });
        }
    }
    (input, None)
}

fn syscall0(id: u64) -> u64 {
    let out;
    unsafe {
        asm!("int 0x80", inlateout("rax") id => out, options(nostack, preserves_flags));
    }
    out
}

fn syscall2(id: u64, a: u64, b: u64) -> u64 {
    let out;
    unsafe {
        asm!(
            "int 0x80",
            inlateout("rax") id => out,
            in("rdi") a,
            in("rsi") b,
            options(nostack, preserves_flags)
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
