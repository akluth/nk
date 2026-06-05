use core::cell::UnsafeCell;

use crate::arch;

struct MouseState {
    x: i16,
    y: i16,
    buttons: u8,
    packet: [u8; 3],
    index: usize,
    sequence: u8,
}

impl MouseState {
    const fn new() -> Self {
        Self {
            x: 320,
            y: 220,
            buttons: 0,
            packet: [0; 3],
            index: 0,
            sequence: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.index == 0 && byte & 0x08 == 0 {
            return;
        }

        self.packet[self.index] = byte;
        self.index += 1;
        if self.index < 3 {
            return;
        }

        self.index = 0;
        let flags = self.packet[0];
        let mut dx = self.packet[1] as i16;
        let mut dy = self.packet[2] as i16;
        if flags & 0x10 != 0 {
            dx |= !0xff;
        }
        if flags & 0x20 != 0 {
            dy |= !0xff;
        }

        self.x = (self.x + dx).clamp(0, 1279);
        self.y = (self.y - dy).clamp(0, 719);
        self.buttons = flags & 0x07;
        self.sequence = self.sequence.wrapping_add(1);
    }

    fn packed(&self) -> u64 {
        self.x as u16 as u64
            | ((self.y as u16 as u64) << 16)
            | ((self.buttons as u64) << 32)
            | ((self.sequence as u64) << 40)
    }
}

struct GlobalMouse(UnsafeCell<MouseState>);

unsafe impl Sync for GlobalMouse {}

static MOUSE: GlobalMouse = GlobalMouse(UnsafeCell::new(MouseState::new()));

pub fn init() {
    unsafe {
        wait_input();
        arch::outb(0x64, 0xa8);
        wait_input();
        arch::outb(0x64, 0x20);
        wait_output();
        let config = (arch::inb(0x60) | 0x02) & !0x20;
        wait_input();
        arch::outb(0x64, 0x60);
        wait_input();
        arch::outb(0x60, config);

        mouse_command(0xf6);
        mouse_command(0xf4);
    }
}

pub fn push_byte(byte: u8) {
    unsafe {
        (*MOUSE.0.get()).push(byte);
    }
}

pub fn packed_state() -> u64 {
    unsafe { (*MOUSE.0.get()).packed() }
}

unsafe fn mouse_command(command: u8) {
    wait_input();
    arch::outb(0x64, 0xd4);
    wait_input();
    arch::outb(0x60, command);
    wait_output();
    let _ack = arch::inb(0x60);
}

unsafe fn wait_input() {
    for _ in 0..100_000 {
        if arch::inb(0x64) & 0x02 == 0 {
            return;
        }
    }
}

unsafe fn wait_output() {
    for _ in 0..100_000 {
        if arch::inb(0x64) & 0x01 != 0 {
            return;
        }
    }
}
