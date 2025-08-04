pub mod font_cache;
#[cfg(feature = "opengl")]
mod opengl;

use std::{cell::RefCell, rc::Rc};

use crate::os::Window;
use font_cache::FontCache;

pub struct RenderBatch {
    data: Vec<Rect2DInst>,
    bytes_count: isize,
    texture: Option<u32>,
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
    pub fn x(&self, xval: f32) -> RectCoords {
        RectCoords {
            x0: self.x0 + xval,
            y0: self.y0,
            x1: self.x1 + xval,
            y1: self.y1,
        }
    }
    pub fn y(&self, yval: f32) -> RectCoords {
        RectCoords {
            x0: self.x0,
            y0: self.y0 + yval,
            x1: self.x1,
            y1: self.y1 + yval,
        }
    }
    pub fn mul(&self, coef: f32) -> RectCoords {
        RectCoords {
            x0: self.x0 * coef,
            y0: self.y0 * coef,
            x1: self.x1 * coef,
            y1: self.y1 * coef,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
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
            texture: None,
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
    pub font_cache: Rc<RefCell<FontCache>>,
    pub icon_font_cache: Rc<RefCell<FontCache>>,
    batches: Vec<RenderBatch>,
}

use log;
impl Renderer {
    pub fn new(win: Window) -> Self {
        // TODO(xarkes): Can we have this at compile time rather than runtime?
        // debug_assert!(std::mem::size_of::<Rect2DInst>() == 4 * 4 * 3);
        log::debug!("render new");
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
        let font_cache = Rc::new(RefCell::new(FontCache::new(include_bytes!(
            "../../assets/NotoSans-Regular.ttf"
        ))));
        let icon_font_cache = Rc::new(RefCell::new(FontCache::new(include_bytes!(
            "../../assets/MaterialIcons-Regular.ttf"
        ))));
        let mut batches = Vec::new();
        batches.push(RenderBatch::new(100));
        let mut renderer = Renderer {
            win,
            ctx,
            font_cache,
            icon_font_cache,
            batches,
        };
        renderer.update_font_texture(false);
        renderer.update_font_texture(true);
        renderer
    }

    pub fn update_font_texture(&mut self, font_icon: bool) {
        let mut fc = match font_icon {
            true => self.icon_font_cache.borrow_mut(),
            false => self.font_cache.borrow_mut(),
        };
        if fc.dirty {
            let texture_id = self.ctx.update_font_texture(fc.atlas());
            fc.texture_id = texture_id;
            fc.dirty = false;
            println!(
                "Font cache dirty, generating new texture: texture_id: {} (font_icon: {})",
                texture_id, font_icon
            );
        }
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
    }

    pub fn current_batch(&mut self) -> &mut RenderBatch {
        let id = self.batches.len() - 1;
        &mut self.batches[id]
    }

    pub fn add_rect(&mut self, inst: Rect2DInst, texture: Option<u32>) {
        if texture != self.current_batch().texture {
            self.batches.push(RenderBatch::new(100));
            self.current_batch().texture = texture;
        }
        self.current_batch().add_rect(inst);
    }

    #[cfg(debug_assertions)]
    pub fn vsync(&mut self, enable: bool) {
        self.ctx.vsync(enable);
    }
}
