use core::ptr::write_volatile;

#[derive(Clone, Copy)]
pub struct Color(pub u32);

pub struct Framebuffer {
    address: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    bytes_per_pixel: usize,
}

impl Framebuffer {
    pub const fn new(
        address: *mut u8,
        width: usize,
        height: usize,
        pitch: usize,
        bits_per_pixel: usize,
    ) -> Self {
        Self {
            address,
            width,
            height,
            pitch,
            bytes_per_pixel: bits_per_pixel / 8,
        }
    }

    pub fn clear(&mut self, color: Color) {
        self.rect(0, 0, self.width, self.height, color);
    }

    pub fn rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        let x_end = (x + width).min(self.width);
        let y_end = (y + height).min(self.height);

        for yy in y..y_end {
            for xx in x..x_end {
                self.pixel(xx, yy, color);
            }
        }
    }

    pub fn pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.width || y >= self.height || self.bytes_per_pixel < 3 {
            return;
        }

        let offset = y * self.pitch + x * self.bytes_per_pixel;
        unsafe {
            match self.bytes_per_pixel {
                3 => {
                    write_volatile(self.address.add(offset), (color.0 & 0xff) as u8);
                    write_volatile(self.address.add(offset + 1), ((color.0 >> 8) & 0xff) as u8);
                    write_volatile(self.address.add(offset + 2), ((color.0 >> 16) & 0xff) as u8);
                }
                _ => write_volatile(self.address.add(offset) as *mut u32, color.0),
            }
        }
    }

    pub fn address(&self) -> u64 {
        self.address as u64
    }

    pub fn byte_len(&self) -> u64 {
        (self.pitch * self.height) as u64
    }
}
