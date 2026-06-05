pub mod gui {
    use core::cell::UnsafeCell;

    use crate::{
        framebuffer::{Color, Framebuffer},
        serial,
    };

    struct GlobalFramebuffer(UnsafeCell<Option<Framebuffer>>);

    unsafe impl Sync for GlobalFramebuffer {}

    static FRAMEBUFFER: GlobalFramebuffer = GlobalFramebuffer(UnsafeCell::new(None));

    pub fn install(framebuffer: Framebuffer) {
        unsafe {
            *FRAMEBUFFER.0.get() = Some(framebuffer);
        }
        serial::write_line("nk: framebuffer service ready");
    }

    pub fn clear(color: u32) {
        with_framebuffer(|fb| fb.clear(Color(color)));
    }

    pub fn rect(x: usize, y: usize, width: usize, height: usize, color: u32) {
        with_framebuffer(|fb| fb.rect(x, y, width, height, Color(color)));
    }

    pub fn text(x: usize, y: usize, bytes: *const u8, len: usize, color: u32) {
        if bytes.is_null() || len > 256 {
            return;
        }

        let text = unsafe { core::slice::from_raw_parts(bytes, len) };
        let mut cursor = x;
        for byte in text {
            draw_char(cursor, y, *byte, color);
            cursor += 18;
        }
    }

    pub fn console_write(bytes: &[u8]) {
        static mut CONSOLE_READY: bool = false;
        static mut CONSOLE_X: usize = 184;
        static mut CONSOLE_Y: usize = 520;

        unsafe {
            if !CONSOLE_READY {
                rect(150, 468, 720, 210, 0x00f3f5f7);
                rect(150, 468, 720, 40, 0x00343d4a);
                rect(166, 481, 10, 10, 0x00ff605c);
                rect(184, 481, 10, 10, 0x00ffbd44);
                rect(202, 481, 10, 10, 0x0000ca4e);
                let title = b"cat output";
                text(234, 478, title.as_ptr(), title.len(), 0x00f3f5f7);
                CONSOLE_READY = true;
            }

            for byte in bytes {
                if *byte == b'\n' {
                    CONSOLE_X = 184;
                    CONSOLE_Y += 28;
                    continue;
                }
                draw_char(CONSOLE_X, CONSOLE_Y, *byte, 0x00101820);
                CONSOLE_X += 18;
                if CONSOLE_X > 820 {
                    CONSOLE_X = 184;
                    CONSOLE_Y += 28;
                }
            }
        }
    }

    fn with_framebuffer(run: impl FnOnce(&mut Framebuffer)) {
        unsafe {
            if let Some(framebuffer) = (*FRAMEBUFFER.0.get()).as_mut() {
                run(framebuffer);
            }
        }
    }

    fn draw_char(x: usize, y: usize, byte: u8, color: u32) {
        const SCALE: usize = 3;
        let glyph = glyph(byte);
        with_framebuffer(|fb| {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        fb.rect(x + col * SCALE, y + row * SCALE, SCALE, SCALE, Color(color));
                    }
                }
            }
        });
    }

    fn glyph(byte: u8) -> [u8; 7] {
        match byte {
            b'!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
            b':' => [0, 0b00100, 0b00100, 0, 0b00100, 0b00100, 0],
            b'-' => [0, 0, 0, 0b11111, 0, 0, 0],
            b'.' => [0, 0, 0, 0, 0, 0, 0b00100],
            b'>' => [0b10000, 0b01000, 0b00100, 0b00010, 0b00100, 0b01000, 0b10000],
            b' ' => [0, 0, 0, 0, 0, 0, 0],
            b'0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
            b'1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
            b'2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
            b'3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
            b'4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
            b'5' => [0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110],
            b'6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
            b'7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
            b'8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
            b'9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
            b'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
            b'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
            b'a' => [0, 0b01110, 0b00001, 0b01111, 0b10001, 0b10011, 0b01101],
            b'b' => [0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b11001, 0b10110],
            b'c' => [0, 0b01110, 0b10001, 0b10000, 0b10000, 0b10001, 0b01110],
            b'd' => [0b00001, 0b00001, 0b01101, 0b10011, 0b10001, 0b10011, 0b01101],
            b'e' => [0, 0b01110, 0b10001, 0b11111, 0b10000, 0b10001, 0b01110],
            b'f' => [0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000],
            b'g' => [0, 0b01101, 0b10011, 0b10001, 0b01111, 0b00001, 0b01110],
            b'h' => [0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001],
            b'i' => [0b00100, 0, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110],
            b'k' => [0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
            b'l' => [0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
            b'm' => [0, 0b11010, 0b10101, 0b10101, 0b10101, 0b10101, 0b10101],
            b'n' => [0, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001, 0b10001],
            b'o' => [0, 0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
            b'p' => [0, 0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000],
            b'r' => [0, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000, 0b10000],
            b's' => [0, 0b01111, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110],
            b't' => [0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00101, 0b00010],
            b'u' => [0, 0b10001, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101],
            b'v' => [0, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
            b'w' => [0, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
            b'x' => [0, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0],
            b'y' => [0, 0b10001, 0b10001, 0b10011, 0b01101, 0b00001, 0b01110],
            _ => [0b11111, 0b10001, 0b10101, 0b10101, 0b10101, 0b10001, 0b11111],
        }
    }
}
