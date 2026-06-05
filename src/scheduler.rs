use core::cell::UnsafeCell;

const USER_TASKS: usize = 4;

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
struct UserTask {
    name: &'static str,
    initial_frame: TrapFrame,
    frame: TrapFrame,
    ticks: u64,
    active: bool,
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
            initial_frame: Self::EMPTY_FRAME,
            frame: Self::EMPTY_FRAME,
            ticks: 0,
            active: false,
        }
    }
}

struct UserScheduler {
    tasks: [UserTask; USER_TASKS],
    current: usize,
    installed: usize,
}

pub struct UserTaskSnapshot {
    pub name: &'static str,
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
        }
    }

    fn install(&mut self, index: usize, name: &'static str, frame: TrapFrame) {
        if index >= USER_TASKS {
            return;
        }

        self.tasks[index] = UserTask {
            name,
            initial_frame: frame,
            frame,
            ticks: 0,
            active: true,
        };
        self.installed = self.installed.max(index + 1);
    }

    fn schedule(&mut self, frame: &mut TrapFrame) -> Option<&'static str> {
        if self.installed < 2 || frame.cs & 0x3 != 0x3 {
            return None;
        }

        self.tasks[self.current].frame = *frame;
        self.tasks[self.current].ticks = self.tasks[self.current].ticks.wrapping_add(1);

        let mut next = (self.current + 1) % self.installed;
        while !self.tasks[next].active {
            next = (next + 1) % self.installed;
        }

        self.current = next;
        *frame = self.tasks[self.current].frame;
        Some(self.tasks[self.current].name)
    }

    fn first_frame(&self) -> Option<TrapFrame> {
        if self.installed == 0 || !self.tasks[0].active {
            None
        } else {
            Some(self.tasks[0].frame)
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
            name: task.name,
            ticks: task.ticks,
            active: task.active,
            current: index == self.current,
        })
    }

    fn current_name(&self) -> Option<&'static str> {
        if self.installed == 0 || !self.tasks[self.current].active {
            None
        } else {
            Some(self.tasks[self.current].name)
        }
    }

    fn exit_current(&mut self, frame: &mut TrapFrame) -> bool {
        if self.installed == 0 {
            return false;
        }

        self.tasks[self.current].active = false;
        let mut next = (self.current + 1) % self.installed;
        for _ in 0..self.installed {
            if self.tasks[next].active {
                self.current = next;
                *frame = self.tasks[self.current].frame;
                return true;
            }
            next = (next + 1) % self.installed;
        }

        false
    }

    fn set_active(&mut self, index: usize, active: bool) {
        if index >= self.installed {
            return;
        }
        self.tasks[index].active = active;
    }

    fn restart_by_name(&mut self, name: &'static str) -> bool {
        for index in 0..self.installed {
            if self.tasks[index].name == name {
                self.tasks[index].frame = self.tasks[index].initial_frame;
                self.tasks[index].active = true;
                return true;
            }
        }
        false
    }
}

struct GlobalScheduler(UnsafeCell<Option<Scheduler>>);
struct GlobalUserScheduler(UnsafeCell<UserScheduler>);

unsafe impl Sync for GlobalScheduler {}
unsafe impl Sync for GlobalUserScheduler {}

static SCHEDULER: GlobalScheduler = GlobalScheduler(UnsafeCell::new(None));
static USER_SCHEDULER: GlobalUserScheduler = GlobalUserScheduler(UnsafeCell::new(UserScheduler::new()));

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

pub fn install_user_task(index: usize, name: &'static str, frame: TrapFrame) {
    unsafe {
        (*USER_SCHEDULER.0.get()).install(index, name, frame);
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

pub fn schedule_user(frame: &mut TrapFrame) -> Option<&'static str> {
    unsafe { (*USER_SCHEDULER.0.get()).schedule(frame) }
}

pub fn user_task_count() -> usize {
    unsafe { (*USER_SCHEDULER.0.get()).task_count() }
}

pub fn user_task_info(index: usize) -> Option<UserTaskSnapshot> {
    unsafe { (*USER_SCHEDULER.0.get()).task_info(index) }
}

pub fn current_user_name() -> Option<&'static str> {
    unsafe { (*USER_SCHEDULER.0.get()).current_name() }
}

pub fn exit_current_user(frame: &mut TrapFrame) -> bool {
    unsafe { (*USER_SCHEDULER.0.get()).exit_current(frame) }
}

pub fn restart_user_task(name: &'static str) -> bool {
    unsafe { (*USER_SCHEDULER.0.get()).restart_by_name(name) }
}
