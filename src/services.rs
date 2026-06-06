pub mod gui {
    use core::cell::UnsafeCell;

    use crate::{
        font,
        framebuffer::{Color, Framebuffer},
        serial,
    };

    struct GlobalFramebuffer(UnsafeCell<Option<Framebuffer>>);

    unsafe impl Sync for GlobalFramebuffer {}

    static FRAMEBUFFER: GlobalFramebuffer = GlobalFramebuffer(UnsafeCell::new(None));
    static mut CONSOLE_READY: bool = false;
    static mut CONSOLE_X: usize = 184;
    static mut CONSOLE_Y: usize = 520;

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
            cursor += font::ADVANCE;
        }
    }

    pub fn console_write(bytes: &[u8]) {
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
                    CONSOLE_Y += font::HEIGHT + 6;
                    continue;
                }
                draw_char(CONSOLE_X, CONSOLE_Y, *byte, 0x00101820);
                CONSOLE_X += font::ADVANCE;
                if CONSOLE_X > 820 {
                    CONSOLE_X = 184;
                    CONSOLE_Y += font::HEIGHT + 6;
                }
            }
        }
    }

    pub fn reset_console() {
        unsafe {
            CONSOLE_READY = false;
            CONSOLE_X = 184;
            CONSOLE_Y = 520;
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
        let glyph = font::glyph(byte);
        with_framebuffer(|fb| {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..font::WIDTH {
                    if bits & (1 << (font::WIDTH - 1 - col)) != 0 {
                        fb.pixel(x + col, y + row, Color(color));
                    }
                }
            }
        });
    }
}
