#![no_std]
#![no_main]

mod arch;
mod desktop;
mod framebuffer;
mod ipc;
mod limine;
mod serial;
mod scheduler;

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let kernel = microkernel::Kernel::bootstrap();
    kernel.run()
}

mod microkernel {
    use crate::{arch, desktop, ipc, limine, scheduler, serial};

    pub struct Kernel {
        scheduler: scheduler::Scheduler,
        ipc: ipc::MessageBus,
    }

    impl Kernel {
        pub fn bootstrap() -> Self {
            Self {
                scheduler: scheduler::Scheduler::new(),
                ipc: ipc::MessageBus::new(),
            }
        }

        pub fn run(mut self) -> ! {
            serial::init();
            serial::write_line("nk: kernel entered");
            self.scheduler.spawn("desktop");
            self.scheduler.spawn("idle");
            self.ipc.publish(ipc::Message::new("kernel", "desktop", "paint"));

            if let Some(mut fb) = limine::framebuffer() {
                serial::write_line("nk: framebuffer found");
                desktop::Desktop::new().draw(&mut fb);
                serial::write_line("nk: desktop painted");
            } else {
                serial::write_line("nk: no framebuffer response");
            }

            loop {
                self.scheduler.tick();
                arch::halt();
            }
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        arch::halt();
    }
}
