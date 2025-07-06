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
#[derive(Clone, Copy, Debug)]
pub struct RectCoords {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}
impl RectCoords {
    pub fn from_size(x: f32, y: f32, w: f32, h: f32) -> Self {
        RectCoords {
            x0: x,
            y0: y,
            x1: x + w,
            y1: y + h,
        }
    }
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct V4f32 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
#[derive(Debug)]
pub struct Extra {
    pub omit_texture: f32,
    pub _unused: [f32; 3],
}
impl Extra {
    pub fn new(omit_texture: bool) -> Self {
        let omit = match omit_texture {
            true => 1.0,
            false => 0.0,
        };
        Extra {
            omit_texture: omit,
            _unused: [0.0, 0.0, 0.0],
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct Rect2DInst {
    /// Coordinates in screen space (pixels) of top left and bottom right corners of the rect
    pub dst: RectCoords,
    /// Coordinates in texture space, top left, bottom right
    pub src: RectCoords,
    /// Color for each corner of the rectangle
    pub colors: [V4f32; 4],
    pub extra: Extra,
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
    pub font_cache: Box<FontCache>,
    batches: Vec<RenderBatch>,
}

impl Renderer {
    pub fn new(win: Window) -> Self {
        // TODO(xarkes): Can we have this at compile time rather than runtime?
        // debug_assert!(std::mem::size_of::<Rect2DInst>() == 4 * 4 * 3);
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
        let font_cache = Box::new(FontCache::new());
        ctx.update_font_texture(font_cache.atlas());
        // XXX(xarkes): This sucks, make it better
        let mut batches = Vec::new();
        batches.push(RenderBatch::new(100));
        batches.push(RenderBatch::new(100));
        Renderer {
            win,
            ctx,
            font_cache,
            batches,
        }
    }

    pub fn update_font_texture(&mut self) {
        let atlas = self.font_cache.atlas();
        self.ctx.update_font_texture(atlas);
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        self.ctx.resize(w, h);
    }

    pub fn render_frame(&mut self) {
        self.ctx.begin_frame();
        self.ctx.render(&self.batches);
        self.ctx.end_frame();

        self.batches.clear();
        self.batches.push(RenderBatch::new(100));
        self.batches.push(RenderBatch::new(100));
    }

    // XXX(xarkes): rework this
    pub fn current_batch(&mut self) -> &mut RenderBatch {
        &mut self.batches[0]
    }
    pub fn debug_batch(&mut self) -> &mut RenderBatch {
        &mut self.batches[1]
    }
    pub fn add_batch(&mut self, batch: RenderBatch) {
        self.batches.push(batch);
    }
}
