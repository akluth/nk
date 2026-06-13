use core::cell::UnsafeCell;

use crate::{arch, scheduler, services};

const LINE_CAP: usize = 1024;
const READY_CAP: usize = 4096;

struct TtyState {
    line: [u8; LINE_CAP],
    line_len: usize,
    ready: [u8; READY_CAP],
    read: usize,
    write: usize,
    raw: [bool; scheduler::USER_TASKS],
    foreground_pgid: u64,
}

impl TtyState {
    const fn new() -> Self {
        Self {
            line: [0; LINE_CAP],
            line_len: 0,
            ready: [0; READY_CAP],
            read: 0,
            write: 0,
            raw: [false; scheduler::USER_TASKS],
            foreground_pgid: 1,
        }
    }

    fn has_ready(&self) -> bool {
        self.read != self.write
    }

    fn push_ready(&mut self, byte: u8) {
        let next = (self.write + 1) % READY_CAP;
        if next == self.read {
            return;
        }
        self.ready[self.write] = byte;
        self.write = next;
    }

    fn pop_ready(&mut self) -> Option<u8> {
        if self.read == self.write {
            return None;
        }
        let byte = self.ready[self.read];
        self.read = (self.read + 1) % READY_CAP;
        Some(byte)
    }
}

struct GlobalTty(UnsafeCell<TtyState>);

unsafe impl Sync for GlobalTty {}

static TTY: GlobalTty = GlobalTty(UnsafeCell::new(TtyState::new()));

pub fn reset_task(index: usize) {
    unsafe {
        if index < scheduler::USER_TASKS {
            (*TTY.0.get()).raw[index] = false;
        }
    }
}

pub fn set_raw(index: usize, raw: bool) {
    unsafe {
        if index < scheduler::USER_TASKS {
            (*TTY.0.get()).raw[index] = raw;
        }
    }
}

pub fn is_raw(index: usize) -> bool {
    unsafe { index < scheduler::USER_TASKS && (*TTY.0.get()).raw[index] }
}

pub fn has_input() -> bool {
    unsafe { (*TTY.0.get()).has_ready() }
}

pub fn foreground_pgid() -> u64 {
    unsafe { (*TTY.0.get()).foreground_pgid }
}

pub fn set_foreground_pgid(pgid: u64) -> bool {
    if pgid == 0 {
        return false;
    }
    unsafe {
        (*TTY.0.get()).foreground_pgid = pgid;
    }
    true
}

pub fn read(
    frame: &mut scheduler::TrapFrame,
    task_index: usize,
    buffer: *mut u8,
    len: usize,
) -> Option<i64> {
    if len == 0 {
        return Some(0);
    }

    unsafe {
        let tty = &mut *TTY.0.get();
        if task_index < scheduler::USER_TASKS && tty.raw[task_index] {
            if let Some(count) = pop_ready_bytes(tty, buffer, len) {
                return Some(count as i64);
            }
            return Some(-11);
        }

        if let Some(count) = pop_ready_bytes(tty, buffer, len) {
            return Some(count as i64);
        }
    }

    if let Some(task_switch) = scheduler::block_current_for_stdin(frame, buffer as u64) {
        unsafe {
            arch::load_cr3(task_switch.pml4_phys);
        }
        None
    } else {
        Some(-11)
    }
}

pub fn input_byte(byte: u8) {
    unsafe {
        let task = scheduler::stdin_waiter_index().unwrap_or_else(current_task_index);
        let tty = &mut *TTY.0.get();
        if task < scheduler::USER_TASKS && tty.raw[task] {
            tty.push_ready(byte);
            wake_stdin_reader(tty);
            return;
        }

        match byte {
            3 => {
                tty.line_len = 0;
                echo_byte(b'^');
                echo_byte(b'C');
                echo_byte(b'\n');
                crate::linux_abi::signal_tty_foreground(2);
            }
            8 | 127 => {
                if tty.line_len > 0 {
                    tty.line_len -= 1;
                    echo_byte(8);
                }
            }
            b'\n' | b'\r' => {
                echo_byte(b'\n');
                for index in 0..tty.line_len {
                    tty.push_ready(tty.line[index]);
                }
                tty.push_ready(b'\n');
                tty.line_len = 0;
                wake_stdin_reader(tty);
            }
            byte if byte >= 0x20 && tty.line_len < LINE_CAP - 1 => {
                tty.line[tty.line_len] = byte;
                tty.line_len += 1;
                echo_byte(byte);
            }
            _ => {}
        }
    }
}

fn current_task_index() -> usize {
    scheduler::current_user_index()
        .filter(|index| *index < scheduler::USER_TASKS)
        .unwrap_or(0)
}

unsafe fn pop_ready_bytes(tty: &mut TtyState, buffer: *mut u8, len: usize) -> Option<usize> {
    if !tty.has_ready() {
        return None;
    }
    let mut count = 0usize;
    while count < len {
        let Some(byte) = tty.pop_ready() else {
            break;
        };
        *buffer.add(count) = byte;
        count += 1;
    }
    Some(count)
}

fn echo_byte(byte: u8) {
    services::gui::console_write(&[byte]);
}

unsafe fn wake_stdin_reader(tty: &mut TtyState) {
    if let Some(wake) = scheduler::wake_stdin_waiter() {
        let Some(byte) = tty.pop_ready() else {
            return;
        };
        let current_pml4 = arch::read_cr3();
        arch::load_cr3(wake.pml4_phys);
        *(wake.buffer as *mut u8) = byte;
        arch::load_cr3(current_pml4);
    }
}
