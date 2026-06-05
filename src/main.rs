#![no_std]
#![no_main]

mod arch;
mod desktop;
mod framebuffer;
mod ipc;
mod interrupts;
mod limine;
mod pci;
mod serial;
mod scheduler;
mod services;
mod userland;
mod virtio;

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let kernel = microkernel::Kernel::bootstrap();
    kernel.run()
}

mod microkernel {
    use crate::{arch, interrupts, ipc, limine, scheduler, serial, services, userland, virtio};

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
            scheduler::install(self.scheduler);
            self.ipc.publish(ipc::Message::new("kernel", "desktop", "paint"));

            if let Some(mut fb) = limine::framebuffer() {
                serial::write_line("nk: framebuffer found");
                services::gui::GuiService::new().run(&mut fb);
            } else {
                serial::write_line("nk: no framebuffer response");
            }

            interrupts::init();
            serial::write_line("nk: interrupts enabled");
            userland::init();
            userland::smoke_test_syscall();
            virtio::init();

            loop {
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
