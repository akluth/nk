use core::{cell::UnsafeCell, ptr};

pub const MAX_USER_TASKS: usize = 16;
pub const USER_TASKS: usize = MAX_USER_TASKS;
const IA32_FS_BASE: u32 = 0xc000_0100;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrapFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

pub struct Task {
    pub name: &'static str,
    pub ticks: u64,
}

pub struct Scheduler {
    tasks: [Option<Task>; 8],
    cursor: usize,
    ticks: u64,
}

#[derive(Clone, Copy)]
pub enum UserAbi {
    Native,
    Linux,
}

#[derive(Clone, Copy)]
struct UserTask {
    name: &'static str,
    pid: u64,
    parent_pid: u64,
    process_group: u64,
    session: u64,
    session_leader: bool,
    abi: UserAbi,
    pml4_phys: u64,
    initial_frame: TrapFrame,
    frame: TrapFrame,
    fs_base: u64,
    ticks: u64,
    active: bool,
    waiting_stdin: bool,
    stdin_buffer: u64,
    waiting_pipe_read: bool,
    pipe_id: u16,
    pipe_buffer: u64,
    waiting_child: bool,
    wait_pid: i32,
    wait_status: u64,
    waiting_timeout: bool,
    wake_tick: u64,
    zombie: bool,
    exit_status: i32,
}

impl UserTask {
    const EMPTY_FRAME: TrapFrame = TrapFrame {
        r15: 0,
        r14: 0,
        r13: 0,
        r12: 0,
        r11: 0,
        r10: 0,
        r9: 0,
        r8: 0,
        rdi: 0,
        rsi: 0,
        rbp: 0,
        rbx: 0,
        rdx: 0,
        rcx: 0,
        rax: 0,
        rip: 0,
        cs: 0,
        rflags: 0,
        rsp: 0,
        ss: 0,
    };

    const fn empty() -> Self {
        Self {
            name: "",
            pid: 0,
            parent_pid: 0,
            process_group: 0,
            session: 0,
            session_leader: false,
            abi: UserAbi::Native,
            pml4_phys: 0,
            initial_frame: Self::EMPTY_FRAME,
            frame: Self::EMPTY_FRAME,
            fs_base: 0,
            ticks: 0,
            active: false,
            waiting_stdin: false,
            stdin_buffer: 0,
            waiting_pipe_read: false,
            pipe_id: 0,
            pipe_buffer: 0,
            waiting_child: false,
            wait_pid: 0,
            wait_status: 0,
            waiting_timeout: false,
            wake_tick: 0,
            zombie: false,
            exit_status: 0,
        }
    }
}

const fn initial_fs_base(frame: TrapFrame) -> u64 {
    let _ = frame;
    0
}

#[derive(Clone, Copy)]
pub struct UserSwitch {
    pub name: &'static str,
    pub pml4_phys: u64,
}

#[derive(Clone, Copy)]
pub struct StdinWake {
    pub pml4_phys: u64,
    pub buffer: u64,
}

#[derive(Clone, Copy)]
pub struct PipeWake {
    pub pml4_phys: u64,
    pub buffer: u64,
}

struct UserScheduler {
    tasks: [*mut UserTask; USER_TASKS],
    current: usize,
    installed: usize,
    focus: usize,
    next_pid: u64,
    initialized: bool,
}

pub struct UserTaskSnapshot {
    pub ticks: u64,
    pub active: bool,
    pub current: bool,
}

impl UserScheduler {
    const fn new() -> Self {
        Self {
            tasks: [ptr::null_mut(); USER_TASKS],
            current: 0,
            installed: 0,
            focus: 0,
            next_pid: 2,
            initialized: false,
        }
    }

    fn init_process_table(&mut self) -> bool {
        if self.initialized {
            return true;
        }

        for index in 0..USER_TASKS {
            let Some(task) = (unsafe { crate::memory::alloc_kernel_object::<UserTask>() }) else {
                return false;
            };
            *task = UserTask::empty();
            self.tasks[index] = task as *mut UserTask;
        }
        self.initialized = true;
        crate::serial::write_str("nk: user process descriptors allocated=");
        crate::serial::write_dec_u8(USER_TASKS as u8);
        crate::serial::write_line("");
        true
    }

    fn is_ready(&self) -> bool {
        self.initialized
    }

    fn task(&self, index: usize) -> Option<&UserTask> {
        if index >= USER_TASKS || self.tasks[index].is_null() {
            None
        } else {
            Some(unsafe { &*self.tasks[index] })
        }
    }

    fn task_mut(&mut self, index: usize) -> Option<&mut UserTask> {
        if index >= USER_TASKS || self.tasks[index].is_null() {
            None
        } else {
            Some(unsafe { &mut *self.tasks[index] })
        }
    }

    fn alloc_pid(&mut self) -> u64 {
        let pid = self.next_pid;
        self.next_pid = self.next_pid.wrapping_add(1).max(2);
        pid
    }

    fn install(
        &mut self,
        index: usize,
        name: &'static str,
        abi: UserAbi,
        pml4_phys: u64,
        frame: TrapFrame,
    ) {
        if !self.is_ready() || index >= USER_TASKS {
            return;
        }

        let parent_pid = if index == 0 {
            0
        } else {
            self.current_pid().unwrap_or(0)
        };
        let pid = if index == 0 { 1 } else { self.alloc_pid() };
        let process_group = pid;
        let session = if index == 0 {
            pid
        } else {
            self.current_session().unwrap_or(pid)
        };
        let Some(task) = self.task_mut(index) else {
            return;
        };
        *task = UserTask {
            name,
            pid,
            parent_pid,
            process_group,
            session,
            session_leader: index == 0,
            abi,
            pml4_phys,
            initial_frame: frame,
            frame,
            fs_base: initial_fs_base(frame),
            ticks: 0,
            active: true,
            waiting_stdin: false,
            stdin_buffer: 0,
            waiting_pipe_read: false,
            pipe_id: 0,
            pipe_buffer: 0,
            waiting_child: false,
            wait_pid: 0,
            wait_status: 0,
            waiting_timeout: false,
            wake_tick: 0,
            zombie: false,
            exit_status: 0,
        };
        self.installed = self.installed.max(index + 1);
    }

    fn replace_frame(&mut self, index: usize, name: &'static str, abi: UserAbi, frame: TrapFrame) {
        if !self.is_ready() || index >= self.installed {
            return;
        }

        let Some(task) = self.task_mut(index) else {
            return;
        };
        task.name = name;
        task.abi = abi;
        task.initial_frame = frame;
        task.frame = frame;
        task.fs_base = initial_fs_base(frame);
        task.active = true;
        task.waiting_stdin = false;
        task.stdin_buffer = 0;
        task.waiting_pipe_read = false;
        task.pipe_id = 0;
        task.pipe_buffer = 0;
        task.waiting_child = false;
        task.wait_pid = 0;
        task.wait_status = 0;
        task.waiting_timeout = false;
        task.wake_tick = 0;
        task.zombie = false;
        task.exit_status = 0;
    }

    fn allocate_child_slot(&self) -> Option<usize> {
        if !self.is_ready() {
            return None;
        }
        for index in 0..USER_TASKS {
            let Some(task) = self.task(index) else {
                continue;
            };
            if task.pid == 0
                || (!task.active
                    && !task.waiting_stdin
                    && !task.waiting_pipe_read
                    && !task.waiting_child
                    && !task.waiting_timeout
                    && !task.zombie)
            {
                return Some(index);
            }
        }
        None
    }

    fn schedule(&mut self, frame: &mut TrapFrame) -> Option<UserSwitch> {
        if !self.is_ready() || self.installed < 2 || frame.cs & 0x3 != 0x3 {
            return None;
        }

        self.task_mut(self.current)?.frame = *frame;
        self.save_fs_base(self.current);
        let task = self.task_mut(self.current)?;
        task.ticks = task.ticks.wrapping_add(1);

        let Some(next) = self.next_active_from((self.current + 1) % self.installed) else {
            return None;
        };

        self.current = next;
        self.restore_fs_base(self.current);
        let task = *self.task(self.current)?;
        *frame = task.frame;
        Some(UserSwitch {
            name: task.name,
            pml4_phys: task.pml4_phys,
        })
    }

    fn next_active_from(&self, mut next: usize) -> Option<usize> {
        for _ in 0..self.installed {
            if self.task(next).map_or(false, |task| task.active) {
                return Some(next);
            }
            next = (next + 1) % self.installed;
        }
        None
    }

    fn block_current_for_stdin(
        &mut self,
        frame: &mut TrapFrame,
        buffer: u64,
    ) -> Option<UserSwitch> {
        if !self.is_ready() || self.installed < 2 || frame.cs & 0x3 != 0x3 {
            return None;
        }

        let current = self.current;
        let next = self.next_active_from((current + 1) % self.installed)?;
        self.task_mut(current)?.frame = *frame;
        self.save_fs_base(current);
        let task = self.task_mut(current)?;
        task.active = false;
        task.waiting_stdin = true;
        task.stdin_buffer = buffer;
        self.current = next;
        self.restore_fs_base(self.current);
        let task = *self.task(self.current)?;
        *frame = task.frame;
        Some(UserSwitch {
            name: task.name,
            pml4_phys: task.pml4_phys,
        })
    }

    fn block_current_for_pipe_read(
        &mut self,
        frame: &mut TrapFrame,
        pipe_id: u16,
        buffer: u64,
    ) -> Option<UserSwitch> {
        if !self.is_ready() || self.installed < 2 || frame.cs & 0x3 != 0x3 {
            return None;
        }

        let current = self.current;
        let next = self.next_active_from((current + 1) % self.installed)?;
        self.task_mut(current)?.frame = *frame;
        self.save_fs_base(current);
        let task = self.task_mut(current)?;
        task.active = false;
        task.waiting_pipe_read = true;
        task.pipe_id = pipe_id;
        task.pipe_buffer = buffer;
        self.current = next;
        self.restore_fs_base(self.current);
        let task = *self.task(self.current)?;
        *frame = task.frame;
        Some(UserSwitch {
            name: task.name,
            pml4_phys: task.pml4_phys,
        })
    }

    fn block_current_for_child(
        &mut self,
        frame: &mut TrapFrame,
        wait_pid: i32,
        wait_status: u64,
    ) -> Option<UserSwitch> {
        if !self.is_ready() || self.installed < 2 || frame.cs & 0x3 != 0x3 {
            return None;
        }

        let current = self.current;
        let next = self.next_active_from((current + 1) % self.installed)?;
        self.task_mut(current)?.frame = *frame;
        self.save_fs_base(current);
        let task = self.task_mut(current)?;
        task.active = false;
        task.waiting_child = true;
        task.wait_pid = wait_pid;
        task.wait_status = wait_status;
        self.current = next;
        self.restore_fs_base(self.current);
        let task = *self.task(self.current)?;
        *frame = task.frame;
        Some(UserSwitch {
            name: task.name,
            pml4_phys: task.pml4_phys,
        })
    }

    fn block_current_for_timeout(
        &mut self,
        frame: &mut TrapFrame,
        ticks: u64,
        now: u64,
    ) -> Option<UserSwitch> {
        if !self.is_ready() || self.installed < 2 || frame.cs & 0x3 != 0x3 || ticks == 0 {
            return None;
        }

        let current = self.current;
        let next = self.next_active_from((current + 1) % self.installed)?;
        self.task_mut(current)?.frame = *frame;
        self.save_fs_base(current);
        let task = self.task_mut(current)?;
        task.active = false;
        task.waiting_timeout = true;
        task.wake_tick = now.saturating_add(ticks).max(now + 1);
        self.current = next;
        self.restore_fs_base(self.current);
        let task = *self.task(self.current)?;
        *frame = task.frame;
        Some(UserSwitch {
            name: task.name,
            pml4_phys: task.pml4_phys,
        })
    }

    fn wake_stdin_waiter(&mut self) -> Option<StdinWake> {
        if !self.is_ready() {
            return None;
        }
        for index in 0..self.installed {
            let Some(task) = self.task_mut(index) else {
                continue;
            };
            if task.waiting_stdin {
                task.waiting_stdin = false;
                task.active = true;
                task.frame.rax = 1;
                return Some(StdinWake {
                    pml4_phys: task.pml4_phys,
                    buffer: task.stdin_buffer,
                });
            }
        }
        None
    }

    fn wake_pipe_reader(&mut self, pipe_id: u16, result: i64) -> Option<PipeWake> {
        if !self.is_ready() {
            return None;
        }
        for index in 0..self.installed {
            let Some(task) = self.task_mut(index) else {
                continue;
            };
            if task.waiting_pipe_read && task.pipe_id == pipe_id {
                task.waiting_pipe_read = false;
                task.active = true;
                task.frame.rax = result as u64;
                return Some(PipeWake {
                    pml4_phys: task.pml4_phys,
                    buffer: task.pipe_buffer,
                });
            }
        }
        None
    }

    fn stdin_waiter_index(&self) -> Option<usize> {
        if !self.is_ready() {
            return None;
        }
        for index in 0..self.installed {
            let Some(task) = self.task(index) else {
                continue;
            };
            if task.waiting_stdin {
                return Some(index);
            }
        }
        None
    }

    fn wake_timeouts(&mut self, now: u64) {
        if !self.is_ready() {
            return;
        }
        for index in 0..self.installed {
            let Some(task) = self.task_mut(index) else {
                continue;
            };
            if task.waiting_timeout && now >= task.wake_tick {
                task.waiting_timeout = false;
                task.wake_tick = 0;
                task.active = true;
                task.frame.rax = 0;
            }
        }
    }

    fn fork_current_to(&mut self, child: usize, child_pml4: u64, frame: &TrapFrame) -> Option<u64> {
        if !self.is_ready() || child >= USER_TASKS || self.installed == 0 {
            return None;
        }

        self.save_fs_base(self.current);
        let parent = *self.task(self.current)?;
        let parent_pid = parent.pid;
        let child_pid = self.alloc_pid();
        let mut child_task = parent;
        child_task.name = "child";
        child_task.pid = child_pid;
        child_task.parent_pid = parent_pid;
        child_task.process_group = parent.process_group;
        child_task.session = parent.session;
        child_task.session_leader = false;
        child_task.pml4_phys = child_pml4;
        child_task.frame = *frame;
        child_task.frame.rax = 0;
        child_task.initial_frame = child_task.frame;
        child_task.active = true;
        child_task.waiting_stdin = false;
        child_task.stdin_buffer = 0;
        child_task.waiting_pipe_read = false;
        child_task.pipe_id = 0;
        child_task.pipe_buffer = 0;
        child_task.waiting_child = false;
        child_task.wait_pid = 0;
        child_task.wait_status = 0;
        child_task.waiting_timeout = false;
        child_task.wake_tick = 0;
        child_task.zombie = false;
        child_task.exit_status = 0;
        *self.task_mut(child)? = child_task;
        self.installed = self.installed.max(child + 1);
        Some(child_pid)
    }

    fn first_frame(&self) -> Option<TrapFrame> {
        if !self.is_ready() || self.installed == 0 || !self.task(0)?.active {
            None
        } else {
            Some(self.task(0)?.frame)
        }
    }

    fn first_pml4(&self) -> Option<u64> {
        if !self.is_ready() || self.installed == 0 || !self.task(0)?.active {
            None
        } else {
            Some(self.task(0)?.pml4_phys)
        }
    }

    fn task_count(&self) -> usize {
        self.installed
    }

    fn task_info(&self, index: usize) -> Option<UserTaskSnapshot> {
        if index >= self.installed {
            return None;
        }

        let task = *self.task(index)?;
        Some(UserTaskSnapshot {
            ticks: task.ticks,
            active: task.active
                || task.waiting_stdin
                || task.waiting_pipe_read
                || task.waiting_child
                || task.waiting_timeout
                || task.zombie,
            current: index == self.current,
        })
    }

    fn current_abi(&self) -> Option<UserAbi> {
        if !self.is_ready() || self.installed == 0 || !self.task(self.current)?.active {
            None
        } else {
            Some(self.task(self.current)?.abi)
        }
    }

    fn current_pid(&self) -> Option<u64> {
        if !self.is_ready() || self.installed == 0 {
            None
        } else {
            let pid = self.task(self.current)?.pid;
            if pid == 0 {
                None
            } else {
                Some(pid)
            }
        }
    }

    fn current_parent_pid(&self) -> Option<u64> {
        if !self.is_ready() || self.installed == 0 {
            None
        } else {
            let task = *self.task(self.current)?;
            if task.pid == 0 {
                None
            } else {
                Some(task.parent_pid)
            }
        }
    }

    fn current_process_group(&self) -> Option<u64> {
        if !self.is_ready() || self.installed == 0 {
            None
        } else {
            let pgid = self.task(self.current)?.process_group;
            if pgid == 0 {
                None
            } else {
                Some(pgid)
            }
        }
    }

    fn current_session(&self) -> Option<u64> {
        if !self.is_ready() || self.installed == 0 {
            None
        } else {
            let session = self.task(self.current)?.session;
            if session == 0 {
                None
            } else {
                Some(session)
            }
        }
    }

    fn task_index_by_pid(&self, pid: u64) -> Option<usize> {
        if pid == 0 {
            return Some(self.current);
        }
        for index in 0..self.installed {
            let task = self.task(index)?;
            if task.pid == pid {
                return Some(index);
            }
        }
        None
    }

    fn task_pid(&self, index: usize) -> Option<u64> {
        let pid = self.task(index)?.pid;
        if pid == 0 {
            None
        } else {
            Some(pid)
        }
    }

    fn task_process_group(&self, index: usize) -> Option<u64> {
        let pgid = self.task(index)?.process_group;
        if pgid == 0 {
            None
        } else {
            Some(pgid)
        }
    }

    fn process_group_for_pid(&self, pid: u64) -> Option<u64> {
        let index = self.task_index_by_pid(pid)?;
        let pgid = self.task(index)?.process_group;
        if pgid == 0 {
            None
        } else {
            Some(pgid)
        }
    }

    fn session_for_pid(&self, pid: u64) -> Option<u64> {
        let index = self.task_index_by_pid(pid)?;
        let session = self.task(index)?.session;
        if session == 0 {
            None
        } else {
            Some(session)
        }
    }

    fn set_process_group(&mut self, pid: u64, pgid: u64) -> bool {
        if !self.is_ready() {
            return false;
        }
        let Some(index) = self.task_index_by_pid(pid) else {
            return false;
        };
        let target = if pgid == 0 {
            self.task(index).map_or(0, |task| task.pid)
        } else {
            pgid
        };
        let current_session = self.current_session().unwrap_or(0);
        let Some(task) = self.task_mut(index) else {
            return false;
        };
        if target == 0 || task.session != current_session || task.session_leader {
            return false;
        }
        task.process_group = target;
        true
    }

    fn create_session(&mut self) -> Option<u64> {
        if !self.is_ready() || self.installed == 0 {
            return None;
        }
        let task = self.task(self.current)?;
        let pid = task.pid;
        if pid == 0 || task.process_group == pid {
            return None;
        }
        let task = self.task_mut(self.current)?;
        task.session = pid;
        task.process_group = pid;
        task.session_leader = true;
        Some(pid)
    }

    fn current_pml4(&self) -> Option<u64> {
        if !self.is_ready() || self.installed == 0 {
            None
        } else {
            let pml4 = self.task(self.current)?.pml4_phys;
            if pml4 == 0 {
                None
            } else {
                Some(pml4)
            }
        }
    }

    fn exit_current(&mut self, frame: &mut TrapFrame, status: i32) -> Option<u64> {
        if !self.is_ready() || self.installed == 0 {
            return None;
        }

        let exiting = self.current;
        let exiting_task = *self.task(exiting)?;
        let exiting_pid = exiting_task.pid;
        let parent_pid = exiting_task.parent_pid;
        self.save_fs_base(exiting);
        {
            let task = self.task_mut(exiting)?;
            task.active = false;
            task.waiting_stdin = false;
            task.stdin_buffer = 0;
            task.waiting_pipe_read = false;
            task.pipe_id = 0;
            task.pipe_buffer = 0;
            task.waiting_child = false;
            task.wait_pid = 0;
            task.wait_status = 0;
            task.waiting_timeout = false;
            task.wake_tick = 0;
            task.zombie = true;
            task.exit_status = status;
        }

        let mut awakened_parent = None;
        for parent in 0..self.installed {
            let Some(parent_task) = self.task(parent) else {
                continue;
            };
            if parent_task.pid == parent_pid
                && parent_task.waiting_child
                && wait_pid_matches(parent_task.wait_pid, exiting_pid)
            {
                let parent_task = self.task_mut(parent)?;
                parent_task.waiting_child = false;
                parent_task.wait_pid = 0;
                parent_task.active = true;
                parent_task.frame.rax = exiting_pid;
                self.write_wait_status(parent, status);
                let exiting_task = self.task_mut(exiting)?;
                exiting_task.zombie = false;
                exiting_task.pid = 0;
                exiting_task.parent_pid = 0;
                exiting_task.process_group = 0;
                exiting_task.session = 0;
                exiting_task.session_leader = false;
                awakened_parent = Some(parent);
                break;
            }
        }

        if let Some(parent) = awakened_parent {
            self.current = parent;
            self.focus = parent;
            self.restore_fs_base(parent);
            let parent_task = *self.task(parent)?;
            *frame = parent_task.frame;
            return Some(parent_task.pml4_phys);
        }

        if exiting != 0 && self.installed > 0 && self.task(0).map_or(false, |task| task.active) {
            self.current = 0;
            self.focus = 0;
            self.restore_fs_base(0);
            let task = *self.task(0)?;
            *frame = task.frame;
            return Some(task.pml4_phys);
        }

        self.switch_to_next(frame)
    }

    fn switch_to_next(&mut self, frame: &mut TrapFrame) -> Option<u64> {
        let next = self.next_active_from((self.current + 1) % self.installed)?;
        self.current = next;
        self.focus = next;
        self.restore_fs_base(self.current);
        let task = *self.task(self.current)?;
        *frame = task.frame;
        Some(task.pml4_phys)
    }

    fn save_fs_base(&mut self, index: usize) {
        if index < self.installed {
            if let Some(task) = self.task_mut(index) {
                task.fs_base = unsafe { crate::arch::rdmsr(IA32_FS_BASE) };
            }
        }
    }

    fn restore_fs_base(&self, index: usize) {
        if index < self.installed {
            if let Some(task) = self.task(index) {
                unsafe {
                    crate::arch::wrmsr(IA32_FS_BASE, task.fs_base);
                }
            }
        }
    }

    fn write_wait_status(&mut self, parent: usize, status: i32) {
        let Some(parent_task) = self.task(parent) else {
            return;
        };
        let address = parent_task.wait_status;
        if address == 0 {
            return;
        }
        unsafe {
            let current_cr3 = crate::arch::read_cr3();
            crate::arch::load_cr3(parent_task.pml4_phys);
            *(address as *mut i32) = (status & 0xff) << 8;
            crate::arch::load_cr3(current_cr3);
        }
        if let Some(parent_task) = self.task_mut(parent) {
            parent_task.wait_status = 0;
        }
    }

    fn wait_for_child(&mut self, frame: &mut TrapFrame, pid: i32) -> WaitResult {
        let Some(child) = self.find_waitable_child(pid, true) else {
            if self.find_waitable_child(pid, false).is_some() {
                if let Some(task_switch) = self.block_current_for_child(frame, pid, frame.rsi) {
                    return WaitResult::Blocked(task_switch);
                }
            }
            return WaitResult::NoChild;
        };

        if self.task(child).map_or(false, |task| task.zombie) {
            let Some(child_task) = self.task(child).copied() else {
                return WaitResult::NoChild;
            };
            let child_pid = child_task.pid;
            let status = child_task.exit_status;
            let Some(child_task) = self.task_mut(child) else {
                return WaitResult::NoChild;
            };
            child_task.zombie = false;
            child_task.active = false;
            child_task.waiting_stdin = false;
            child_task.waiting_pipe_read = false;
            child_task.waiting_child = false;
            child_task.waiting_timeout = false;
            child_task.wake_tick = 0;
            child_task.pid = 0;
            child_task.parent_pid = 0;
            child_task.process_group = 0;
            child_task.session = 0;
            child_task.session_leader = false;
            self.write_wait_status(self.current, status);
            return WaitResult::Exited(child_pid);
        }

        if let Some(task_switch) = self.block_current_for_child(frame, pid, frame.rsi) {
            return WaitResult::Blocked(task_switch);
        }

        WaitResult::NoChild
    }

    fn find_waitable_child(&self, requested_pid: i32, zombie_only: bool) -> Option<usize> {
        let parent_pid = self.task(self.current)?.pid;
        if parent_pid == 0 {
            return None;
        }
        for index in 0..self.installed {
            let Some(task) = self.task(index) else {
                continue;
            };
            if task.pid == 0 || task.parent_pid != parent_pid {
                continue;
            }
            if requested_pid > 0 && task.pid != requested_pid as u64 {
                continue;
            }
            let waitable = task.zombie
                || (!zombie_only
                    && (task.active
                        || task.waiting_stdin
                        || task.waiting_pipe_read
                        || task.waiting_timeout
                        || task.waiting_child));
            if waitable {
                return Some(index);
            }
        }
        None
    }

    fn restart(&mut self, index: usize) -> bool {
        if index < self.installed {
            let Some(task) = self.task_mut(index) else {
                return false;
            };
            task.frame = task.initial_frame;
            task.active = true;
            task.waiting_stdin = false;
            task.stdin_buffer = 0;
            task.waiting_pipe_read = false;
            task.pipe_id = 0;
            task.pipe_buffer = 0;
            task.waiting_child = false;
            task.wait_pid = 0;
            task.wait_status = 0;
            task.waiting_timeout = false;
            task.wake_tick = 0;
            task.zombie = false;
            task.exit_status = 0;
            self.focus = index;
            true
        } else {
            false
        }
    }

    fn set_focus(&mut self, index: usize) {
        if index < self.installed {
            self.focus = index;
        }
    }

    fn focus(&self) -> usize {
        self.focus
    }

    fn task_running_or_waiting(&self, index: usize) -> bool {
        if index >= self.installed {
            return false;
        }
        self.task(index).map_or(false, |task| {
            task.active
                || task.waiting_stdin
                || task.waiting_pipe_read
                || task.waiting_child
                || task.waiting_timeout
        })
    }

    fn reap_task(&mut self, index: usize) {
        if index >= self.installed {
            return;
        }
        let Some(task) = self.task_mut(index) else {
            return;
        };
        if task.zombie {
            task.zombie = false;
            task.active = false;
            task.waiting_stdin = false;
            task.waiting_pipe_read = false;
            task.pipe_id = 0;
            task.pipe_buffer = 0;
            task.waiting_child = false;
            task.wait_pid = 0;
            task.waiting_timeout = false;
            task.wake_tick = 0;
            task.pid = 0;
            task.parent_pid = 0;
            task.process_group = 0;
            task.session = 0;
            task.session_leader = false;
        }
    }
}

pub enum WaitResult {
    Exited(u64),
    Blocked(UserSwitch),
    NoChild,
}

fn wait_pid_matches(requested_pid: i32, child_pid: u64) -> bool {
    requested_pid <= 0 || child_pid == requested_pid as u64
}

struct GlobalScheduler(UnsafeCell<Option<Scheduler>>);
struct GlobalUserScheduler(UnsafeCell<UserScheduler>);

unsafe impl Sync for GlobalScheduler {}
unsafe impl Sync for GlobalUserScheduler {}

static SCHEDULER: GlobalScheduler = GlobalScheduler(UnsafeCell::new(None));
static USER_SCHEDULER: GlobalUserScheduler =
    GlobalUserScheduler(UnsafeCell::new(UserScheduler::new()));

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            tasks: [None, None, None, None, None, None, None, None],
            cursor: 0,
            ticks: 0,
        }
    }

    pub fn spawn(&mut self, name: &'static str) {
        for slot in &mut self.tasks {
            if slot.is_none() {
                *slot = Some(Task { name, ticks: 0 });
                return;
            }
        }
    }

    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        self.cursor = (self.cursor + 1) % self.tasks.len();
        if let Some(task) = &mut self.tasks[self.cursor] {
            let _active = task.name;
            task.ticks = task.ticks.wrapping_add(1);
        }
    }

    pub const fn ticks(&self) -> u64 {
        self.ticks
    }
}

pub fn install(scheduler: Scheduler) {
    unsafe {
        *SCHEDULER.0.get() = Some(scheduler);
    }
}

pub fn tick() -> u64 {
    unsafe {
        if let Some(scheduler) = (*SCHEDULER.0.get()).as_mut() {
            scheduler.tick();
            let ticks = scheduler.ticks();
            (*USER_SCHEDULER.0.get()).wake_timeouts(ticks);
            ticks
        } else {
            0
        }
    }
}

pub fn ticks() -> u64 {
    unsafe {
        if let Some(scheduler) = (*SCHEDULER.0.get()).as_ref() {
            scheduler.ticks()
        } else {
            0
        }
    }
}

pub fn install_user_task(
    index: usize,
    name: &'static str,
    abi: UserAbi,
    pml4_phys: u64,
    frame: TrapFrame,
) {
    unsafe {
        (*USER_SCHEDULER.0.get()).install(index, name, abi, pml4_phys, frame);
    }
}

pub fn init_user_process_table() -> bool {
    unsafe { (*USER_SCHEDULER.0.get()).init_process_table() }
}

pub fn replace_user_task_frame(index: usize, name: &'static str, abi: UserAbi, frame: TrapFrame) {
    unsafe {
        (*USER_SCHEDULER.0.get()).replace_frame(index, name, abi, frame);
    }
}

pub fn first_user_frame() -> Option<TrapFrame> {
    unsafe { (*USER_SCHEDULER.0.get()).first_frame() }
}

pub fn first_user_pml4() -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).first_pml4() }
}

pub fn schedule_user(frame: &mut TrapFrame) -> Option<UserSwitch> {
    unsafe { (*USER_SCHEDULER.0.get()).schedule(frame) }
}

pub fn block_current_for_stdin(frame: &mut TrapFrame, buffer: u64) -> Option<UserSwitch> {
    unsafe { (*USER_SCHEDULER.0.get()).block_current_for_stdin(frame, buffer) }
}

pub fn block_current_for_pipe_read(
    frame: &mut TrapFrame,
    pipe_id: u16,
    buffer: u64,
) -> Option<UserSwitch> {
    unsafe { (*USER_SCHEDULER.0.get()).block_current_for_pipe_read(frame, pipe_id, buffer) }
}

pub fn wake_stdin_waiter() -> Option<StdinWake> {
    unsafe { (*USER_SCHEDULER.0.get()).wake_stdin_waiter() }
}

pub fn wake_pipe_reader(pipe_id: u16, result: i64) -> Option<PipeWake> {
    unsafe { (*USER_SCHEDULER.0.get()).wake_pipe_reader(pipe_id, result) }
}

pub fn stdin_waiter_index() -> Option<usize> {
    unsafe { (*USER_SCHEDULER.0.get()).stdin_waiter_index() }
}

pub fn fork_current_user_to(child: usize, child_pml4: u64, frame: &TrapFrame) -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).fork_current_to(child, child_pml4, frame) }
}

pub fn allocate_child_slot() -> Option<usize> {
    unsafe { (*USER_SCHEDULER.0.get()).allocate_child_slot() }
}

pub fn wait_for_child(frame: &mut TrapFrame, pid: i32) -> WaitResult {
    unsafe { (*USER_SCHEDULER.0.get()).wait_for_child(frame, pid) }
}

pub fn block_current_for_spawn(frame: &mut TrapFrame) -> Option<UserSwitch> {
    unsafe { (*USER_SCHEDULER.0.get()).block_current_for_child(frame, -1, 0) }
}

pub fn block_current_for_timeout(
    frame: &mut TrapFrame,
    ticks: u64,
    now: u64,
) -> Option<UserSwitch> {
    unsafe { (*USER_SCHEDULER.0.get()).block_current_for_timeout(frame, ticks, now) }
}

pub fn current_user_index() -> Option<usize> {
    unsafe {
        let scheduler = &*USER_SCHEDULER.0.get();
        if !scheduler.is_ready() || scheduler.installed == 0 {
            None
        } else {
            Some(scheduler.current)
        }
    }
}

pub fn user_task_count() -> usize {
    unsafe { (*USER_SCHEDULER.0.get()).task_count() }
}

pub fn user_task_info(index: usize) -> Option<UserTaskSnapshot> {
    unsafe { (*USER_SCHEDULER.0.get()).task_info(index) }
}

pub fn current_user_abi() -> Option<UserAbi> {
    unsafe { (*USER_SCHEDULER.0.get()).current_abi() }
}

pub fn current_user_pid() -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).current_pid() }
}

pub fn current_user_parent_pid() -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).current_parent_pid() }
}

pub fn current_user_process_group() -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).current_process_group() }
}

pub fn process_group_for_pid(pid: u64) -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).process_group_for_pid(pid) }
}

pub fn task_index_for_pid(pid: u64) -> Option<usize> {
    unsafe { (*USER_SCHEDULER.0.get()).task_index_by_pid(pid) }
}

pub fn task_pid(index: usize) -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).task_pid(index) }
}

pub fn task_process_group(index: usize) -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).task_process_group(index) }
}

pub fn session_for_pid(pid: u64) -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).session_for_pid(pid) }
}

pub fn set_process_group(pid: u64, pgid: u64) -> bool {
    unsafe { (*USER_SCHEDULER.0.get()).set_process_group(pid, pgid) }
}

pub fn create_session() -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).create_session() }
}

pub fn current_user_pml4() -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).current_pml4() }
}

pub fn exit_current_user(frame: &mut TrapFrame, status: i32) -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).exit_current(frame, status) }
}

pub fn restart_user_task(index: usize) -> bool {
    unsafe { (*USER_SCHEDULER.0.get()).restart(index) }
}

pub fn set_focus(index: usize) {
    unsafe {
        (*USER_SCHEDULER.0.get()).set_focus(index);
    }
}

pub fn focus() -> usize {
    unsafe { (*USER_SCHEDULER.0.get()).focus() }
}

pub fn user_task_running_or_waiting(index: usize) -> bool {
    unsafe { (*USER_SCHEDULER.0.get()).task_running_or_waiting(index) }
}

pub fn reap_user_task(index: usize) {
    unsafe {
        (*USER_SCHEDULER.0.get()).reap_task(index);
    }
}
