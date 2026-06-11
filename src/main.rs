#![no_std]
#![no_main]

mod arch;
mod ata;
mod font;
mod framebuffer;
mod gdt;
mod interrupts;
mod ipc;
mod keyboard;
mod limine;
mod linux_abi;
mod memory;
mod mouse;
mod nkfs;
mod pci;
mod scheduler;
mod serial;
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
    use crate::{
        arch, ata, gdt, interrupts, ipc, limine, memory, nkfs, scheduler, serial, services,
        userland, virtio,
    };

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
            unsafe {
                arch::enable_sse();
            }
            serial::write_line("nk: sse enabled");
            gdt::init();
            self.scheduler.spawn("desktop");
            self.scheduler.spawn("idle");
            scheduler::install(self.scheduler);
            self.ipc
                .publish(ipc::Message::new("kernel", "desktop", "paint"));

            let mut framebuffer_mapping = None;
            if let Some(fb) = limine::framebuffer() {
                serial::write_line("nk: framebuffer found");
                if let Some(hhdm_offset) = limine::hhdm_offset() {
                    framebuffer_mapping = Some(memory::FramebufferMapping {
                        virt: fb.address(),
                        phys: fb.address() - hhdm_offset,
                        len: fb.byte_len(),
                    });
                }
                services::gui::install(fb);
            } else {
                serial::write_line("nk: no framebuffer response");
            }

            interrupts::init();
            serial::write_line("nk: interrupts enabled");
            userland::init();
            let mut can_enter_user = false;
            if let Some(kernel_address) = limine::kernel_address() {
                let roots = memory::create_user_address_spaces(kernel_address, framebuffer_mapping);
                userland::install_page_table_roots(roots);
                can_enter_user = true;
            } else {
                serial::write_line("nk: no kernel address response");
            }
            userland::smoke_test_syscall();
            virtio::init();
            ata::smoke_test();
            nkfs::smoke_test();
            if can_enter_user {
                userland::install_first_task();
                userland::start_first_task();
            }

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
