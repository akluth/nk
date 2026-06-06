use core::cell::UnsafeCell;

pub const USER_TASKS: usize = 4;

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
    abi: UserAbi,
    pml4_phys: u64,
    initial_frame: TrapFrame,
    frame: TrapFrame,
    ticks: u64,
    active: bool,
    waiting_stdin: bool,
    stdin_buffer: u64,
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
            abi: UserAbi::Native,
            pml4_phys: 0,
            initial_frame: Self::EMPTY_FRAME,
            frame: Self::EMPTY_FRAME,
            ticks: 0,
            active: false,
            waiting_stdin: false,
            stdin_buffer: 0,
        }
    }
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

struct UserScheduler {
    tasks: [UserTask; USER_TASKS],
    current: usize,
    installed: usize,
    focus: usize,
}

pub struct UserTaskSnapshot {
    pub ticks: u64,
    pub active: bool,
    pub current: bool,
}

impl UserScheduler {
    const fn new() -> Self {
        Self {
            tasks: [UserTask::empty(); USER_TASKS],
            current: 0,
            installed: 0,
            focus: 1,
        }
    }

    fn install(
        &mut self,
        index: usize,
        name: &'static str,
        abi: UserAbi,
        pml4_phys: u64,
        frame: TrapFrame,
    ) {
        if index >= USER_TASKS {
            return;
        }

        self.tasks[index] = UserTask {
            name,
            abi,
            pml4_phys,
            initial_frame: frame,
            frame,
            ticks: 0,
            active: true,
            waiting_stdin: false,
            stdin_buffer: 0,
        };
        self.installed = self.installed.max(index + 1);
    }

    fn schedule(&mut self, frame: &mut TrapFrame) -> Option<UserSwitch> {
        if self.installed < 2 || frame.cs & 0x3 != 0x3 {
            return None;
        }

        self.tasks[self.current].frame = *frame;
        self.tasks[self.current].ticks = self.tasks[self.current].ticks.wrapping_add(1);

        let Some(next) = self.next_active_from((self.current + 1) % self.installed) else {
            return None;
        };

        self.current = next;
        *frame = self.tasks[self.current].frame;
        Some(UserSwitch {
            name: self.tasks[self.current].name,
            pml4_phys: self.tasks[self.current].pml4_phys,
        })
    }

    fn next_active_from(&self, mut next: usize) -> Option<usize> {
        for _ in 0..self.installed {
            if self.tasks[next].active {
                return Some(next);
            }
            next = (next + 1) % self.installed;
        }
        None
    }

    fn block_current_for_stdin(&mut self, frame: &mut TrapFrame, buffer: u64) -> Option<UserSwitch> {
        if self.installed < 2 || frame.cs & 0x3 != 0x3 {
            return None;
        }

        let current = self.current;
        let next = self.next_active_from((current + 1) % self.installed)?;
        self.tasks[current].frame = *frame;
        self.tasks[current].active = false;
        self.tasks[current].waiting_stdin = true;
        self.tasks[current].stdin_buffer = buffer;
        self.current = next;
        *frame = self.tasks[self.current].frame;
        Some(UserSwitch {
            name: self.tasks[self.current].name,
            pml4_phys: self.tasks[self.current].pml4_phys,
        })
    }

    fn wake_stdin_waiter(&mut self) -> Option<StdinWake> {
        for task in &mut self.tasks[..self.installed] {
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

    fn first_frame(&self) -> Option<TrapFrame> {
        if self.installed == 0 || !self.tasks[0].active {
            None
        } else {
            Some(self.tasks[0].frame)
        }
    }

    fn first_pml4(&self) -> Option<u64> {
        if self.installed == 0 || !self.tasks[0].active {
            None
        } else {
            Some(self.tasks[0].pml4_phys)
        }
    }

    fn task_count(&self) -> usize {
        self.installed
    }

    fn task_info(&self, index: usize) -> Option<UserTaskSnapshot> {
        if index >= self.installed {
            return None;
        }

        let task = self.tasks[index];
        Some(UserTaskSnapshot {
            ticks: task.ticks,
            active: task.active || task.waiting_stdin,
            current: index == self.current,
        })
    }

    fn current_abi(&self) -> Option<UserAbi> {
        if self.installed == 0 || !self.tasks[self.current].active {
            None
        } else {
            Some(self.tasks[self.current].abi)
        }
    }

    fn exit_current(&mut self, frame: &mut TrapFrame) -> Option<u64> {
        if self.installed == 0 {
            return None;
        }

        self.tasks[self.current].active = false;
        self.tasks[self.current].waiting_stdin = false;
        self.tasks[self.current].stdin_buffer = 0;
        let mut next = (self.current + 1) % self.installed;
        for _ in 0..self.installed {
            if self.tasks[next].active {
                self.current = next;
                self.focus = next;
                *frame = self.tasks[self.current].frame;
                return Some(self.tasks[self.current].pml4_phys);
            }
            next = (next + 1) % self.installed;
        }

        None
    }

    fn set_active(&mut self, index: usize, active: bool) {
        if index >= self.installed {
            return;
        }
        self.tasks[index].active = active;
        if active {
            self.tasks[index].waiting_stdin = false;
            self.tasks[index].stdin_buffer = 0;
        }
    }

    fn restart(&mut self, index: usize) -> bool {
        if index < self.installed {
            self.tasks[index].frame = self.tasks[index].initial_frame;
            self.tasks[index].active = true;
            self.tasks[index].waiting_stdin = false;
            self.tasks[index].stdin_buffer = 0;
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

pub fn set_user_task_active(index: usize, active: bool) {
    unsafe {
        (*USER_SCHEDULER.0.get()).set_active(index, active);
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

pub fn wake_stdin_waiter() -> Option<StdinWake> {
    unsafe { (*USER_SCHEDULER.0.get()).wake_stdin_waiter() }
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

pub fn exit_current_user(frame: &mut TrapFrame) -> Option<u64> {
    unsafe { (*USER_SCHEDULER.0.get()).exit_current(frame) }
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
