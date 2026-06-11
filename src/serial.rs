use crate::{arch, services};

const COM1: u16 = 0x3f8;

pub fn init() {
    unsafe {
        arch::outb(COM1 + 1, 0x00);
        arch::outb(COM1 + 3, 0x80);
        arch::outb(COM1, 0x03);
        arch::outb(COM1 + 1, 0x00);
        arch::outb(COM1 + 3, 0x03);
        arch::outb(COM1 + 2, 0xc7);
        arch::outb(COM1 + 4, 0x0b);
    }
}

pub fn write_line(text: &str) {
    for byte in text.bytes() {
        write_log_byte(byte);
    }
    write_log_byte(b'\n');
}

pub fn write_hex_u16(value: u16) {
    write_str("0x");
    for shift in (0..16).step_by(4).rev() {
        write_nibble(((value >> shift) & 0xf) as u8);
    }
}

pub fn write_hex_u64(value: u64) {
    write_str("0x");
    for shift in (0..64).step_by(4).rev() {
        write_nibble(((value >> shift) & 0xf) as u8);
    }
}

pub fn write_dec_u8(value: u8) {
    if value >= 100 {
        write_log_byte(b'0' + value / 100);
    }
    if value >= 10 {
        write_log_byte(b'0' + (value / 10) % 10);
    }
    write_log_byte(b'0' + value % 10);
}

pub fn write_str(text: &str) {
    for byte in text.bytes() {
        write_log_byte(byte);
    }
}

pub fn write_str_byte(byte: u8) {
    write_byte(byte);
}

fn write_byte(byte: u8) {
    unsafe {
        while (arch::inb(COM1 + 5) & 0x20) == 0 {}
        arch::outb(COM1, byte);
    }
}

fn write_log_byte(byte: u8) {
    write_byte(byte);
    services::gui::kernel_log_byte(byte);
}

fn write_nibble(value: u8) {
    let digit = match value {
        0..=9 => b'0' + value,
        _ => b'a' + (value - 10),
    };
    write_log_byte(digit);
}
