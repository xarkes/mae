#[cfg(feature = "cpu")]
mod cpu;
pub mod font_cache;
#[cfg(feature = "opengl")]
mod opengl;

use std::{cell::RefCell, rc::Rc};

use crate::os::Window;
use font_cache::FontCache;

pub trait RenderBackend {
    fn update_font_texture(&mut self, atlas: &font_cache::Atlas) -> u32;
    fn resize(&mut self, w: f32, h: f32);
    fn begin_frame(&mut self);
    fn end_frame(&mut self);
    fn render(&mut self, batches: &Vec<RenderBatch>);
    fn vsync(&mut self, enable: bool);
}

pub struct RenderBatch {
    pub(crate) data: Vec<Rect2DInst>,
    pub(crate) bytes_count: isize,
    pub(crate) texture: Option<u32>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    #[cfg(feature = "opengl")]
    OpenGL,
    #[cfg(feature = "cpu")]
    CPU,
}

impl Backend {
    /// Returns the default backend (prefers OpenGL if available)
    pub fn default_backend() -> Self {
        #[cfg(feature = "opengl")]
        return Backend::OpenGL;
        #[cfg(all(feature = "cpu", not(feature = "opengl")))]
        return Backend::CPU;
    }

    /// Returns backend from MAE_RENDERER environment variable, or default
    pub fn from_env() -> Self {
        match std::env::var("MAE_RENDERER").as_deref() {
            #[cfg(feature = "opengl")]
            Ok("opengl") => Backend::OpenGL,
            #[cfg(feature = "cpu")]
            Ok("cpu") => Backend::CPU,
            _ => Self::default_backend(),
        }
    }
}

pub struct Renderer {
    pub win: Window,
    ctx: Box<dyn RenderBackend>,
    pub font_cache: Rc<RefCell<FontCache>>,
    pub icon_font_cache: Rc<RefCell<FontCache>>,
    batches: Vec<RenderBatch>,
}

use log;
impl Renderer {
    pub fn new(win: Window) -> Self {
        Self::with_backend(win, Backend::from_env())
    }

    pub fn with_backend(win: Window, backend: Backend) -> Self {
        println!("render new with backend: {:?}", backend);

        let t0 = std::time::Instant::now();
        let ctx: Box<dyn RenderBackend> = Self::create_backend(&win, backend);
        println!("[profile] create_backend: {:?}", t0.elapsed());

        let t1 = std::time::Instant::now();
        let font_cache = Rc::new(RefCell::new(FontCache::new(include_bytes!(
            "../../assets/NotoSans-Regular.ttf"
        ))));
        println!("[profile] font_cache (NotoSans): {:?}", t1.elapsed());

        let t2 = std::time::Instant::now();
        let icon_font_cache = Rc::new(RefCell::new(FontCache::new(include_bytes!(
            "../../assets/MaterialIcons-Regular.ttf"
        ))));
        println!(
            "[profile] icon_font_cache (MaterialIcons): {:?}",
            t2.elapsed()
        );

        let mut batches = Vec::new();
        batches.push(RenderBatch::new(100));
        let mut renderer = Renderer {
            win,
            ctx,
            font_cache,
            icon_font_cache,
            batches,
        };

        let t3 = std::time::Instant::now();
        renderer.update_font_texture(false);
        println!("[profile] update_font_texture (text): {:?}", t3.elapsed());

        let t4 = std::time::Instant::now();
        renderer.update_font_texture(true);
        println!("[profile] update_font_texture (icons): {:?}", t4.elapsed());

        println!("[profile] total init: {:?}", t0.elapsed());
        renderer
    }

    fn create_backend(win: &Window, backend: Backend) -> Box<dyn RenderBackend> {
        match backend {
            #[cfg(feature = "opengl")]
            Backend::OpenGL => Box::new(opengl::GLContext::new(win)),
            #[cfg(feature = "cpu")]
            Backend::CPU => Box::new(cpu::CPUContext::new(win)),
        }
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

    pub fn vsync(&mut self, enable: bool) {
        self.ctx.vsync(enable);
    }
}
