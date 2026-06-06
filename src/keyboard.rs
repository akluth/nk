use core::cell::UnsafeCell;

const BUFFER_LEN: usize = 64;

struct KeyboardBuffer {
    bytes: [u8; BUFFER_LEN],
    read: usize,
    write: usize,
}

impl KeyboardBuffer {
    const fn new() -> Self {
        Self {
            bytes: [0; BUFFER_LEN],
            read: 0,
            write: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        let next = (self.write + 1) % BUFFER_LEN;
        if next == self.read {
            return;
        }

        self.bytes[self.write] = byte;
        self.write = next;
    }

    fn pop(&mut self) -> Option<u8> {
        if self.read == self.write {
            return None;
        }

        let byte = self.bytes[self.read];
        self.read = (self.read + 1) % BUFFER_LEN;
        Some(byte)
    }
}

struct GlobalKeyboard(UnsafeCell<KeyboardBuffer>);

unsafe impl Sync for GlobalKeyboard {}

static KEYBOARD: GlobalKeyboard = GlobalKeyboard(UnsafeCell::new(KeyboardBuffer::new()));

pub fn push_key(byte: u8) {
    unsafe {
        (*KEYBOARD.0.get()).push(byte);
    }
}

pub fn decode_scancode(scancode: u8) -> Option<u8> {
    if scancode & 0x80 != 0 {
        return None;
    }

    decode(scancode)
}

pub fn pop_key() -> Option<u8> {
    unsafe { (*KEYBOARD.0.get()).pop() }
}

fn decode(scancode: u8) -> Option<u8> {
    match scancode {
        0x02 => Some(b'1'),
        0x03 => Some(b'2'),
        0x04 => Some(b'3'),
        0x05 => Some(b'4'),
        0x06 => Some(b'5'),
        0x07 => Some(b'6'),
        0x08 => Some(b'7'),
        0x09 => Some(b'8'),
        0x0a => Some(b'9'),
        0x0b => Some(b'0'),
        0x0e => Some(8),
        0x1c => Some(b'\n'),
        0x39 => Some(b' '),
        0x10 => Some(b'q'),
        0x11 => Some(b'w'),
        0x12 => Some(b'e'),
        0x13 => Some(b'r'),
        0x14 => Some(b't'),
        0x15 => Some(b'y'),
        0x16 => Some(b'u'),
        0x17 => Some(b'i'),
        0x18 => Some(b'o'),
        0x19 => Some(b'p'),
        0x1e => Some(b'a'),
        0x1f => Some(b's'),
        0x20 => Some(b'd'),
        0x21 => Some(b'f'),
        0x22 => Some(b'g'),
        0x23 => Some(b'h'),
        0x24 => Some(b'j'),
        0x25 => Some(b'k'),
        0x26 => Some(b'l'),
        0x2c => Some(b'z'),
        0x2d => Some(b'x'),
        0x2e => Some(b'c'),
        0x2f => Some(b'v'),
        0x30 => Some(b'b'),
        0x31 => Some(b'n'),
        0x32 => Some(b'm'),
        0x34 => Some(b'.'),
        _ => None,
    }
}
