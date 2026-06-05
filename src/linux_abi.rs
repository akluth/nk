use core::cell::UnsafeCell;

use crate::{fat32, scheduler, serial, services};

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_EXIT: u64 = 60;

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
        SYS_EXIT => {
            serial::write_line("nk: linux cat exited");
            let _scheduled = scheduler::exit_current_user(frame);
            true
        }
        _ => false,
    }
}

fn open(path: *const u8) -> i64 {
    let Some(short_name) = path_to_fat_name(path) else {
        return -2;
    };

    let Some(data) = fat32::read_file(&short_name) else {
        return -2;
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
        return -9;
    }

    unsafe {
        let file = &mut *FILE3.0.get();
        let Some(data) = file.data else {
            return -9;
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
        return -14;
    }
    if fd != 1 && fd != 2 {
        return -9;
    }
    if len > 4096 {
        return -22;
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
        -9
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
