use core::cell::UnsafeCell;

pub struct Task {
    pub name: &'static str,
    pub ticks: u64,
}

pub struct Scheduler {
    tasks: [Option<Task>; 8],
    cursor: usize,
    ticks: u64,
}

struct GlobalScheduler(UnsafeCell<Option<Scheduler>>);

unsafe impl Sync for GlobalScheduler {}

static SCHEDULER: GlobalScheduler = GlobalScheduler(UnsafeCell::new(None));

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
