pub mod gui {
    use crate::{desktop::Desktop, framebuffer::Framebuffer, serial};

    pub struct GuiService {
        desktop: Desktop,
    }

    impl GuiService {
        pub const fn new() -> Self {
            Self {
                desktop: Desktop::new(),
            }
        }

        pub fn run(&self, framebuffer: &mut Framebuffer) {
            self.desktop.draw(framebuffer);
            serial::write_line("nk: gui service painted desktop");
        }
    }
}
