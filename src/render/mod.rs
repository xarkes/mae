mod font_cache;
#[cfg(feature = "opengl")]
mod opengl;

use crate::os::Window;
use font_cache::FontCache;

pub(crate) struct RenderBatch {
    data: Vec<Rect2DInst>,
    bytes_count: isize,
}

#[repr(C)]
pub struct Rect2DInst {
    pub x: f32,
    pub y: f32,
    pub tex_x: f32,
    pub tex_y: f32,
    pub x2: f32,
    pub y2: f32,
    pub tex_x2: f32,
    pub tex_y2: f32,
    pub x3: f32,
    pub y3: f32,
    pub tex_x3: f32,
    pub tex_y3: f32,
    pub x4: f32,
    pub y4: f32,
    pub tex_x4: f32,
    pub tex_y4: f32,
    pub x5: f32,
    pub y5: f32,
    pub tex_x5: f32,
    pub tex_y5: f32,
    pub x6: f32,
    pub y6: f32,
    pub tex_x6: f32,
    pub tex_y6: f32,
}

impl RenderBatch {
    pub fn new(prealloc: usize) -> Self {
        RenderBatch {
            data: Vec::with_capacity(prealloc),
            bytes_count: 0,
        }
    }

    pub fn add_rect(&mut self, inst: Rect2DInst) {
        self.data.push(inst);
        self.bytes_count += std::mem::size_of::<Rect2DInst>() as isize;
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
        // TODO(xarkes): Can we have this at compile time rather than runtime?
        debug_assert!(std::mem::size_of::<Rect2DInst>() == 6 * 4 * 4);
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
