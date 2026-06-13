use core::cell::UnsafeCell;

const BUFFER_LEN: usize = 256;

struct KeyboardBuffer {
    bytes: [u8; BUFFER_LEN],
    read: usize,
    write: usize,
    shift: bool,
    ctrl: bool,
    extended: bool,
}

impl KeyboardBuffer {
    const fn new() -> Self {
        Self {
            bytes: [0; BUFFER_LEN],
            read: 0,
            write: 0,
            shift: false,
            ctrl: false,
            extended: false,
        }
    }

    fn pop(&mut self) -> Option<u8> {
        if self.read == self.write {
            return None;
        }

        let byte = self.bytes[self.read];
        self.read = (self.read + 1) % BUFFER_LEN;
        Some(byte)
    }

    fn has_key(&self) -> bool {
        self.read != self.write
    }

    fn push(&mut self, byte: u8) {
        let next = (self.write + 1) % BUFFER_LEN;
        if next == self.read {
            return;
        }
        self.bytes[self.write] = byte;
        self.write = next;
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.push(*byte);
        }
    }
}

struct GlobalKeyboard(UnsafeCell<KeyboardBuffer>);

unsafe impl Sync for GlobalKeyboard {}

static KEYBOARD: GlobalKeyboard = GlobalKeyboard(UnsafeCell::new(KeyboardBuffer::new()));

pub fn decode_scancode(scancode: u8) -> Option<u8> {
    unsafe {
        let keyboard = &mut *KEYBOARD.0.get();
        match scancode {
            0xe0 => {
                keyboard.extended = true;
                return None;
            }
            0x2a | 0x36 => {
                keyboard.shift = true;
                return None;
            }
            0xaa | 0xb6 => {
                keyboard.shift = false;
                return None;
            }
            0x1d => {
                keyboard.ctrl = true;
                return None;
            }
            0x9d => {
                keyboard.ctrl = false;
                return None;
            }
            code if code & 0x80 != 0 => {
                keyboard.extended = false;
                return None;
            }
            code => {
                if keyboard.extended {
                    keyboard.extended = false;
                    match code {
                        0x47 => keyboard.push_bytes(b"\x1b[H"),
                        0x48 => keyboard.push_bytes(b"\x1b[A"),
                        0x4b => keyboard.push_bytes(b"\x1b[D"),
                        0x4d => keyboard.push_bytes(b"\x1b[C"),
                        0x4f => keyboard.push_bytes(b"\x1b[F"),
                        0x50 => keyboard.push_bytes(b"\x1b[B"),
                        0x53 => keyboard.push(127),
                        _ => {}
                    }
                    return None;
                }
                let byte = decode(code, keyboard.shift)?;
                if keyboard.ctrl && byte.is_ascii_alphabetic() {
                    Some(byte.to_ascii_lowercase() & 0x1f)
                } else {
                    Some(byte)
                }
            }
        }
    }
}

pub fn pop_key() -> Option<u8> {
    unsafe { (*KEYBOARD.0.get()).pop() }
}

pub fn has_key() -> bool {
    unsafe { (*KEYBOARD.0.get()).has_key() }
}

pub fn push_key(byte: u8) {
    unsafe {
        (*KEYBOARD.0.get()).push(byte);
    }
}

fn decode(scancode: u8, shift: bool) -> Option<u8> {
    let byte = match scancode {
        0x02 => {
            if shift {
                b'!'
            } else {
                b'1'
            }
        }
        0x03 => {
            if shift {
                b'"'
            } else {
                b'2'
            }
        }
        0x04 => {
            if shift {
                0
            } else {
                b'3'
            }
        }
        0x05 => {
            if shift {
                b'$'
            } else {
                b'4'
            }
        }
        0x06 => {
            if shift {
                b'%'
            } else {
                b'5'
            }
        }
        0x07 => {
            if shift {
                b'&'
            } else {
                b'6'
            }
        }
        0x08 => {
            if shift {
                b'/'
            } else {
                b'7'
            }
        }
        0x09 => {
            if shift {
                b'('
            } else {
                b'8'
            }
        }
        0x0a => {
            if shift {
                b')'
            } else {
                b'9'
            }
        }
        0x0b => {
            if shift {
                b'='
            } else {
                b'0'
            }
        }
        0x0c => {
            if shift {
                b'?'
            } else {
                b'-'
            }
        }
        0x0d => {
            if shift {
                b'`'
            } else {
                b'\''
            }
        }
        0x0e => 8,
        0x1c => b'\n',
        0x39 => b' ',
        0x10 => letter(b'q', shift),
        0x11 => letter(b'w', shift),
        0x12 => letter(b'e', shift),
        0x13 => letter(b'r', shift),
        0x14 => letter(b't', shift),
        0x15 => letter(b'z', shift),
        0x16 => letter(b'u', shift),
        0x17 => letter(b'i', shift),
        0x18 => letter(b'o', shift),
        0x19 => letter(b'p', shift),
        0x1a => 0,
        0x1b => {
            if shift {
                b'*'
            } else {
                b'+'
            }
        }
        0x1e => letter(b'a', shift),
        0x1f => letter(b's', shift),
        0x20 => letter(b'd', shift),
        0x21 => letter(b'f', shift),
        0x22 => letter(b'g', shift),
        0x23 => letter(b'h', shift),
        0x24 => letter(b'j', shift),
        0x25 => letter(b'k', shift),
        0x26 => letter(b'l', shift),
        0x27 => 0,
        0x28 => 0,
        0x29 => {
            if shift {
                b'>'
            } else {
                b'^'
            }
        }
        0x2b => {
            if shift {
                b'\''
            } else {
                b'#'
            }
        }
        0x2c => letter(b'y', shift),
        0x2d => letter(b'x', shift),
        0x2e => letter(b'c', shift),
        0x2f => letter(b'v', shift),
        0x30 => letter(b'b', shift),
        0x31 => letter(b'n', shift),
        0x32 => letter(b'm', shift),
        0x33 => {
            if shift {
                b';'
            } else {
                b','
            }
        }
        0x34 => {
            if shift {
                b':'
            } else {
                b'.'
            }
        }
        0x35 => {
            if shift {
                b'?'
            } else {
                b'/'
            }
        }
        _ => return None,
    };
    if byte == 0 {
        None
    } else {
        Some(byte)
    }
}

const fn letter(byte: u8, shift: bool) -> u8 {
    if shift {
        byte - 32
    } else {
        byte
    }
}
