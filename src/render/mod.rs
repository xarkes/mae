mod font_cache;
#[cfg(feature = "opengl")]
mod opengl;

use crate::os::Window;
use font_cache::FontCache;

pub(crate) struct RenderBatch {
    data: Vec<f32>,
    bytes_count: isize,
}

impl RenderBatch {
    const ELEM_SIZE: usize = std::mem::size_of::<f32>() * 24;
    pub fn new(prealloc: usize) -> Self {
        RenderBatch {
            data: Vec::with_capacity(prealloc * RenderBatch::ELEM_SIZE),
            bytes_count: 0,
        }
    }

    pub fn add_data(&mut self, data: [f32; 24]) {
        self.data.extend(data);
        self.bytes_count += RenderBatch::ELEM_SIZE as isize;
    }
}

pub struct Renderer {
    pub win: Window,
    pub ctx: Box<opengl::GLContext>,
    pub font_cache: FontCache,
    batches: Vec<RenderBatch>,
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
            batches: Vec::new(),
        }
    }

    pub fn update_font_texture(&mut self) {
        let atlas = self.font_cache.atlas();
        self.ctx.update_font_texture(atlas);
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        self.ctx.resize(w, h);
    }

    pub fn update(&mut self) {
        self.ctx.begin_frame();
        self.ctx.render(&self.batches);
        self.ctx.end_frame();
        self.batches.clear();
    }

    pub fn add_batch(&mut self, batch: RenderBatch) {
        self.batches.push(batch);
    }
}
