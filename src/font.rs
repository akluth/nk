use core::ptr;

const PSF2_MAGIC: u32 = 0x864a_b572;
const MAX_FONT_BYTES: usize = 64 * 1024;

static mut FONT_BYTES: [u8; MAX_FONT_BYTES] = [0; MAX_FONT_BYTES];
static mut FONT_LEN: usize = 0;
static mut FONT: Font = Font::empty();

#[derive(Clone, Copy)]
struct Font {
    glyphs: usize,
    glyph_size: usize,
    height: usize,
    width: usize,
    bytes_per_row: usize,
    glyph_offset: usize,
    first: u8,
    loaded: bool,
}

impl Font {
    const fn empty() -> Self {
        Self {
            glyphs: 0,
            glyph_size: 0,
            height: 16,
            width: 8,
            bytes_per_row: 1,
            glyph_offset: 0,
            first: 0,
            loaded: false,
        }
    }
}

pub fn load_psf(bytes: &[u8]) -> bool {
    let Some(font) = parse_psf2(bytes) else {
        return false;
    };
    if bytes.len() > MAX_FONT_BYTES {
        return false;
    }

    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), ptr::addr_of_mut!(FONT_BYTES).cast(), bytes.len());
        FONT_LEN = bytes.len();
        FONT = font;
    }
    true
}

pub fn is_loaded() -> bool {
    unsafe { FONT.loaded && FONT_LEN != 0 }
}

pub fn width() -> usize {
    unsafe { FONT.width }
}

pub fn height() -> usize {
    unsafe { FONT.height }
}

pub fn advance() -> usize {
    width().saturating_add(1)
}

pub fn line_height() -> usize {
    height().saturating_add(2)
}

pub fn glyph_bit(byte: u8, row: usize, col: usize) -> bool {
    unsafe {
        if !is_loaded() || row >= FONT.height || col >= FONT.width {
            return false;
        }
        let Some(index) = glyph_index(byte) else {
            return false;
        };
        let offset = FONT
            .glyph_offset
            .saturating_add(index.saturating_mul(FONT.glyph_size))
            .saturating_add(row.saturating_mul(FONT.bytes_per_row))
            .saturating_add(col / 8);
        if offset >= FONT_LEN {
            return false;
        }
        let byte = FONT_BYTES[offset];
        let mask = 0x80 >> (col % 8);
        byte & mask != 0
    }
}

unsafe fn glyph_index(byte: u8) -> Option<usize> {
    if FONT.glyphs == 95 && byte >= FONT.first && byte < FONT.first.saturating_add(95) {
        Some((byte - FONT.first) as usize)
    } else if (byte as usize) < FONT.glyphs {
        Some(byte as usize)
    } else if byte == b'\t' && (b' ' as usize) < FONT.glyphs {
        Some(b' ' as usize)
    } else {
        None
    }
}

fn parse_psf2(bytes: &[u8]) -> Option<Font> {
    if bytes.len() < 32 {
        return None;
    }
    let magic = read_u32(bytes, 0)?;
    if magic != PSF2_MAGIC {
        return None;
    }
    let header_size = read_u32(bytes, 8)? as usize;
    let glyphs = read_u32(bytes, 16)? as usize;
    let glyph_size = read_u32(bytes, 20)? as usize;
    let height = read_u32(bytes, 24)? as usize;
    let width = read_u32(bytes, 28)? as usize;
    if header_size < 32
        || glyphs == 0
        || glyphs > 256
        || glyph_size == 0
        || width == 0
        || width > 32
        || height == 0
        || height > 32
    {
        return None;
    }
    let bytes_per_row = (width + 7) / 8;
    if glyph_size < bytes_per_row.saturating_mul(height) {
        return None;
    }
    let glyph_bytes = glyphs.checked_mul(glyph_size)?;
    if header_size.checked_add(glyph_bytes)? > bytes.len() {
        return None;
    }

    Some(Font {
        glyphs,
        glyph_size,
        height,
        width,
        bytes_per_row,
        glyph_offset: header_size,
        first: if glyphs == 95 { 32 } else { 0 },
        loaded: true,
    })
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let data = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}
