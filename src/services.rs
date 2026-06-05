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
            cursor += 8;
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
        let glyph = glyph(byte);
        with_framebuffer(|fb| {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        fb.rect(x + col * 2, y + row * 2, 2, 2, Color(color));
                    }
                }
            }
        });
    }

    fn glyph(byte: u8) -> [u8; 7] {
        match byte {
            b'!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
            b' ' => [0, 0, 0, 0, 0, 0, 0],
            b'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
            b'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
            b'a' => [0, 0b01110, 0b00001, 0b01111, 0b10001, 0b10011, 0b01101],
            b'e' => [0, 0b01110, 0b10001, 0b11111, 0b10000, 0b10001, 0b01110],
            b'l' => [0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
            b'o' => [0, 0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
            b'r' => [0, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000, 0b10000],
            b't' => [0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00101, 0b00010],
            _ => [0b11111, 0b10001, 0b10101, 0b10101, 0b10101, 0b10001, 0b11111],
        }
    }
}
