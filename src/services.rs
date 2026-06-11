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
    const TERM_MAX_COLS: usize = 160;
    const TERM_MAX_ROWS: usize = 64;
    const TERM_BG: u32 = 0x00000000;
    const TERM_FG: u32 = 0x00d8d8d8;
    const TERM_LINE_H: usize = font::HEIGHT + 2;
    static mut TERM_BYTES: [[u8; TERM_MAX_COLS]; TERM_MAX_ROWS] =
        [[0; TERM_MAX_COLS]; TERM_MAX_ROWS];
    static mut TERM_LENS: [usize; TERM_MAX_ROWS] = [0; TERM_MAX_ROWS];
    static mut TEXT_COL: usize = 0;
    static mut TEXT_ROW: usize = 0;
    static mut ANSI_STATE: u8 = 0;
    static mut CSI_VALUE: usize = 0;
    static mut CSI_HAS_VALUE: bool = false;

    pub fn install(framebuffer: Framebuffer) {
        unsafe {
            *FRAMEBUFFER.0.get() = Some(framebuffer);
        }
        reset_terminal_screen();
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
                draw_terminal_byte(*byte);
            }
        }
    }

    pub fn reset_console() {
        unsafe {
            CONSOLE_BYTES = [0; CONSOLE_LEN];
            CONSOLE_WRITE = 0;
            CONSOLE_SEQ = CONSOLE_SEQ.wrapping_add(1);
            reset_terminal_state();
        }
        reset_terminal_screen();
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

    unsafe fn draw_terminal_byte(byte: u8) {
        let Some((cols, rows)) = terminal_grid() else {
            return;
        };

        if handle_ansi_byte(byte, cols) {
            return;
        }

        hide_cursor();
        match byte {
            b'\r' => {
                TEXT_COL = 0;
            }
            b'\n' => {
                TEXT_COL = 0;
                TEXT_ROW += 1;
                if TEXT_ROW >= rows {
                    scroll_terminal(rows, cols);
                    TEXT_ROW = rows.saturating_sub(1);
                }
            }
            8 | 127 => {
                if TEXT_COL > 0 {
                    TEXT_COL -= 1;
                    TERM_BYTES[TEXT_ROW][TEXT_COL] = 0;
                    TERM_LENS[TEXT_ROW] = TEXT_COL;
                    draw_terminal_cell(TEXT_COL, TEXT_ROW, b' ');
                }
            }
            byte if byte >= 0x20 => {
                if TEXT_COL >= cols {
                    TEXT_COL = 0;
                    TEXT_ROW += 1;
                }
                if TEXT_ROW >= rows {
                    scroll_terminal(rows, cols);
                    TEXT_ROW = rows.saturating_sub(1);
                }
                TERM_BYTES[TEXT_ROW][TEXT_COL] = byte;
                TERM_LENS[TEXT_ROW] = TERM_LENS[TEXT_ROW].max(TEXT_COL + 1);
                draw_terminal_cell(TEXT_COL, TEXT_ROW, byte);
                TEXT_COL += 1;
            }
            _ => {}
        }
        show_cursor();
    }

    unsafe fn terminal_grid() -> Option<(usize, usize)> {
        let framebuffer = (*FRAMEBUFFER.0.get()).as_ref()?;
        let cols = (framebuffer.width() / font::ADVANCE)
            .clamp(1, TERM_MAX_COLS);
        let rows = (framebuffer.height() / TERM_LINE_H).clamp(1, TERM_MAX_ROWS);
        Some((cols, rows))
    }

    unsafe fn reset_terminal_state() {
        TERM_BYTES = [[0; TERM_MAX_COLS]; TERM_MAX_ROWS];
        TERM_LENS = [0; TERM_MAX_ROWS];
        TEXT_COL = 0;
        TEXT_ROW = 0;
        ANSI_STATE = 0;
        CSI_VALUE = 0;
        CSI_HAS_VALUE = false;
    }

    fn reset_terminal_screen() {
        unsafe {
            reset_terminal_state();
        }
        clear(TERM_BG);
    }

    unsafe fn scroll_terminal(rows: usize, cols: usize) {
        for row in 1..rows {
            TERM_BYTES[row - 1] = TERM_BYTES[row];
            TERM_LENS[row - 1] = TERM_LENS[row].min(cols);
        }
        TERM_BYTES[rows - 1] = [0; TERM_MAX_COLS];
        TERM_LENS[rows - 1] = 0;
        redraw_terminal(rows, cols);
    }

    unsafe fn handle_ansi_byte(byte: u8, cols: usize) -> bool {
        match ANSI_STATE {
            0 => {
                if byte == 0x1b {
                    ANSI_STATE = 1;
                    return true;
                }
                false
            }
            1 => {
                if byte == b'[' {
                    ANSI_STATE = 2;
                    CSI_VALUE = 0;
                    CSI_HAS_VALUE = false;
                } else {
                    ANSI_STATE = 0;
                }
                true
            }
            _ => {
                if byte.is_ascii_digit() {
                    CSI_VALUE = CSI_VALUE
                        .saturating_mul(10)
                        .saturating_add((byte - b'0') as usize);
                    CSI_HAS_VALUE = true;
                    return true;
                }
                if byte == b'?' || byte == b';' {
                    return true;
                }

                hide_cursor();
                let value = if CSI_HAS_VALUE { CSI_VALUE } else { 1 };
                match byte {
                    b'G' => {
                        TEXT_COL = value.saturating_sub(1).min(cols.saturating_sub(1));
                    }
                    b'C' => {
                        TEXT_COL = TEXT_COL.saturating_add(value).min(cols.saturating_sub(1));
                    }
                    b'D' => {
                        TEXT_COL = TEXT_COL.saturating_sub(value);
                    }
                    b'H' | b'f' => {
                        TEXT_COL = 0;
                        TEXT_ROW = 0;
                    }
                    b'J' => {
                        reset_terminal_screen();
                    }
                    b'K' => {
                        erase_to_end_of_line(cols);
                    }
                    b'h' | b'l' | b'm' => {}
                    _ => {}
                }
                ANSI_STATE = 0;
                CSI_VALUE = 0;
                CSI_HAS_VALUE = false;
                show_cursor();
                true
            }
        }
    }

    unsafe fn erase_to_end_of_line(cols: usize) {
        for col in TEXT_COL..cols {
            TERM_BYTES[TEXT_ROW][col] = 0;
            draw_terminal_cell(col, TEXT_ROW, b' ');
        }
        TERM_LENS[TEXT_ROW] = TERM_LENS[TEXT_ROW].min(TEXT_COL);
    }

    unsafe fn redraw_terminal(rows: usize, cols: usize) {
        clear(TERM_BG);
        for row in 0..rows {
            draw_terminal_row(row, cols);
        }
    }

    unsafe fn draw_terminal_row(row: usize, cols: usize) {
        let len = TERM_LENS[row].min(cols);
        for col in 0..len {
            let byte = TERM_BYTES[row][col];
            if byte != 0 {
                draw_terminal_cell(col, row, byte);
            }
        }
    }

    unsafe fn draw_terminal_cell(col: usize, row: usize, byte: u8) {
        let x = col * font::ADVANCE;
        let y = row * TERM_LINE_H;
        rect(x, y, font::ADVANCE, TERM_LINE_H, TERM_BG);
        if byte != b' ' {
            draw_char(x, y, byte, TERM_FG);
        }
    }

    unsafe fn hide_cursor() {
        if TEXT_ROW >= TERM_MAX_ROWS || TEXT_COL >= TERM_MAX_COLS {
            return;
        }
        draw_terminal_cell(TEXT_COL, TEXT_ROW, TERM_BYTES[TEXT_ROW][TEXT_COL]);
    }

    unsafe fn show_cursor() {
        let Some((cols, rows)) = terminal_grid() else {
            return;
        };
        if TEXT_COL >= cols || TEXT_ROW >= rows {
            return;
        }
        let x = TEXT_COL * font::ADVANCE;
        let y = TEXT_ROW * TERM_LINE_H + font::HEIGHT;
        rect(x, y, font::WIDTH, 2, TERM_FG);
    }
}
