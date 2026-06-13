#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_WRITE: u64 = 1;
const SYS_CLOSE: u64 = 3;
const SYS_PIPE: u64 = 22;
const SYS_DUP2: u64 = 33;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_WAIT4: u64 = 61;
const SYS_EXIT: u64 = 60;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut fds = [0i32; 2];
    if syscall1(SYS_PIPE, fds.as_mut_ptr() as u64) != 0 {
        write_all(2, b"pipecheck: pipe failed\n");
        exit(1);
    }

    let writer = syscall0(SYS_FORK) as i64;
    if writer == 0 {
        let _ = syscall2(SYS_DUP2, fds[1] as u64, 1);
        close_pair(&fds);
        exec_echo();
        write_all(2, b"pipecheck: exec echo failed\n");
        exit(1);
    }
    if writer < 0 {
        write_all(2, b"pipecheck: fork writer failed\n");
        exit(1);
    }

    let reader = syscall0(SYS_FORK) as i64;
    if reader == 0 {
        let _ = syscall2(SYS_DUP2, fds[0] as u64, 0);
        close_pair(&fds);
        exec_cat();
        write_all(2, b"pipecheck: exec cat failed\n");
        exit(1);
    }
    if reader < 0 {
        write_all(2, b"pipecheck: fork reader failed\n");
        exit(1);
    }

    close_pair(&fds);
    let _ = syscall4(SYS_WAIT4, writer as u64, 0, 0, 0);
    let _ = syscall4(SYS_WAIT4, reader as u64, 0, 0, 0);
    write_all(1, b"pipecheck: done\n");
    exit(0);
}

fn exec_echo() {
    static PATH: &[u8] = b"/bin/echo\0";
    static ARG0: &[u8] = b"echo\0";
    static ARG1: &[u8] = b"pipe-ok\0";
    let argv = [ARG0.as_ptr() as u64, ARG1.as_ptr() as u64, 0];
    let _ = syscall2(SYS_EXECVE, PATH.as_ptr() as u64, argv.as_ptr() as u64);
}

fn exec_cat() {
    static PATH: &[u8] = b"/bin/cat\0";
    static ARG0: &[u8] = b"cat\0";
    let argv = [ARG0.as_ptr() as u64, 0];
    let _ = syscall2(SYS_EXECVE, PATH.as_ptr() as u64, argv.as_ptr() as u64);
}

fn close_pair(fds: &[i32; 2]) {
    let _ = syscall1(SYS_CLOSE, fds[0] as u64);
    let _ = syscall1(SYS_CLOSE, fds[1] as u64);
}

fn write_all(fd: i32, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let written = syscall3(SYS_WRITE, fd as u64, bytes.as_ptr() as u64, bytes.len() as u64);
        if written as i64 <= 0 {
            return;
        }
        bytes = &bytes[written as usize..];
    }
}

fn exit(code: i32) -> ! {
    let _ = syscall1(SYS_EXIT, code as u64);
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}

fn syscall0(id: u64) -> u64 {
    let out;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") id => out,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    out
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

fn syscall4(id: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let out;
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") id => out,
            in("rdi") a,
            in("rsi") b,
            in("rdx") c,
            in("r10") d,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    out
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(1)
}
