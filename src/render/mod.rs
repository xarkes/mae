#[cfg(feature = "opengl")]
mod opengl;

use crate::os::Window;
use opengl::GLContext;

pub struct Renderer {
    pub win: Window,
    pub ctx: Box<GLContext>,
}

impl Renderer {
    pub fn new(win: Window) -> Self {
        let mut available_renderers = Vec::new();
        if cfg!(feature = "opengl") {
            available_renderers.push("opengl");
        }

        if available_renderers.is_empty() {
            panic!("No renderer available!");
        }

        let renderer = available_renderers[0];
        let ctx = match renderer {
            "opengl" => Box::new(GLContext::new(&win)),
            _ => {
                panic!("Renderer not implemented!");
            }
        };
        Renderer { win, ctx }
    }

    pub fn update(&self) {
        self.ctx.update();
    }

    pub fn render_text(&self) {
        // TODO!
        // Read the font data.
        let font = include_bytes!("/System/Library/Fonts/SFNSMono.ttf") as &[u8];
        // Parse it into the font type.
        let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default()).unwrap();
        // Rasterize and get the layout metrics for the letter 'g' at 17px.
        let (metrics, bitmap) = font.rasterize('g', 17.0);
        println!("Bitmap length... {}", bitmap.len());
        println!("Metrics: {}x{}", metrics.width, metrics.height);
        for (i, v) in bitmap.iter().enumerate() {
            if i % metrics.width == 0 {
                println!();
            }
            // print!("{v:02x}");
            let c = match v {
                0..100 => '.',
                100..200 => '*',
                200..=255 => '#',
            };
            print!("{c}");
        }
    }
}
