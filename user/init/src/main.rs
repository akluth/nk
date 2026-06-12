#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_YIELD: u64 = 0;
const SYS_WRITE: u64 = 40;
const SYS_EXEC_NATIVE: u64 = 46;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    write(b"init: starting /bin/nsh\n");
    let path = b"/bin/nsh";
    if syscall2(SYS_EXEC_NATIVE, path.as_ptr() as u64, path.len() as u64) != 0 {
        write(b"init: exec /bin/nsh failed\n");
    }
    loop {
        syscall0(SYS_YIELD);
    }
}

fn write(bytes: &[u8]) {
    syscall2(SYS_WRITE, bytes.as_ptr() as u64, bytes.len() as u64);
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

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        syscall0(SYS_YIELD);
    }
}
