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
    const CONSOLE_LEN: usize = 2048;
    static mut CONSOLE_BYTES: [u8; CONSOLE_LEN] = [0; CONSOLE_LEN];
    static mut CONSOLE_WRITE: usize = 0;
    static mut CONSOLE_SEQ: u64 = 0;

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
            for byte in bytes {
                CONSOLE_BYTES[CONSOLE_WRITE % CONSOLE_LEN] = *byte;
                CONSOLE_WRITE = CONSOLE_WRITE.wrapping_add(1);
                CONSOLE_SEQ = CONSOLE_SEQ.wrapping_add(1);
            }
        }
    }

    pub fn reset_console() {
        unsafe {
            CONSOLE_BYTES = [0; CONSOLE_LEN];
            CONSOLE_WRITE = 0;
            CONSOLE_SEQ = CONSOLE_SEQ.wrapping_add(1);
        }
    }

    pub fn console_seq() -> u64 {
        unsafe { CONSOLE_SEQ }
    }

    pub fn console_len() -> usize {
        unsafe { CONSOLE_WRITE.min(CONSOLE_LEN) }
    }

    pub fn console_byte(index: usize) -> u8 {
        unsafe {
            let len = CONSOLE_WRITE.min(CONSOLE_LEN);
            if index >= len {
                return 0;
            }
            let start = CONSOLE_WRITE.saturating_sub(len);
            CONSOLE_BYTES[(start + index) % CONSOLE_LEN]
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
