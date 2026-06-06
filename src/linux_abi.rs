use core::cell::UnsafeCell;

use crate::{arch, fat32, keyboard, scheduler, serial, services};

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_STAT: u64 = 4;
const SYS_FSTAT: u64 = 5;
const SYS_MMAP: u64 = 9;
const SYS_MUNMAP: u64 = 11;
const SYS_LSEEK: u64 = 8;
const SYS_RT_SIGACTION: u64 = 13;
const SYS_RT_SIGPROCMASK: u64 = 14;
const SYS_IOCTL: u64 = 16;
const SYS_WRITEV: u64 = 20;
const SYS_ACCESS: u64 = 21;
const SYS_FCNTL: u64 = 72;
const SYS_GETCWD: u64 = 79;
const SYS_CHDIR: u64 = 80;
const SYS_GETTIMEOFDAY: u64 = 96;
const SYS_GETRESUID: u64 = 118;
const SYS_GETRESGID: u64 = 120;
const SYS_READLINK: u64 = 89;
const SYS_GETUID: u64 = 102;
const SYS_GETGID: u64 = 104;
const SYS_GETEUID: u64 = 107;
const SYS_GETEGID: u64 = 108;
const SYS_GETPPID: u64 = 110;
const SYS_ARCH_PRCTL: u64 = 158;
const SYS_SET_TID_ADDRESS: u64 = 218;
const SYS_BRK: u64 = 12;
const SYS_GETPID: u64 = 39;
const SYS_WAIT4: u64 = 61;
const SYS_UNAME: u64 = 63;
const SYS_EXIT: u64 = 60;
const SYS_CLOCK_GETTIME: u64 = 228;
const SYS_OPENAT: u64 = 257;
const SYS_EXIT_GROUP: u64 = 231;
const SYS_SET_ROBUST_LIST: u64 = 273;
const SYS_NEWFSTATAT: u64 = 262;
const SYS_PRLIMIT64: u64 = 302;
const SYS_GETRANDOM: u64 = 318;

const ARCH_SET_FS: u64 = 0x1002;
const ARCH_GET_FS: u64 = 0x1003;
const IA32_FS_BASE: u32 = 0xc000_0100;

const EBADF: i64 = -9;
const ECHILD: i64 = -10;
const ENOENT: i64 = -2;
const EFAULT: i64 = -14;
const EINVAL: i64 = -22;
const ENOSYS: i64 = -38;

const USER_MMAP_START: u64 = 0x0000_0000_4010_0000;
const USER_MMAP_END: u64 = 0x0000_0000_4017_0000;
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
static mut MMAP_CURSOR: u64 = USER_MMAP_START;
static mut PROGRAM_BREAK: u64 = USER_BRK_START;
static mut UNKNOWN_LOGS: u64 = 0;

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
        SYS_STAT => {
            frame.rax = stat_path(frame.rdi as *const u8, frame.rsi as *mut u8) as u64;
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
        SYS_MMAP => {
            frame.rax = mmap(frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8 as i64) as u64;
            true
        }
        SYS_MUNMAP => {
            frame.rax = 0;
            true
        }
        SYS_RT_SIGACTION | SYS_RT_SIGPROCMASK => {
            frame.rax = 0;
            true
        }
        SYS_IOCTL => {
            frame.rax = ioctl(frame.rdi as i32, frame.rsi, frame.rdx as *mut u8) as u64;
            true
        }
        SYS_WRITEV => {
            frame.rax = writev(frame.rdi as i32, frame.rsi as *const u8, frame.rdx as usize) as u64;
            true
        }
        SYS_ACCESS => {
            frame.rax = access(frame.rdi as *const u8) as u64;
            true
        }
        SYS_FCNTL => {
            frame.rax = fcntl(frame.rdi as i32, frame.rsi, frame.rdx) as u64;
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
        SYS_WAIT4 => {
            frame.rax = ECHILD as u64;
            true
        }
        SYS_GETCWD => {
            frame.rax = getcwd(frame.rdi as *mut u8, frame.rsi as usize) as u64;
            true
        }
        SYS_GETTIMEOFDAY => {
            frame.rax = gettimeofday(frame.rdi as *mut u8) as u64;
            true
        }
        SYS_GETRESUID | SYS_GETRESGID => {
            frame.rax = write_three_ids(
                frame.rdi as *mut u32,
                frame.rsi as *mut u32,
                frame.rdx as *mut u32,
            ) as u64;
            true
        }
        SYS_CHDIR => {
            frame.rax = chdir(frame.rdi as *const u8) as u64;
            true
        }
        SYS_READLINK => {
            frame.rax = readlink(
                frame.rdi as *const u8,
                frame.rsi as *mut u8,
                frame.rdx as usize,
            ) as u64;
            true
        }
        SYS_GETUID | SYS_GETGID | SYS_GETEUID | SYS_GETEGID => {
            frame.rax = 0;
            true
        }
        SYS_GETPPID => {
            frame.rax = 1;
            true
        }
        SYS_ARCH_PRCTL => {
            frame.rax = arch_prctl(frame.rdi, frame.rsi) as u64;
            true
        }
        SYS_SET_ROBUST_LIST => {
            frame.rax = 0;
            true
        }
        SYS_SET_TID_ADDRESS => {
            frame.rax = 4;
            true
        }
        SYS_EXIT => {
            serial::write_line("nk: linux task exited");
            let _scheduled = scheduler::exit_current_user(frame);
            true
        }
        SYS_EXIT_GROUP => {
            serial::write_line("nk: linux task exited");
            let _scheduled = scheduler::exit_current_user(frame);
            true
        }
        SYS_CLOCK_GETTIME => {
            frame.rax = clock_gettime(frame.rsi as *mut u8) as u64;
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
        SYS_PRLIMIT64 => {
            frame.rax = 0;
            true
        }
        SYS_GETRANDOM => {
            frame.rax = getrandom(frame.rdi as *mut u8, frame.rsi as usize) as u64;
            true
        }
        _ => {
            log_unknown_syscall(frame.rax);
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
        return read_stdin(buffer, len);
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

fn read_stdin(buffer: *mut u8, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }

    let key = loop {
        if let Some(key) = keyboard::pop_key() {
            break key as u8;
        }
        crate::arch::halt();
    };

    unsafe {
        *buffer = key;
    }
    1
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

fn writev(fd: i32, iov: *const u8, count: usize) -> i64 {
    if iov.is_null() {
        return EFAULT;
    }
    if count > 16 {
        return EINVAL;
    }

    let mut total = 0i64;
    for index in 0..count {
        let base_offset = index * 16;
        let base = unsafe { read_user_u64(iov.add(base_offset)) } as *const u8;
        let len = unsafe { read_user_u64(iov.add(base_offset + 8)) } as usize;
        let written = write(fd, base, len);
        if written < 0 {
            return written;
        }
        total += written;
    }
    total
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

fn fcntl(fd: i32, command: u64, _arg: u64) -> i64 {
    if fd != 0 && fd != 1 && fd != 2 && fd != 3 {
        return EBADF;
    }
    match command {
        1 | 2 | 3 => 0,
        _ => 0,
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

fn mmap(address: u64, len: u64, _prot: u64, flags: u64, fd: i64) -> i64 {
    const MAP_FIXED: u64 = 0x10;
    const MAP_ANONYMOUS: u64 = 0x20;

    if len == 0 {
        return EINVAL;
    }
    if fd != -1 && flags & MAP_ANONYMOUS == 0 {
        return EINVAL;
    }

    let aligned_len = (len + 4095) & !4095;
    unsafe {
        let base = if flags & MAP_FIXED != 0 && address != 0 {
            address
        } else {
            let next = (MMAP_CURSOR + 4095) & !4095;
            MMAP_CURSOR = next.saturating_add(aligned_len);
            next
        };
        if base < USER_MMAP_START || base.saturating_add(aligned_len) > USER_MMAP_END {
            return -12;
        }
        base as i64
    }
}

fn stat_fd(fd: i32, stat_buf: *mut u8) -> i64 {
    if fd != 0 && fd != 1 && fd != 2 && fd != 3 {
        return EBADF;
    }
    write_fake_stat(stat_buf)
}

fn stat_path(path: *const u8, stat_buf: *mut u8) -> i64 {
    if path_is_root_or_dot(path) {
        return write_fake_stat(stat_buf);
    }
    let Some(short_name) = path_to_fat_name(path) else {
        return ENOENT;
    };
    if fat32::read_file(&short_name).is_none() {
        return ENOENT;
    }
    write_fake_stat(stat_buf)
}

fn access(path: *const u8) -> i64 {
    if path_is_root_or_dot(path) {
        return 0;
    }
    let Some(short_name) = path_to_fat_name(path) else {
        return ENOENT;
    };
    if fat32::read_file(&short_name).is_some() {
        0
    } else {
        ENOENT
    }
}

fn ioctl(fd: i32, request: u64, argp: *mut u8) -> i64 {
    if fd != 0 && fd != 1 && fd != 2 {
        return EBADF;
    }

    match request {
        0x5401 => write_termios(argp),
        0x5402 | 0x5403 | 0x5404 => 0,
        0x5405 | 0x5406 | 0x5413 => write_winsize(argp),
        _ => 0,
    }
}

fn write_termios(argp: *mut u8) -> i64 {
    if argp.is_null() {
        return EFAULT;
    }
    unsafe {
        for index in 0..64 {
            *argp.add(index) = 0;
        }
        let iflag = 0u32.to_le_bytes();
        let oflag = 1u32.to_le_bytes();
        let cflag = 0xbf_u32.to_le_bytes();
        let lflag = 0x8a3b_u32.to_le_bytes();
        for (index, byte) in iflag.iter().enumerate() {
            *argp.add(index) = *byte;
        }
        for (index, byte) in oflag.iter().enumerate() {
            *argp.add(4 + index) = *byte;
        }
        for (index, byte) in cflag.iter().enumerate() {
            *argp.add(8 + index) = *byte;
        }
        for (index, byte) in lflag.iter().enumerate() {
            *argp.add(12 + index) = *byte;
        }
    }
    0
}

fn write_winsize(argp: *mut u8) -> i64 {
    if argp.is_null() {
        return EFAULT;
    }
    unsafe {
        let rows = 24u16.to_le_bytes();
        let cols = 80u16.to_le_bytes();
        *argp.add(0) = rows[0];
        *argp.add(1) = rows[1];
        *argp.add(2) = cols[0];
        *argp.add(3) = cols[1];
        for index in 4..8 {
            *argp.add(index) = 0;
        }
    }
    0
}

fn getcwd(buffer: *mut u8, len: usize) -> i64 {
    if buffer.is_null() {
        return EFAULT;
    }
    if len < 2 {
        return EINVAL;
    }
    unsafe {
        *buffer = b'/';
        *buffer.add(1) = 0;
    }
    buffer as i64
}

fn gettimeofday(buffer: *mut u8) -> i64 {
    if buffer.is_null() {
        return 0;
    }
    unsafe {
        write_user_i64(buffer, 1);
        write_user_i64(buffer.add(8), 0);
    }
    0
}

fn clock_gettime(buffer: *mut u8) -> i64 {
    if buffer.is_null() {
        return EFAULT;
    }
    unsafe {
        write_user_i64(buffer, 1);
        write_user_i64(buffer.add(8), 0);
    }
    0
}

fn write_three_ids(first: *mut u32, second: *mut u32, third: *mut u32) -> i64 {
    unsafe {
        if !first.is_null() {
            *first = 0;
        }
        if !second.is_null() {
            *second = 0;
        }
        if !third.is_null() {
            *third = 0;
        }
    }
    0
}

fn readlink(path: *const u8, buffer: *mut u8, len: usize) -> i64 {
    if path.is_null() || buffer.is_null() {
        return EFAULT;
    }
    if !path_equals(path, b"/proc/self/exe") && !path_equals(path, b"/proc/curproc/file") {
        return ENOENT;
    }

    let value = b"/bash";
    let count = len.min(value.len());
    unsafe {
        core::ptr::copy_nonoverlapping(value.as_ptr(), buffer, count);
    }
    count as i64
}

fn chdir(path: *const u8) -> i64 {
    if path_is_root_or_dot(path) {
        0
    } else {
        ENOENT
    }
}

fn getrandom(buffer: *mut u8, len: usize) -> i64 {
    if buffer.is_null() {
        return EFAULT;
    }
    let count = len.min(64);
    unsafe {
        for index in 0..count {
            *buffer.add(index) = (index as u8).wrapping_mul(37).wrapping_add(11);
        }
    }
    count as i64
}

fn arch_prctl(code: u64, address: u64) -> i64 {
    match code {
        ARCH_SET_FS => unsafe {
            arch::wrmsr(IA32_FS_BASE, address);
            0
        },
        ARCH_GET_FS => {
            if address == 0 {
                return EFAULT;
            }
            let value = unsafe { arch::rdmsr(IA32_FS_BASE) };
            unsafe {
                *(address as *mut u64) = value;
            }
            0
        }
        _ => EINVAL,
    }
}

fn log_unknown_syscall(id: u64) {
    unsafe {
        UNKNOWN_LOGS = UNKNOWN_LOGS.wrapping_add(1);
        if UNKNOWN_LOGS <= 32 || UNKNOWN_LOGS % 128 == 0 {
            serial::write_str("nk: unknown linux syscall id=");
            serial::write_hex_u64(id);
            serial::write_line("");
        }
    }
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

fn path_is_root_or_dot(path: *const u8) -> bool {
    if path.is_null() {
        return false;
    }

    unsafe {
        match (*path, *path.add(1), *path.add(2)) {
            (b'/', 0, _) | (b'.', 0, _) | (b'.', b'/', 0) => true,
            _ => false,
        }
    }
}

fn path_equals(path: *const u8, value: &[u8]) -> bool {
    if path.is_null() {
        return false;
    }
    unsafe {
        for (index, expected) in value.iter().enumerate() {
            if *path.add(index) != *expected {
                return false;
            }
        }
        *path.add(value.len()) == 0
    }
}

unsafe fn read_user_u64(ptr: *const u8) -> u64 {
    let mut bytes = [0u8; 8];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = *ptr.add(index);
    }
    u64::from_le_bytes(bytes)
}

unsafe fn write_user_i64(ptr: *mut u8, value: i64) {
    for (index, byte) in value.to_le_bytes().iter().enumerate() {
        *ptr.add(index) = *byte;
    }
}
