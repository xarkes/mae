mod font_cache;
#[cfg(feature = "opengl")]
mod opengl;

use crate::os::Window;
use font_cache::FontCache;

pub struct Renderer {
    pub win: Window,
    pub ctx: Box<opengl::GLContext>,
    font_cache: FontCache,
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
        let mut ctx = match renderer {
            "opengl" => Box::new(opengl::GLContext::new(&win)),
            _ => {
                panic!("Renderer not implemented!");
            }
        };
        let font_cache = FontCache::new();
        ctx.update_font_texture(font_cache.atlas());
        Renderer {
            win,
            ctx,
            font_cache,
        }
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        self.ctx.resize(w, h);
    }

    pub fn update(&mut self) {
        self.ctx.update(&mut self.font_cache);
    }
}
