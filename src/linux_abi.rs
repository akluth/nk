use core::cell::UnsafeCell;

use crate::{arch, keyboard, memory, nkfs, scheduler, serial, services, userland};

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_STAT: u64 = 4;
const SYS_FSTAT: u64 = 5;
const SYS_LSTAT: u64 = 6;
const SYS_POLL: u64 = 7;
const SYS_MMAP: u64 = 9;
const SYS_MPROTECT: u64 = 10;
const SYS_MUNMAP: u64 = 11;
const SYS_LSEEK: u64 = 8;
const SYS_RT_SIGACTION: u64 = 13;
const SYS_RT_SIGPROCMASK: u64 = 14;
const SYS_IOCTL: u64 = 16;
const SYS_PWRITE64: u64 = 18;
const SYS_WRITEV: u64 = 20;
const SYS_PIPE: u64 = 22;
const SYS_ACCESS: u64 = 21;
const SYS_MADVISE: u64 = 28;
const SYS_FSYNC: u64 = 74;
const SYS_FDATASYNC: u64 = 75;
const SYS_TRUNCATE: u64 = 76;
const SYS_FTRUNCATE: u64 = 77;
const SYS_FCNTL: u64 = 72;
const SYS_GETCWD: u64 = 79;
const SYS_CHDIR: u64 = 80;
const SYS_UNLINK: u64 = 87;
const SYS_GETTIMEOFDAY: u64 = 96;
const SYS_GETRLIMIT: u64 = 97;
const SYS_GETRESUID: u64 = 118;
const SYS_GETRESGID: u64 = 120;
const SYS_SIGALTSTACK: u64 = 131;
const SYS_READLINK: u64 = 89;
const SYS_GETUID: u64 = 102;
const SYS_GETGID: u64 = 104;
const SYS_GETEUID: u64 = 107;
const SYS_GETEGID: u64 = 108;
const SYS_GETPPID: u64 = 110;
const SYS_TKILL: u64 = 200;
const SYS_ARCH_PRCTL: u64 = 158;
const SYS_GETXATTR: u64 = 191;
const SYS_LGETXATTR: u64 = 192;
const SYS_FGETXATTR: u64 = 193;
const SYS_LISTXATTR: u64 = 194;
const SYS_LLISTXATTR: u64 = 195;
const SYS_FLISTXATTR: u64 = 196;
const SYS_SET_TID_ADDRESS: u64 = 218;
const SYS_GETDENTS64: u64 = 217;
const SYS_BRK: u64 = 12;
const SYS_GETPID: u64 = 39;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_WAIT4: u64 = 61;
const SYS_UNAME: u64 = 63;
const SYS_EXIT: u64 = 60;
const SYS_CLOCK_GETTIME: u64 = 228;
const SYS_OPENAT: u64 = 257;
const SYS_UNLINKAT: u64 = 263;
const SYS_FACCESSAT: u64 = 269;
const SYS_SPLICE: u64 = 275;
const SYS_PIPE2: u64 = 293;
const SYS_EXIT_GROUP: u64 = 231;
const SYS_SET_ROBUST_LIST: u64 = 273;
const SYS_NEWFSTATAT: u64 = 262;
const SYS_PRLIMIT64: u64 = 302;
const SYS_GETRANDOM: u64 = 318;
const SYS_STATX: u64 = 332;
const SYS_RSEQ: u64 = 439;

const ARCH_SET_FS: u64 = 0x1002;
const ARCH_GET_FS: u64 = 0x1003;
const IA32_FS_BASE: u32 = 0xc000_0100;

const EBADF: i64 = -9;
const ECHILD: i64 = -10;
const ENOENT: i64 = -2;
const EFAULT: i64 = -14;
const EINVAL: i64 = -22;
const EPIPE: i64 = -32;
const ENOSYS: i64 = -38;
const EAGAIN: i64 = -11;
const EMFILE: i64 = -24;
const ENODATA: i64 = -61;

const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;
const O_CREAT: u64 = 0o100;
const O_TRUNC: u64 = 0o1000;

const USER_BRK_START: u64 = 0x0000_0000_4100_0000;
const USER_BRK_END: u64 = 0x0000_0000_4140_0000;
const USER_MMAP_START: u64 = 0x0000_0000_4140_0000;
const USER_MMAP_END: u64 = 0x0000_0000_41f0_0000;

#[derive(Clone, Copy)]
struct OpenFile {
    data: Option<&'static [u8]>,
    offset: usize,
    is_dir: bool,
    writable: bool,
    ram_index: usize,
    mode: u32,
    path: [u8; 256],
    path_len: usize,
}

impl OpenFile {
    const fn empty() -> Self {
        Self {
            data: None,
            offset: 0,
            is_dir: false,
            writable: false,
            ram_index: usize::MAX,
            mode: 0,
            path: [0; 256],
            path_len: 0,
        }
    }
}

struct GlobalOpenFiles(UnsafeCell<[[OpenFile; MAX_OPEN_FILES]; scheduler::USER_TASKS]>);

unsafe impl Sync for GlobalOpenFiles {}

const FIRST_USER_FD: i32 = 3;
const MAX_OPEN_FILES: usize = 16;
const FD_BUFFER_CAP: usize = 256 * 1024;
static OPEN_FILES: GlobalOpenFiles = GlobalOpenFiles(UnsafeCell::new(
    [[OpenFile::empty(); MAX_OPEN_FILES]; scheduler::USER_TASKS],
));
static mut FD_BUFFERS: [[[u8; FD_BUFFER_CAP]; MAX_OPEN_FILES]; scheduler::USER_TASKS] =
    [[[0; FD_BUFFER_CAP]; MAX_OPEN_FILES]; scheduler::USER_TASKS];
const INPUT_LINE_CAP: usize = 1024;
const READY_INPUT_CAP: usize = 2048;
static mut INPUT_LINE: [u8; INPUT_LINE_CAP] = [0; INPUT_LINE_CAP];
static mut INPUT_LINE_LEN: usize = 0;
static mut READY_INPUT: [u8; READY_INPUT_CAP] = [0; READY_INPUT_CAP];
static mut READY_READ: usize = 0;
static mut READY_WRITE: usize = 0;
static mut TASK_CWDS: [[u8; 256]; scheduler::USER_TASKS] = [[0; 256]; scheduler::USER_TASKS];
static mut TASK_CWD_LENS: [usize; scheduler::USER_TASKS] = [0; scheduler::USER_TASKS];
static mut MMAP_CURSORS: [u64; scheduler::USER_TASKS] = [USER_MMAP_START; scheduler::USER_TASKS];
static mut PROGRAM_BREAKS: [u64; scheduler::USER_TASKS] = [USER_BRK_START; scheduler::USER_TASKS];
static mut STDOUT_BUDGETS: [isize; scheduler::USER_TASKS] = [-1; scheduler::USER_TASKS];
static mut STDIN_RAW: [bool; scheduler::USER_TASKS] = [false; scheduler::USER_TASKS];
static mut UNKNOWN_LOGS: u64 = 0;

pub fn reset_process_state(index: usize) {
    unsafe {
        if index >= scheduler::USER_TASKS {
            return;
        }
        MMAP_CURSORS[index] = USER_MMAP_START;
        PROGRAM_BREAKS[index] = USER_BRK_START;
        STDOUT_BUDGETS[index] = -1;
        STDIN_RAW[index] = false;
        for file in &mut (*OPEN_FILES.0.get())[index] {
            *file = OpenFile::empty();
        }
        if TASK_CWD_LENS[index] == 0 {
            TASK_CWDS[index][0] = b'/';
            TASK_CWD_LENS[index] = 1;
        }
    }
}

pub fn set_stdout_budget_for(index: usize, bytes: usize) {
    unsafe {
        if index < scheduler::USER_TASKS {
            STDOUT_BUDGETS[index] = bytes as isize;
        }
    }
}

fn current_task_index() -> usize {
    scheduler::current_user_index()
        .filter(|index| *index < scheduler::USER_TASKS)
        .unwrap_or(0)
}

pub fn handle_syscall(frame: &mut scheduler::TrapFrame) -> bool {
    match frame.rax {
        SYS_READ => {
            if let Some(result) = read(
                frame,
                frame.rdi as i32,
                frame.rsi as *mut u8,
                frame.rdx as usize,
            ) {
                frame.rax = result as u64;
            }
            true
        }
        SYS_WRITE => {
            frame.rax = write(frame.rdi as i32, frame.rsi as *const u8, frame.rdx as usize) as u64;
            true
        }
        SYS_OPEN => {
            frame.rax = open_at(-100, frame.rdi as *const u8, frame.rsi) as u64;
            true
        }
        SYS_CLOSE => {
            frame.rax = close(frame.rdi as i32) as u64;
            true
        }
        SYS_STAT | SYS_LSTAT => {
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
        SYS_POLL => {
            frame.rax = poll(frame.rdi as *mut u8, frame.rsi as usize) as u64;
            true
        }
        SYS_MMAP => {
            frame.rax =
                mmap(frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8 as i64, frame.r9) as u64;
            true
        }
        SYS_MUNMAP => {
            frame.rax = 0;
            true
        }
        SYS_MPROTECT => {
            frame.rax = 0;
            true
        }
        SYS_MADVISE => {
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
        SYS_PWRITE64 => {
            frame.rax = pwrite64(
                frame.rdi as i32,
                frame.rsi as *const u8,
                frame.rdx as usize,
                frame.r10 as usize,
            ) as u64;
            true
        }
        SYS_WRITEV => {
            frame.rax = writev(frame.rdi as i32, frame.rsi as *const u8, frame.rdx as usize) as u64;
            true
        }
        SYS_PIPE | SYS_PIPE2 | SYS_SPLICE => {
            frame.rax = ENOSYS as u64;
            true
        }
        SYS_ACCESS => {
            frame.rax = access(frame.rdi as *const u8) as u64;
            true
        }
        SYS_FACCESSAT => {
            frame.rax = access_at(frame.rdi as i32, frame.rsi as *const u8) as u64;
            true
        }
        SYS_FCNTL => {
            frame.rax = fcntl(frame.rdi as i32, frame.rsi, frame.rdx) as u64;
            true
        }
        SYS_FSYNC | SYS_FDATASYNC => {
            frame.rax = 0;
            true
        }
        SYS_TRUNCATE => {
            frame.rax = truncate_path(frame.rdi as *const u8, frame.rsi as usize) as u64;
            true
        }
        SYS_FTRUNCATE => {
            frame.rax = ftruncate_fd(frame.rdi as i32, frame.rsi as usize) as u64;
            true
        }
        SYS_BRK => {
            frame.rax = brk(frame.rdi) as u64;
            true
        }
        SYS_GETPID => {
            frame.rax = scheduler::current_user_pid().unwrap_or(1);
            true
        }
        SYS_FORK => {
            frame.rax = fork(frame) as u64;
            true
        }
        SYS_EXECVE => {
            frame.rax = execve(frame, frame.rdi as *const u8, frame.rsi as *const u64) as u64;
            true
        }
        SYS_UNAME => {
            frame.rax = uname(frame.rdi as *mut u8) as u64;
            true
        }
        SYS_WAIT4 => {
            match scheduler::wait_for_child(frame, frame.rdi as i32) {
                scheduler::WaitResult::Exited(pid) => {
                    frame.rax = pid;
                }
                scheduler::WaitResult::Blocked(task_switch) => unsafe {
                    crate::arch::load_cr3(task_switch.pml4_phys);
                },
                scheduler::WaitResult::NoChild => {
                    frame.rax = ECHILD as u64;
                }
            }
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
        SYS_GETRLIMIT => {
            frame.rax = getrlimit(frame.rsi as *mut u8) as u64;
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
        SYS_SIGALTSTACK => {
            frame.rax = 0;
            true
        }
        SYS_CHDIR => {
            frame.rax = chdir(frame.rdi as *const u8) as u64;
            true
        }
        SYS_UNLINK => {
            frame.rax = unlink(frame.rdi as *const u8) as u64;
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
            frame.rax = scheduler::current_user_parent_pid().unwrap_or(0);
            true
        }
        SYS_ARCH_PRCTL => {
            frame.rax = arch_prctl(frame.rdi, frame.rsi) as u64;
            true
        }
        SYS_TKILL => {
            frame.rax = 0;
            true
        }
        SYS_GETXATTR | SYS_LGETXATTR | SYS_FGETXATTR => {
            frame.rax = ENODATA as u64;
            true
        }
        SYS_LISTXATTR | SYS_LLISTXATTR | SYS_FLISTXATTR => {
            frame.rax = 0;
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
        SYS_GETDENTS64 => {
            frame.rax =
                getdents64(frame.rdi as i32, frame.rsi as *mut u8, frame.rdx as usize) as u64;
            true
        }
        SYS_EXIT => {
            if let Some(pml4_phys) = scheduler::exit_current_user(frame, frame.rdi as i32) {
                unsafe {
                    crate::arch::load_cr3(pml4_phys);
                }
            }
            true
        }
        SYS_EXIT_GROUP => {
            if let Some(pml4_phys) = scheduler::exit_current_user(frame, frame.rdi as i32) {
                unsafe {
                    crate::arch::load_cr3(pml4_phys);
                }
            }
            true
        }
        SYS_CLOCK_GETTIME => {
            frame.rax = clock_gettime(frame.rsi as *mut u8) as u64;
            true
        }
        SYS_OPENAT => {
            frame.rax = open_at(frame.rdi as i32, frame.rsi as *const u8, frame.rdx) as u64;
            true
        }
        SYS_UNLINKAT => {
            frame.rax = unlink_at(frame.rdi as i32, frame.rsi as *const u8) as u64;
            true
        }
        SYS_NEWFSTATAT => {
            frame.rax = stat_path_at(
                frame.rdi as i32,
                frame.rsi as *const u8,
                frame.rdx as *mut u8,
            ) as u64;
            true
        }
        SYS_PRLIMIT64 => {
            frame.rax = prlimit64(frame.rdx as *const u8, frame.r10 as *mut u8) as u64;
            true
        }
        SYS_GETRANDOM => {
            frame.rax = getrandom(frame.rdi as *mut u8, frame.rsi as usize) as u64;
            true
        }
        SYS_STATX => {
            frame.rax = statx(
                frame.rdi as i32,
                frame.rsi as *const u8,
                frame.r8 as *mut u8,
            ) as u64;
            true
        }
        SYS_RSEQ => {
            frame.rax = ENOSYS as u64;
            true
        }
        _ => {
            log_unknown_syscall(frame.rax);
            frame.rax = ENOSYS as u64;
            true
        }
    }
}

fn open_at(dirfd: i32, path: *const u8, flags: u64) -> i64 {
    let mut raw_path = [0u8; 256];
    let raw_len = read_user_cstr(path, &mut raw_path);
    if raw_len == 0 {
        return ENOENT;
    }

    let mut resolved = [0u8; 256];
    let Some(path_len) = resolve_at_path(dirfd, &raw_path[..raw_len], &mut resolved, false) else {
        if flags & O_CREAT == 0 {
            return ENOENT;
        }
        let Some(path_len) = normalize_at_path(dirfd, &raw_path[..raw_len], &mut resolved, false)
        else {
            return ENOENT;
        };
        return open_ram_file(&resolved[..path_len], flags, true);
    };

    let write_requested = flags & (O_WRONLY | O_RDWR | O_TRUNC) != 0;
    if write_requested {
        return open_ram_file(&resolved[..path_len], flags, true);
    }

    let Some(meta) = nkfs::metadata(&resolved[..path_len]) else {
        return ENOENT;
    };
    let Some(data) = read_visible_file_or_dir(&resolved[..path_len], meta.kind) else {
        return ENOENT;
    };
    if data.len() > FD_BUFFER_CAP {
        return EINVAL;
    }
    unsafe {
        let task_index = current_task_index();
        let Some((fd, file_index)) = alloc_open_file() else {
            return EMFILE;
        };
        core::ptr::copy_nonoverlapping(
            data.as_ptr(),
            core::ptr::addr_of_mut!(FD_BUFFERS)
                .cast::<u8>()
                .add((task_index * MAX_OPEN_FILES + file_index) * FD_BUFFER_CAP),
            data.len(),
        );
        let file = &mut (*OPEN_FILES.0.get())[task_index][file_index];
        file.data = Some(core::slice::from_raw_parts(
            core::ptr::addr_of!(FD_BUFFERS)
                .cast::<u8>()
                .add((task_index * MAX_OPEN_FILES + file_index) * FD_BUFFER_CAP),
            data.len(),
        ));
        file.offset = 0;
        file.is_dir = meta.kind == 2;
        file.writable = false;
        file.ram_index = nkfs::ram_file_index(&resolved[..path_len]).unwrap_or(usize::MAX);
        file.mode = if meta.kind == 2 { 0o040555 } else { 0o100555 };
        file.path[..path_len].copy_from_slice(&resolved[..path_len]);
        file.path_len = path_len;
        fd as i64
    }
}

fn open_ram_file(path: &[u8], flags: u64, create_or_truncate: bool) -> i64 {
    let ram_index = if create_or_truncate {
        nkfs::open_writable_file(path, flags & O_TRUNC != 0)
    } else {
        nkfs::ram_file_index(path)
    };
    let Some(ram_index) = ram_index else {
        return ENOENT;
    };
    let Some(data) = nkfs::ram_file_slice(ram_index) else {
        return ENOENT;
    };
    unsafe {
        let task_index = current_task_index();
        let Some((fd, file_index)) = alloc_open_file() else {
            return EMFILE;
        };
        if data.len() > FD_BUFFER_CAP {
            return EINVAL;
        }
        core::ptr::copy_nonoverlapping(
            data.as_ptr(),
            core::ptr::addr_of_mut!(FD_BUFFERS)
                .cast::<u8>()
                .add((task_index * MAX_OPEN_FILES + file_index) * FD_BUFFER_CAP),
            data.len(),
        );
        let file = &mut (*OPEN_FILES.0.get())[task_index][file_index];
        file.data = Some(core::slice::from_raw_parts(
            core::ptr::addr_of!(FD_BUFFERS)
                .cast::<u8>()
                .add((task_index * MAX_OPEN_FILES + file_index) * FD_BUFFER_CAP),
            data.len(),
        ));
        file.offset = 0;
        file.is_dir = false;
        file.writable = flags & (O_WRONLY | O_RDWR | O_CREAT | O_TRUNC) != 0;
        file.ram_index = ram_index;
        file.mode = 0o100755;
        file.path[..path.len()].copy_from_slice(path);
        file.path_len = path.len();
        fd as i64
    }
}

fn read_visible_file_or_dir(path: &[u8], kind: u16) -> Option<&'static [u8]> {
    if kind == 2 {
        nkfs::read_dir(path)
    } else {
        nkfs::read_file(path)
    }
}

unsafe fn alloc_open_file() -> Option<(i32, usize)> {
    let task_index = current_task_index();
    let files = &mut (*OPEN_FILES.0.get())[task_index];
    for index in 0..MAX_OPEN_FILES {
        if files[index].data.is_none() {
            return Some((FIRST_USER_FD + index as i32, index));
        }
    }
    None
}

unsafe fn open_file(fd: i32) -> Option<&'static mut OpenFile> {
    if fd < FIRST_USER_FD {
        return None;
    }
    let index = (fd - FIRST_USER_FD) as usize;
    if index >= MAX_OPEN_FILES {
        return None;
    }
    let task_index = current_task_index();
    let file = &mut (*OPEN_FILES.0.get())[task_index][index];
    if file.data.is_none() {
        return None;
    }
    Some(file)
}

fn fork(frame: &scheduler::TrapFrame) -> i64 {
    let Some(parent) = scheduler::current_user_index() else {
        return EINVAL;
    };
    let Some(child) = scheduler::allocate_child_slot() else {
        return EAGAIN;
    };
    if !memory::copy_user_space(parent, child) {
        return EINVAL;
    }
    let Some(child_pml4) = userland::task_pml4(child) else {
        return EINVAL;
    };
    let Some(pid) = scheduler::fork_current_user_to(child, child_pml4, frame) else {
        return EAGAIN;
    };
    copy_process_state(parent, child);
    pid as i64
}

fn execve(frame: &mut scheduler::TrapFrame, path: *const u8, argv: *const u64) -> i64 {
    let Some(index) = scheduler::current_user_index() else {
        return EINVAL;
    };
    let mut raw_path = [0u8; 256];
    let raw_len = read_user_cstr(path, &mut raw_path);
    if raw_len == 0 {
        return ENOENT;
    }
    let mut exec_path = [0u8; 256];
    let Some(exec_len) = resolve_exec_path(&raw_path[..raw_len], &mut exec_path) else {
        return ENOENT;
    };

    let mut arg_storage = [[0u8; 64]; 4];
    let mut arg_lens = [0usize; 4];
    let mut arg_count = read_argv(argv, &mut arg_storage, &mut arg_lens);
    if arg_count == 0 {
        let fallback = path_basename(path, &mut arg_storage[0]);
        arg_lens[0] = fallback;
        arg_count = 1;
    }
    let mut args: [&[u8]; 4] = [b"", b"", b"", b""];
    for arg_index in 0..arg_count {
        args[arg_index] = &arg_storage[arg_index][..arg_lens[arg_index]];
    }

    let native_name = native_exec_name(&exec_path[..exec_len]);
    let exec_ok = if let Some(name) = native_name {
        userland::exec_native_elf(index, name, &exec_path[..exec_len], frame)
    } else {
        userland::exec_linux_elf(
            index,
            "exec",
            &exec_path[..exec_len],
            &args[..arg_count],
            frame,
        )
    };

    if exec_ok {
        0
    } else {
        ENOENT
    }
}

fn native_exec_name(path: &[u8]) -> Option<&'static str> {
    let name = basename_bytes(path);
    if name == b"gui" || name == b"GUI.elf" {
        Some("gui")
    } else if name == b"taskview" || name == b"taskviewer" {
        Some("taskviewer")
    } else {
        None
    }
}

fn read(frame: &mut scheduler::TrapFrame, fd: i32, buffer: *mut u8, len: usize) -> Option<i64> {
    if buffer.is_null() || len == 0 {
        return Some(0);
    }
    if fd == 0 {
        return read_stdin(frame, buffer, len);
    }
    if fd < FIRST_USER_FD {
        return Some(EBADF);
    }

    unsafe {
        let Some(file) = open_file(fd) else {
            return Some(EBADF);
        };
        let Some(data) = file.data else {
            return Some(EBADF);
        };
        if file.is_dir {
            return Some(EINVAL);
        }
        if file.offset >= data.len() {
            return Some(0);
        }

        let count = len.min(data.len() - file.offset);
        core::ptr::copy_nonoverlapping(data.as_ptr().add(file.offset), buffer, count);
        file.offset += count;
        Some(count as i64)
    }
}

fn read_stdin(frame: &mut scheduler::TrapFrame, buffer: *mut u8, len: usize) -> Option<i64> {
    if len == 0 {
        return Some(0);
    }

    if unsafe { STDIN_RAW[current_task_index()] } {
        if let Some(count) = pop_ready_input(buffer, len) {
            return Some(count as i64);
        }
        if let Some(byte) = keyboard::pop_key() {
            unsafe {
                *buffer = byte;
            }
            return Some(1);
        }
        return Some(EAGAIN);
    }

    if let Some(count) = pop_ready_input(buffer, len) {
        return Some(count as i64);
    }

    if let Some(task_switch) = scheduler::block_current_for_stdin(frame, buffer as u64) {
        unsafe {
            crate::arch::load_cr3(task_switch.pml4_phys);
        }
        None
    } else {
        Some(EAGAIN)
    }
}

pub fn handle_stdin_key(byte: u8) {
    unsafe {
        let task = scheduler::stdin_waiter_index().unwrap_or_else(current_task_index);
        if STDIN_RAW[task] {
            return;
        }
        match byte {
            8 | 127 => {
                if INPUT_LINE_LEN > 0 {
                    INPUT_LINE_LEN -= 1;
                    echo_stdin_key(8);
                }
            }
            b'\n' | b'\r' => {
                echo_stdin_key(b'\n');
                push_ready_input(b'\n');
                INPUT_LINE_LEN = 0;
                wake_stdin_reader();
            }
            byte if byte >= 0x20 && INPUT_LINE_LEN < INPUT_LINE_CAP - 1 => {
                INPUT_LINE[INPUT_LINE_LEN] = byte;
                INPUT_LINE_LEN += 1;
                echo_stdin_key(byte);
            }
            _ => {}
        }
    }
}

fn echo_stdin_key(byte: u8) {
    if byte == 8 || byte == 127 {
        services::gui::console_write(&[8]);
    } else {
        services::gui::console_write(&[byte]);
    }
}

unsafe fn push_ready_input(byte: u8) {
    for index in 0..INPUT_LINE_LEN {
        let next = (READY_WRITE + 1) % READY_INPUT_CAP;
        if next == READY_READ {
            break;
        }
        READY_INPUT[READY_WRITE] = INPUT_LINE[index];
        READY_WRITE = next;
    }
    let next = (READY_WRITE + 1) % READY_INPUT_CAP;
    if next != READY_READ {
        READY_INPUT[READY_WRITE] = byte;
        READY_WRITE = next;
    }
}

fn pop_ready_input(buffer: *mut u8, len: usize) -> Option<usize> {
    unsafe {
        if READY_READ == READY_WRITE {
            return None;
        }
        let mut count = 0usize;
        while count < len && READY_READ != READY_WRITE {
            *buffer.add(count) = READY_INPUT[READY_READ];
            READY_READ = (READY_READ + 1) % READY_INPUT_CAP;
            count += 1;
        }
        Some(count)
    }
}

unsafe fn wake_stdin_reader() {
    if let Some(wake) = scheduler::wake_stdin_waiter() {
        if READY_READ == READY_WRITE {
            return;
        }
        let byte = READY_INPUT[READY_READ];
        READY_READ = (READY_READ + 1) % READY_INPUT_CAP;
        let current_pml4 = arch::read_cr3();
        arch::load_cr3(wake.pml4_phys);
        *(wake.buffer as *mut u8) = byte;
        arch::load_cr3(current_pml4);
    }
}

fn write(fd: i32, buffer: *const u8, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    if buffer.is_null() {
        return EFAULT;
    }
    if fd != 1 && fd != 2 {
        return write_regular_file(fd, buffer, len);
    }

    let len = unsafe {
        let task_index = current_task_index();
        if fd == 1 && STDOUT_BUDGETS[task_index] >= 0 {
            if STDOUT_BUDGETS[task_index] == 0 {
                return EPIPE;
            }
            let allowed = (STDOUT_BUDGETS[task_index] as usize).min(len);
            STDOUT_BUDGETS[task_index] -= allowed as isize;
            allowed
        } else {
            len
        }
    };

    let mut written = 0usize;
    while written < len {
        let count = (len - written).min(4096);
        let chunk = unsafe { buffer.add(written) };
        if !user_buffer_readable(chunk as u64, count) {
            return if written > 0 { written as i64 } else { EFAULT };
        }
        let bytes = unsafe { core::slice::from_raw_parts(chunk, count) };
        for byte in bytes {
            serial::write_str_byte(*byte);
        }
        services::gui::console_write(bytes);
        written += count;
    }
    len as i64
}

fn write_regular_file(fd: i32, buffer: *const u8, len: usize) -> i64 {
    if fd < FIRST_USER_FD {
        return EBADF;
    }
    unsafe {
        let Some(file) = open_file(fd) else {
            return EBADF;
        };
        if !file.writable || file.is_dir || file.ram_index == usize::MAX {
            return EBADF;
        }
        let mut written = 0usize;
        while written < len {
            let count = (len - written).min(4096);
            let chunk = buffer.add(written);
            if !user_buffer_readable(chunk as u64, count) {
                return if written > 0 { written as i64 } else { EFAULT };
            }
            let bytes = core::slice::from_raw_parts(chunk, count);
            let Some(done) = nkfs::write_ram_file(file.ram_index, file.offset, bytes) else {
                return if written > 0 { written as i64 } else { EINVAL };
            };
            file.offset += done;
            written += done;
            if done < count {
                break;
            }
        }
        if let Some(data) = nkfs::ram_file_slice(file.ram_index) {
            file.data = Some(data);
        }
        written as i64
    }
}

fn pwrite64(fd: i32, buffer: *const u8, len: usize, offset: usize) -> i64 {
    if fd < FIRST_USER_FD {
        return EBADF;
    }
    unsafe {
        let Some(file) = open_file(fd) else {
            return EBADF;
        };
        if !file.writable || file.is_dir || file.ram_index == usize::MAX {
            return EBADF;
        }
        let old_offset = file.offset;
        file.offset = offset;
        let written = write_regular_file(fd, buffer, len);
        if let Some(file) = open_file(fd) {
            file.offset = old_offset;
        }
        written
    }
}

fn writev(fd: i32, iov: *const u8, count: usize) -> i64 {
    if iov.is_null() {
        return EFAULT;
    }
    if count > 16 {
        return EINVAL;
    }
    let Some(iov_len) = count.checked_mul(16) else {
        return EINVAL;
    };
    if !user_buffer_readable(iov as u64, iov_len) {
        return EFAULT;
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
    if fd < FIRST_USER_FD {
        return EBADF;
    }
    unsafe {
        let Some(file) = open_file(fd) else {
            return EBADF;
        };
        *file = OpenFile::empty();
    }
    0
}

fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    if fd < FIRST_USER_FD {
        return EBADF;
    }

    unsafe {
        let Some(file) = open_file(fd) else {
            return EBADF;
        };
        let len = if file.ram_index != usize::MAX {
            nkfs::ram_file_slice(file.ram_index).map_or(0, |data| data.len())
        } else {
            file.data.map_or(0, |data| data.len())
        };
        let base = match whence {
            0 => 0i64,
            1 => file.offset as i64,
            2 => len as i64,
            _ => return EINVAL,
        };
        let next = base.saturating_add(offset);
        if next < 0 {
            return EINVAL;
        }
        file.offset = next as usize;
        file.offset as i64
    }
}

fn truncate_path(path: *const u8, len: usize) -> i64 {
    let mut raw_path = [0u8; 256];
    let raw_len = read_user_cstr(path, &mut raw_path);
    if raw_len == 0 {
        return ENOENT;
    }
    let mut resolved = [0u8; 256];
    let Some(path_len) = resolve_at_path(-100, &raw_path[..raw_len], &mut resolved, false) else {
        return ENOENT;
    };
    let Some(index) = nkfs::open_writable_file(&resolved[..path_len], false) else {
        return EINVAL;
    };
    if nkfs::truncate_ram_file(index, len) {
        0
    } else {
        EINVAL
    }
}

fn ftruncate_fd(fd: i32, len: usize) -> i64 {
    if fd < FIRST_USER_FD {
        return EBADF;
    }
    unsafe {
        let Some(file) = open_file(fd) else {
            return EBADF;
        };
        if !file.writable || file.is_dir || file.ram_index == usize::MAX {
            return EBADF;
        }
        if !nkfs::truncate_ram_file(file.ram_index, len) {
            return EINVAL;
        }
        if let Some(data) = nkfs::ram_file_slice(file.ram_index) {
            file.data = Some(data);
        }
    }
    0
}

fn fcntl(fd: i32, command: u64, _arg: u64) -> i64 {
    if fd != 0 && fd != 1 && fd != 2 {
        unsafe {
            if open_file(fd).is_none() {
                return EBADF;
            }
        }
    }
    if fd < 0 {
        return EBADF;
    }
    match command {
        1 | 2 | 3 => 0,
        _ => 0,
    }
}

fn brk(request: u64) -> i64 {
    unsafe {
        let task_index = current_task_index();
        if request == 0 {
            return PROGRAM_BREAKS[task_index] as i64;
        }
        if (USER_BRK_START..=USER_BRK_END).contains(&request) {
            let current = PROGRAM_BREAKS[task_index];
            if request > current {
                let len = (request - current) as usize;
                if !memory::allocate_user_range(task_index, current, len, true) {
                    return current as i64;
                }
            }
            PROGRAM_BREAKS[task_index] = request;
        }
        PROGRAM_BREAKS[task_index] as i64
    }
}

fn mmap(address: u64, len: u64, _prot: u64, flags: u64, fd: i64, offset: u64) -> i64 {
    const MAP_FIXED: u64 = 0x10;
    const MAP_ANONYMOUS: u64 = 0x20;

    if len == 0 {
        return EINVAL;
    }
    let aligned_len = (len + 4095) & !4095;
    unsafe {
        let task_index = current_task_index();
        let base = if flags & MAP_FIXED != 0 && address != 0 {
            address
        } else {
            let next = (MMAP_CURSORS[task_index] + 4095) & !4095;
            MMAP_CURSORS[task_index] = next.saturating_add(aligned_len);
            next
        };
        if base < USER_MMAP_START || base.saturating_add(aligned_len) > USER_MMAP_END {
            return -12;
        }
        if !memory::allocate_user_range(task_index, base, aligned_len as usize, true) {
            return -12;
        }
        if fd != -1 && flags & MAP_ANONYMOUS == 0 {
            let Some(file) = open_file(fd as i32) else {
                return EBADF;
            };
            let Some(data) = file.data else {
                return EBADF;
            };
            let start = offset as usize;
            if start > data.len() {
                return EINVAL;
            }
            let count = (len as usize).min(data.len() - start);
            if count > 0
                && !memory::copy_user_segment(task_index, base, &data[start..start + count], count)
            {
                return -12;
            }
        }
        base as i64
    }
}

fn stat_fd(fd: i32, stat_buf: *mut u8) -> i64 {
    if fd == 0 || fd == 1 || fd == 2 {
        return write_fake_stat(stat_buf);
    }
    if fd >= FIRST_USER_FD {
        unsafe {
            let Some(file) = open_file(fd) else {
                return EBADF;
            };
            let Some(data) = file.data else {
                return EBADF;
            };
            return write_stat(stat_buf, file.mode, data.len() as u64);
        }
    }
    EBADF
}

fn stat_path(path: *const u8, stat_buf: *mut u8) -> i64 {
    stat_path_at(-100, path, stat_buf)
}

fn stat_path_at(dirfd: i32, path: *const u8, stat_buf: *mut u8) -> i64 {
    let mut raw_path = [0u8; 256];
    let raw_len = read_user_cstr(path, &mut raw_path);
    if raw_len == 0 {
        if dirfd >= FIRST_USER_FD {
            unsafe {
                let Some(file) = open_file(dirfd) else {
                    return EBADF;
                };
                let Some(data) = file.data else {
                    return EBADF;
                };
                return write_stat(stat_buf, file.mode, data.len() as u64);
            }
        }
        return ENOENT;
    }
    let mut resolved = [0u8; 256];
    let Some(path_len) = resolve_at_path(dirfd, &raw_path[..raw_len], &mut resolved, false) else {
        return ENOENT;
    };
    let Some(meta) = nkfs::metadata(&resolved[..path_len]) else {
        return ENOENT;
    };
    let mode = if meta.kind == 2 { 0o040555 } else { 0o100555 };
    write_stat(stat_buf, mode, meta.size as u64)
}

fn statx(dirfd: i32, path: *const u8, statx_buf: *mut u8) -> i64 {
    if path.is_null() || statx_buf.is_null() {
        return EFAULT;
    }

    let mut raw_path = [0u8; 256];
    let raw_len = read_user_cstr(path, &mut raw_path);
    if raw_len == 0 {
        if dirfd >= FIRST_USER_FD {
            unsafe {
                let Some(file) = open_file(dirfd) else {
                    return EBADF;
                };
                let Some(data) = file.data else {
                    return EBADF;
                };
                return write_statx(statx_buf, file.mode, data.len() as u64);
            }
        }
        return ENOENT;
    }

    let mut resolved = [0u8; 256];
    let Some(path_len) = resolve_at_path(dirfd, &raw_path[..raw_len], &mut resolved, false) else {
        return ENOENT;
    };
    let Some(meta) = nkfs::metadata(&resolved[..path_len]) else {
        return ENOENT;
    };
    let mode = if meta.kind == 2 { 0o040555 } else { 0o100555 };
    write_statx(statx_buf, mode, meta.size as u64)
}

fn access(path: *const u8) -> i64 {
    access_at(-100, path)
}

fn access_at(dirfd: i32, path: *const u8) -> i64 {
    let mut raw_path = [0u8; 256];
    let raw_len = read_user_cstr(path, &mut raw_path);
    if raw_len == 0 {
        return ENOENT;
    }
    let mut resolved = [0u8; 256];
    let path_len = resolve_at_path(dirfd, &raw_path[..raw_len], &mut resolved, false)
        .or_else(|| resolve_at_path(dirfd, &raw_path[..raw_len], &mut resolved, true));
    if let Some(len) = path_len {
        if nkfs::exists(&resolved[..len]) {
            return 0;
        }
    }
    ENOENT
}

fn resolve_exec_path(input: &[u8], output: &mut [u8; 256]) -> Option<usize> {
    resolve_at_path(-100, input, output, true)
}

fn resolve_at_path(
    dirfd: i32,
    input: &[u8],
    output: &mut [u8; 256],
    executable: bool,
) -> Option<usize> {
    let len = normalize_at_path(dirfd, input, output, executable)?;
    if nkfs::exists(&output[..len]) {
        Some(len)
    } else {
        None
    }
}

fn normalize_at_path(
    dirfd: i32,
    input: &[u8],
    output: &mut [u8; 256],
    executable: bool,
) -> Option<usize> {
    if input.is_empty() {
        return None;
    }

    let mut scratch = [0u8; 256];
    let scratch_len = if input[0] == b'/' {
        copy_path(input, &mut scratch)?
    } else if executable && !path_contains_slash(input) {
        copy_with_prefix(b"/bin/", input, &mut scratch)?
    } else if dirfd >= FIRST_USER_FD {
        copy_relative_to_fd(dirfd, input, &mut scratch)?
    } else {
        copy_relative_to_cwd(input, &mut scratch)?
    };
    canonicalize_path(&scratch[..scratch_len], output)
}

fn copy_path(input: &[u8], output: &mut [u8; 256]) -> Option<usize> {
    if input.len() > output.len() {
        return None;
    }
    output[..input.len()].copy_from_slice(input);
    Some(input.len())
}

fn copy_with_prefix(prefix: &[u8], input: &[u8], output: &mut [u8; 256]) -> Option<usize> {
    let len = prefix.len().checked_add(input.len())?;
    if len > output.len() {
        return None;
    }
    output[..prefix.len()].copy_from_slice(prefix);
    output[prefix.len()..len].copy_from_slice(input);
    Some(len)
}

fn copy_relative_to_fd(fd: i32, input: &[u8], output: &mut [u8; 256]) -> Option<usize> {
    unsafe {
        let file = open_file(fd)?;
        if !file.is_dir || file.path_len == 0 {
            return None;
        }
        let mut len = file.path_len;
        if len > output.len() {
            return None;
        }
        output[..len].copy_from_slice(&file.path[..len]);
        if len > 1 && output[len - 1] != b'/' {
            if len >= output.len() {
                return None;
            }
            output[len] = b'/';
            len += 1;
        }
        let end = len.checked_add(input.len())?;
        if end > output.len() {
            return None;
        }
        output[len..end].copy_from_slice(input);
        Some(end)
    }
}

fn copy_relative_to_cwd(input: &[u8], output: &mut [u8; 256]) -> Option<usize> {
    let mut cwd = [0u8; 256];
    let mut len = current_cwd(&mut cwd);
    if len > output.len() {
        return None;
    }
    output[..len].copy_from_slice(&cwd[..len]);
    if len > 1 && output[len - 1] != b'/' {
        if len >= output.len() {
            return None;
        }
        output[len] = b'/';
        len += 1;
    }
    let end = len.checked_add(input.len())?;
    if end > output.len() {
        return None;
    }
    output[len..end].copy_from_slice(input);
    Some(end)
}

fn canonicalize_path(input: &[u8], output: &mut [u8; 256]) -> Option<usize> {
    if input.is_empty() || input[0] != b'/' {
        return None;
    }

    output[0] = b'/';
    let mut out_len = 1usize;
    let mut cursor = 1usize;
    while cursor <= input.len() {
        while cursor < input.len() && input[cursor] == b'/' {
            cursor += 1;
        }
        let start = cursor;
        while cursor < input.len() && input[cursor] != b'/' {
            cursor += 1;
        }
        let component = &input[start..cursor];
        if component.is_empty() || component == b"." {
            cursor += 1;
            continue;
        }
        if component == b".." {
            if out_len > 1 {
                out_len -= 1;
                while out_len > 1 && output[out_len - 1] != b'/' {
                    out_len -= 1;
                }
                if out_len > 1 && output[out_len - 1] == b'/' {
                    out_len -= 1;
                }
                if out_len == 0 {
                    out_len = 1;
                }
            }
            cursor += 1;
            continue;
        }

        if out_len > 1 {
            if out_len >= output.len() {
                return None;
            }
            output[out_len] = b'/';
            out_len += 1;
        }
        let end = out_len.checked_add(component.len())?;
        if end > output.len() {
            return None;
        }
        output[out_len..end].copy_from_slice(component);
        out_len = end;
        cursor += 1;
    }

    Some(out_len)
}

fn path_contains_slash(path: &[u8]) -> bool {
    path.iter().any(|byte| *byte == b'/')
}

fn basename_bytes(path: &[u8]) -> &[u8] {
    let mut start = 0usize;
    for index in 0..path.len() {
        if path[index] == b'/' {
            start = index + 1;
        }
    }
    &path[start..]
}

fn ioctl(fd: i32, request: u64, argp: *mut u8) -> i64 {
    if fd != 0 && fd != 1 && fd != 2 {
        return EBADF;
    }

    match request {
        0x5401 => write_termios(argp),
        0x5402 | 0x5403 | 0x5404 => set_termios(argp),
        0x5405 | 0x5406 | 0x5413 => write_winsize(argp),
        _ => 0,
    }
}

fn poll(fds: *mut u8, count: usize) -> i64 {
    if fds.is_null() {
        return EFAULT;
    }
    if count == 0 {
        return 0;
    }
    let mut ready = 0i64;
    unsafe {
        for index in 0..count.min(16) {
            let base = fds.add(index * 8);
            let fd = read_user_i32(base);
            let events = read_user_i16(base.add(4));
            let mut revents = 0i16;
            if fd == 0 && events & 1 != 0 {
                if READY_READ != READY_WRITE || keyboard::has_key() {
                    revents |= 1;
                }
            } else if (fd == 1 || fd == 2) && events & 4 != 0 {
                revents |= 4;
            } else if fd >= FIRST_USER_FD && events != 0 && open_file(fd).is_some() {
                revents |= events & 5;
            }
            if revents != 0 {
                ready += 1;
            }
            write_user_u16(base.add(6), revents as u16);
        }
    }
    ready
}

fn getdents64(fd: i32, dirp: *mut u8, count: usize) -> i64 {
    if fd < FIRST_USER_FD {
        return EBADF;
    }
    if dirp.is_null() || count == 0 {
        return EINVAL;
    }

    unsafe {
        let Some(file) = open_file(fd) else {
            return EBADF;
        };
        let Some(data) = file.data else {
            return EBADF;
        };
        if !file.is_dir {
            return EINVAL;
        }

        let mut written = 0usize;
        while file.offset + 8 <= data.len() {
            let raw_offset = file.offset;
            let inode = read_slice_u32(data, raw_offset).unwrap_or(0) as u64;
            let name_len = read_slice_u16(data, raw_offset + 4).unwrap_or(0) as usize;
            let kind = read_slice_u16(data, raw_offset + 6).unwrap_or(0);
            let next = align_up(raw_offset + 8 + name_len, 4);
            if next > data.len() {
                break;
            }
            let record_len = align_up(19 + name_len + 1, 8);
            if written + record_len > count {
                break;
            }
            let out = dirp.add(written);
            write_user_u64(out, inode);
            write_user_i64(out.add(8), next as i64);
            write_user_u16(out.add(16), record_len as u16);
            *out.add(18) = if kind == 2 { 4 } else { 8 };
            core::ptr::copy_nonoverlapping(
                data.as_ptr().add(raw_offset + 8),
                out.add(19),
                name_len,
            );
            *out.add(19 + name_len) = 0;
            for index in 20 + name_len..record_len {
                *out.add(index) = 0;
            }
            file.offset = next;
            written += record_len;
        }
        written as i64
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
        let raw = STDIN_RAW[current_task_index()];
        let lflag_value = if raw { 0u32 } else { 0x8a3b_u32 };
        let lflag = lflag_value.to_le_bytes();
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
        *argp.add(19) = 8;
    }
    0
}

fn set_termios(argp: *mut u8) -> i64 {
    if argp.is_null() {
        return EFAULT;
    }
    unsafe {
        let lflag = read_user_u32(argp.add(12));
        STDIN_RAW[current_task_index()] = lflag & 0x0a == 0;
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
    let mut cwd = [0u8; 256];
    let cwd_len = current_cwd(&mut cwd);
    if len <= cwd_len {
        return EINVAL;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(cwd.as_ptr(), buffer, cwd_len);
        *buffer.add(cwd_len) = 0;
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

fn getrlimit(buffer: *mut u8) -> i64 {
    write_rlimit(buffer)
}

fn prlimit64(new_limit: *const u8, old_limit: *mut u8) -> i64 {
    if !new_limit.is_null() {
        return EINVAL;
    }
    if old_limit.is_null() {
        return 0;
    }
    write_rlimit(old_limit)
}

fn write_rlimit(buffer: *mut u8) -> i64 {
    if buffer.is_null() {
        return EFAULT;
    }
    unsafe {
        write_user_u64(buffer, u64::MAX);
        write_user_u64(buffer.add(8), u64::MAX);
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

    let value = b"/bin/bash";
    let count = len.min(value.len());
    unsafe {
        core::ptr::copy_nonoverlapping(value.as_ptr(), buffer, count);
    }
    count as i64
}

fn chdir(path: *const u8) -> i64 {
    let mut raw_path = [0u8; 256];
    let raw_len = read_user_cstr(path, &mut raw_path);
    if raw_len == 0 {
        return ENOENT;
    }

    let mut resolved = [0u8; 256];
    let Some(path_len) = normalize_at_path(-100, &raw_path[..raw_len], &mut resolved, false) else {
        return ENOENT;
    };
    let Some(meta) = nkfs::metadata(&resolved[..path_len]) else {
        return ENOENT;
    };
    if meta.kind != 2 {
        return ENOENT;
    }

    set_current_cwd(&resolved[..path_len])
}

fn unlink(path: *const u8) -> i64 {
    unlink_at(-100, path)
}

fn unlink_at(dirfd: i32, path: *const u8) -> i64 {
    let mut raw_path = [0u8; 256];
    let raw_len = read_user_cstr(path, &mut raw_path);
    if raw_len == 0 {
        return ENOENT;
    }
    let mut resolved = [0u8; 256];
    let Some(path_len) = normalize_at_path(dirfd, &raw_path[..raw_len], &mut resolved, false) else {
        return ENOENT;
    };
    if nkfs::remove_ram_file(&resolved[..path_len]) {
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
    write_stat(stat_buf, 0o100444, 0)
}

fn write_stat(stat_buf: *mut u8, mode_value: u32, size_value: u64) -> i64 {
    if stat_buf.is_null() {
        return EFAULT;
    }
    unsafe {
        for index in 0..144 {
            *stat_buf.add(index) = 0;
        }
        let mode = mode_value.to_le_bytes();
        for (index, byte) in mode.iter().enumerate() {
            *stat_buf.add(24 + index) = *byte;
        }
        let size = size_value.to_le_bytes();
        for (index, byte) in size.iter().enumerate() {
            *stat_buf.add(48 + index) = *byte;
        }
    }
    0
}

fn write_statx(statx_buf: *mut u8, mode_value: u32, size_value: u64) -> i64 {
    unsafe {
        for index in 0..256 {
            *statx_buf.add(index) = 0;
        }
        write_user_u32(statx_buf, 0x17ff);
        write_user_u32(statx_buf.add(4), 4096);
        write_user_u32(statx_buf.add(16), 1);
        write_user_u16(statx_buf.add(28), mode_value as u16);
        write_user_u64(statx_buf.add(32), 1);
        write_user_u64(statx_buf.add(40), size_value);
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

fn read_argv(argv: *const u64, storage: &mut [[u8; 64]; 4], lens: &mut [usize; 4]) -> usize {
    if argv.is_null() {
        return 0;
    }

    let mut count = 0usize;
    unsafe {
        while count < storage.len() {
            let ptr = *(argv.add(count)) as *const u8;
            if ptr.is_null() {
                break;
            }
            lens[count] = read_user_cstr(ptr, &mut storage[count]);
            if lens[count] == 0 {
                break;
            }
            count += 1;
        }
    }
    count
}

fn path_basename(path: *const u8, output: &mut [u8; 64]) -> usize {
    if path.is_null() {
        return 0;
    }

    let mut raw = [0u8; 64];
    let len = read_user_cstr(path, &mut raw);
    let mut start = 0usize;
    for index in 0..len {
        if raw[index] == b'/' {
            start = index + 1;
        }
    }

    let count = (len - start).min(output.len());
    output[..count].copy_from_slice(&raw[start..start + count]);
    count
}

fn read_user_cstr(path: *const u8, output: &mut [u8]) -> usize {
    if path.is_null() {
        return 0;
    }

    let mut len = 0usize;
    unsafe {
        while len < output.len() {
            let byte = *path.add(len);
            if byte == 0 {
                break;
            }
            output[len] = byte;
            len += 1;
        }
    }
    len
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

fn current_cwd(output: &mut [u8; 256]) -> usize {
    unsafe {
        let index = scheduler::current_user_index().unwrap_or(0);
        let len = TASK_CWD_LENS[index];
        if len == 0 {
            output[0] = b'/';
            1
        } else {
            output[..len].copy_from_slice(&TASK_CWDS[index][..len]);
            len
        }
    }
}

fn set_current_cwd(path: &[u8]) -> i64 {
    let Some(index) = scheduler::current_user_index() else {
        return EINVAL;
    };
    if path.is_empty() || path.len() > 256 || path[0] != b'/' {
        return EINVAL;
    }
    unsafe {
        TASK_CWDS[index][..path.len()].copy_from_slice(path);
        TASK_CWD_LENS[index] = path.len();
    }
    0
}

fn copy_cwd(parent: usize, child: usize) {
    if parent >= scheduler::USER_TASKS || child >= scheduler::USER_TASKS {
        return;
    }
    unsafe {
        let len = TASK_CWD_LENS[parent];
        if len == 0 {
            TASK_CWDS[child][0] = b'/';
            TASK_CWD_LENS[child] = 1;
        } else {
            TASK_CWDS[child][..len].copy_from_slice(&TASK_CWDS[parent][..len]);
            TASK_CWD_LENS[child] = len;
        }
    }
}

fn copy_process_state(parent: usize, child: usize) {
    copy_cwd(parent, child);
    if parent >= scheduler::USER_TASKS || child >= scheduler::USER_TASKS {
        return;
    }
    unsafe {
        MMAP_CURSORS[child] = MMAP_CURSORS[parent];
        PROGRAM_BREAKS[child] = PROGRAM_BREAKS[parent];
        STDOUT_BUDGETS[child] = STDOUT_BUDGETS[parent];

        let open_files = &mut *OPEN_FILES.0.get();
        open_files[child] = open_files[parent];

        let buffers = &mut *core::ptr::addr_of_mut!(FD_BUFFERS);
        buffers[child] = buffers[parent];

        for fd_index in 0..MAX_OPEN_FILES {
            if let Some(data) = open_files[child][fd_index].data {
                open_files[child][fd_index].data = Some(core::slice::from_raw_parts(
                    buffers[child][fd_index].as_ptr(),
                    data.len(),
                ));
            }
        }
    }
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn read_slice_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let data = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([data[0], data[1]]))
}

fn read_slice_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let data = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

fn user_buffer_readable(address: u64, len: usize) -> bool {
    memory::user_range_mapped(current_task_index(), address, len)
}

unsafe fn read_user_u64(ptr: *const u8) -> u64 {
    let mut bytes = [0u8; 8];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = *ptr.add(index);
    }
    u64::from_le_bytes(bytes)
}

unsafe fn read_user_u32(ptr: *const u8) -> u32 {
    let mut bytes = [0u8; 4];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = *ptr.add(index);
    }
    u32::from_le_bytes(bytes)
}

unsafe fn read_user_i32(ptr: *const u8) -> i32 {
    read_user_u32(ptr) as i32
}

unsafe fn read_user_i16(ptr: *const u8) -> i16 {
    let mut bytes = [0u8; 2];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = *ptr.add(index);
    }
    i16::from_le_bytes(bytes)
}

unsafe fn write_user_u16(ptr: *mut u8, value: u16) {
    for (index, byte) in value.to_le_bytes().iter().enumerate() {
        *ptr.add(index) = *byte;
    }
}

unsafe fn write_user_u32(ptr: *mut u8, value: u32) {
    for (index, byte) in value.to_le_bytes().iter().enumerate() {
        *ptr.add(index) = *byte;
    }
}

unsafe fn write_user_u64(ptr: *mut u8, value: u64) {
    for (index, byte) in value.to_le_bytes().iter().enumerate() {
        *ptr.add(index) = *byte;
    }
}

unsafe fn write_user_i64(ptr: *mut u8, value: i64) {
    for (index, byte) in value.to_le_bytes().iter().enumerate() {
        *ptr.add(index) = *byte;
    }
}
