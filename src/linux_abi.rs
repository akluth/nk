use core::cell::UnsafeCell;

use crate::{fat32, scheduler, serial, services};

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_FSTAT: u64 = 5;
const SYS_LSEEK: u64 = 8;
const SYS_BRK: u64 = 12;
const SYS_GETPID: u64 = 39;
const SYS_UNAME: u64 = 63;
const SYS_EXIT: u64 = 60;
const SYS_OPENAT: u64 = 257;
const SYS_EXIT_GROUP: u64 = 231;
const SYS_NEWFSTATAT: u64 = 262;

const EBADF: i64 = -9;
const ENOENT: i64 = -2;
const EFAULT: i64 = -14;
const EINVAL: i64 = -22;
const ENOSYS: i64 = -38;

const USER_BRK_START: u64 = 0x0000_0000_4017_0000;
const USER_BRK_END: u64 = 0x0000_0000_4017_f000;

struct OpenFile {
    data: Option<&'static [u8]>,
    offset: usize,
}

impl OpenFile {
    const fn empty() -> Self {
        Self {
            data: None,
            offset: 0,
        }
    }
}

struct GlobalOpenFile(UnsafeCell<OpenFile>);

unsafe impl Sync for GlobalOpenFile {}

static FILE3: GlobalOpenFile = GlobalOpenFile(UnsafeCell::new(OpenFile::empty()));
static mut PROGRAM_BREAK: u64 = USER_BRK_START;

pub fn handle_syscall(frame: &mut scheduler::TrapFrame) -> bool {
    match frame.rax {
        SYS_READ => {
            frame.rax = read(frame.rdi as i32, frame.rsi as *mut u8, frame.rdx as usize) as u64;
            true
        }
        SYS_WRITE => {
            frame.rax = write(frame.rdi as i32, frame.rsi as *const u8, frame.rdx as usize) as u64;
            true
        }
        SYS_OPEN => {
            frame.rax = open(frame.rdi as *const u8) as u64;
            true
        }
        SYS_CLOSE => {
            frame.rax = close(frame.rdi as i32) as u64;
            true
        }
        SYS_FSTAT => {
            frame.rax = stat_fd(frame.rdi as i32, frame.rsi as *mut u8) as u64;
            true
        }
        SYS_LSEEK => {
            frame.rax = lseek(frame.rdi as i32, frame.rsi as i64, frame.rdx as i32) as u64;
            true
        }
        SYS_BRK => {
            frame.rax = brk(frame.rdi) as u64;
            true
        }
        SYS_GETPID => {
            frame.rax = 4;
            true
        }
        SYS_UNAME => {
            frame.rax = uname(frame.rdi as *mut u8) as u64;
            true
        }
        SYS_EXIT => {
            serial::write_line("nk: linux cat exited");
            let _scheduled = scheduler::exit_current_user(frame);
            true
        }
        SYS_EXIT_GROUP => {
            serial::write_line("nk: linux task exited");
            let _scheduled = scheduler::exit_current_user(frame);
            true
        }
        SYS_OPENAT => {
            frame.rax = open(frame.rsi as *const u8) as u64;
            true
        }
        SYS_NEWFSTATAT => {
            frame.rax = stat_path(frame.rsi as *const u8, frame.rdx as *mut u8) as u64;
            true
        }
        _ => {
            frame.rax = ENOSYS as u64;
            true
        }
    }
}

fn open(path: *const u8) -> i64 {
    let Some(short_name) = path_to_fat_name(path) else {
        return ENOENT;
    };

    let Some(data) = fat32::read_file(&short_name) else {
        return ENOENT;
    };

    unsafe {
        let file = &mut *FILE3.0.get();
        file.data = Some(data);
        file.offset = 0;
    }
    3
}

fn read(fd: i32, buffer: *mut u8, len: usize) -> i64 {
    if buffer.is_null() || len == 0 {
        return 0;
    }
    if fd == 0 {
        return 0;
    }
    if fd != 3 {
        return EBADF;
    }

    unsafe {
        let file = &mut *FILE3.0.get();
        let Some(data) = file.data else {
            return EBADF;
        };
        if file.offset >= data.len() {
            return 0;
        }

        let count = len.min(data.len() - file.offset);
        core::ptr::copy_nonoverlapping(data.as_ptr().add(file.offset), buffer, count);
        file.offset += count;
        count as i64
    }
}

fn write(fd: i32, buffer: *const u8, len: usize) -> i64 {
    if buffer.is_null() {
        return EFAULT;
    }
    if fd != 1 && fd != 2 {
        return EBADF;
    }
    if len > 4096 {
        return EINVAL;
    }

    let bytes = unsafe { core::slice::from_raw_parts(buffer, len) };
    for byte in bytes {
        serial::write_str_byte(*byte);
    }
    services::gui::console_write(bytes);
    len as i64
}

fn close(fd: i32) -> i64 {
    if fd == 3 {
        unsafe {
            let file = &mut *FILE3.0.get();
            file.data = None;
            file.offset = 0;
        }
        0
    } else {
        EBADF
    }
}

fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    if fd != 3 {
        return EBADF;
    }

    unsafe {
        let file = &mut *FILE3.0.get();
        let Some(data) = file.data else {
            return EBADF;
        };
        let base = match whence {
            0 => 0i64,
            1 => file.offset as i64,
            2 => data.len() as i64,
            _ => return EINVAL,
        };
        let next = base.saturating_add(offset);
        if next < 0 {
            return EINVAL;
        }
        file.offset = (next as usize).min(data.len());
        file.offset as i64
    }
}

fn brk(request: u64) -> i64 {
    unsafe {
        if request == 0 {
            return PROGRAM_BREAK as i64;
        }
        if (USER_BRK_START..=USER_BRK_END).contains(&request) {
            PROGRAM_BREAK = request;
        }
        PROGRAM_BREAK as i64
    }
}

fn stat_fd(fd: i32, stat_buf: *mut u8) -> i64 {
    if fd != 0 && fd != 1 && fd != 2 && fd != 3 {
        return EBADF;
    }
    write_fake_stat(stat_buf)
}

fn stat_path(path: *const u8, stat_buf: *mut u8) -> i64 {
    let Some(short_name) = path_to_fat_name(path) else {
        return ENOENT;
    };
    if fat32::read_file(&short_name).is_none() {
        return ENOENT;
    }
    write_fake_stat(stat_buf)
}

fn write_fake_stat(stat_buf: *mut u8) -> i64 {
    if stat_buf.is_null() {
        return EFAULT;
    }
    unsafe {
        for index in 0..144 {
            *stat_buf.add(index) = 0;
        }
        let mode = 0o100444u32.to_le_bytes();
        for (index, byte) in mode.iter().enumerate() {
            *stat_buf.add(24 + index) = *byte;
        }
    }
    0
}

fn uname(buffer: *mut u8) -> i64 {
    if buffer.is_null() {
        return EFAULT;
    }
    write_uts_field(buffer, 0, b"Linux");
    write_uts_field(buffer, 65, b"nk");
    write_uts_field(buffer, 130, b"0.1.0");
    write_uts_field(buffer, 195, b"nk-posix");
    write_uts_field(buffer, 260, b"x86_64");
    0
}

fn write_uts_field(buffer: *mut u8, offset: usize, value: &[u8]) {
    unsafe {
        for index in 0..65 {
            *buffer.add(offset + index) = 0;
        }
        for (index, byte) in value.iter().enumerate() {
            *buffer.add(offset + index) = *byte;
        }
    }
}

fn path_to_fat_name(path: *const u8) -> Option<[u8; 11]> {
    if path.is_null() {
        return None;
    }

    let mut raw = [0u8; 64];
    let mut len = 0usize;
    unsafe {
        while len < raw.len() {
            let byte = *path.add(len);
            if byte == 0 {
                break;
            }
            raw[len] = byte;
            len += 1;
        }
    }

    let mut start = 0usize;
    for index in 0..len {
        if raw[index] == b'/' {
            start = index + 1;
        }
    }

    let name = &raw[start..len];
    let mut out = [b' '; 11];
    let mut pos = 0usize;
    let mut ext = 8usize;
    let mut in_ext = false;
    for byte in name {
        if *byte == b'.' {
            in_ext = true;
            continue;
        }

        let upper = if byte.is_ascii_lowercase() {
            *byte - b'a' + b'A'
        } else {
            *byte
        };
        if in_ext {
            if ext >= 11 {
                return None;
            }
            out[ext] = upper;
            ext += 1;
        } else {
            if pos >= 8 {
                return None;
            }
            out[pos] = upper;
            pos += 1;
        }
    }

    Some(out)
}
