use crate::framebuffer::{Color, Framebuffer};

pub struct Desktop;

impl Desktop {
    pub const fn new() -> Self {
        Self
    }

    pub fn draw(&self, fb: &mut Framebuffer) {
        let bg = Color(0x00191d24);
        let panel = Color(0x00282f3a);
        let accent = Color(0x0000b894);
        let window = Color(0x00e8edf2);
        let shadow = Color(0x000d1117);
        let title = Color(0x003b4252);

        fb.clear(bg);
        fb.rect(0, 0, 1280, 36, panel);
        fb.rect(18, 10, 16, 16, accent);
        fb.rect(46, 14, 160, 8, Color(0x00aab2bd));

        fb.rect(120, 105, 460, 280, shadow);
        fb.rect(112, 96, 460, 280, window);
        fb.rect(112, 96, 460, 34, title);
        fb.rect(128, 108, 10, 10, Color(0x00ff605c));
        fb.rect(146, 108, 10, 10, Color(0x00ffbd44));
        fb.rect(164, 108, 10, 10, Color(0x0000ca4e));
        fb.rect(142, 164, 300, 10, Color(0x00606a78));
        fb.rect(142, 190, 238, 10, Color(0x0088909c));
        fb.rect(142, 232, 360, 76, Color(0x00d0d7de));

        fb.rect(650, 120, 280, 170, shadow);
        fb.rect(642, 112, 280, 170, Color(0x00343d4a));
        fb.rect(666, 140, 108, 12, accent);
        fb.rect(666, 170, 198, 8, Color(0x00b8c0cc));
        fb.rect(666, 194, 166, 8, Color(0x0088909c));
    }
}
