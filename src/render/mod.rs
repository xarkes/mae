#[cfg(feature = "cpu")]
mod cpu;
pub mod font_cache;
#[cfg(feature = "opengl")]
mod opengl;
pub mod software;

use std::{cell::RefCell, rc::Rc};

use crate::os::Window;
use font_cache::{FontCache, FontTag};

pub trait RenderBackend {
    fn create_texture(&mut self, width: usize, height: usize, format: TextureFormat) -> u32;
    fn update_texture_region(
        &mut self,
        id: u32,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        data: &[u8],
    );
    fn remove_texture(&mut self, id: u32);
    fn resize(&mut self, w: f32, h: f32);
    fn begin_frame(&mut self);
    fn end_frame(&mut self);
    fn render(&mut self, batches: &Vec<RenderBatch>);
    fn vsync(&mut self, enable: bool);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormat {
    R8,
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
    pub corner_radius_px: f32,
    pub _unused: [f32; 2],
}
impl Extra {
    pub fn new(omit_texture: bool, corner_radius_px: f32) -> Self {
        let omit = match omit_texture {
            true => 1.0,
            false => 0.0,
        };
        Extra {
            omit_texture: omit,
            corner_radius_px,
            _unused: [0.0, 0.0],
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

    pub fn rects(&self) -> &[Rect2DInst] {
        &self.data
    }

    pub fn texture(&self) -> Option<u32> {
        self.texture
    }

    pub fn set_texture(&mut self, texture: Option<u32>) {
        self.texture = texture;
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
    pub fn label(self) -> &'static str {
        match self {
            #[cfg(feature = "opengl")]
            Backend::OpenGL => "OpenGL",
            #[cfg(feature = "cpu")]
            Backend::CPU => "CPU",
        }
    }

    pub fn available() -> Vec<Self> {
        vec![
            #[cfg(feature = "opengl")]
            Backend::OpenGL,
            #[cfg(feature = "cpu")]
            Backend::CPU,
        ]
    }

    /// Returns the default backend (prefers OpenGL if available)
    pub fn default_backend() -> Self {
        Self::available()
            .into_iter()
            .next()
            .expect("at least one renderer backend must be available")
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
    backend: Backend,
    ctx: Box<dyn RenderBackend>,
    pub font_cache: Rc<RefCell<FontCache>>,
    pub icon_font_cache: Rc<RefCell<FontCache>>,
    batches: Vec<RenderBatch>,
}

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
        let font_cache = Rc::new(RefCell::new(FontCache::new_with_tag(
            FontTag::Main,
            include_bytes!("../../assets/NotoSans-Regular.ttf"),
        )));
        println!("[profile] font_cache (NotoSans): {:?}", t1.elapsed());

        let t2 = std::time::Instant::now();
        let icon_font_cache = Rc::new(RefCell::new(FontCache::new_with_tag(
            FontTag::Icon,
            include_bytes!("../../assets/MaterialIcons-Regular.ttf"),
        )));
        println!(
            "[profile] icon_font_cache (MaterialIcons): {:?}",
            t2.elapsed()
        );

        let batches = vec![RenderBatch::new(100)];
        let mut renderer = Renderer {
            win,
            backend,
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

        for atlas_index in 0..fc.atlas_count() {
            if fc.atlas_texture_id(atlas_index) == 0 {
                let atlas = fc.atlas(atlas_index);
                let texture_id =
                    self.ctx
                        .create_texture(atlas.width, atlas.height, TextureFormat::R8);
                fc.atlas_mut(atlas_index).set_texture_id(texture_id);
                let atlas = fc.atlas(atlas_index);
                self.ctx.update_texture_region(
                    texture_id,
                    0,
                    0,
                    atlas.width,
                    atlas.height,
                    &atlas.data,
                );
            }
        }

        for upload in fc.take_pending_uploads() {
            let texture_id = fc.atlas_texture_id(upload.atlas_index);
            if texture_id != 0 {
                self.ctx.update_texture_region(
                    texture_id,
                    upload.x,
                    upload.y,
                    upload.width,
                    upload.height,
                    &upload.data,
                );
            }
        }

        fc.refresh_run_texture_ids();
    }

    pub fn begin_font_frame(&mut self) {
        self.font_cache.borrow_mut().begin_frame();
        self.icon_font_cache.borrow_mut().begin_frame();
    }

    fn remove_font_textures(&mut self, font_icon: bool) {
        let mut fc = match font_icon {
            true => self.icon_font_cache.borrow_mut(),
            false => self.font_cache.borrow_mut(),
        };
        for atlas_index in 0..fc.atlas_count() {
            let texture_id = fc.atlas_texture_id(atlas_index);
            if texture_id != 0 {
                self.ctx.remove_texture(texture_id);
                fc.atlas_mut(atlas_index).set_texture_id(0);
            }
        }
        fc.mark_backend_lost();
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        self.ctx.resize(w, h);
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn set_backend(&mut self, backend: Backend) {
        if self.backend == backend {
            return;
        }

        self.remove_font_textures(false);
        self.remove_font_textures(true);
        self.ctx = Self::create_backend(&self.win, backend);
        self.backend = backend;

        let render_size = self.win.get_render_size();
        self.resize(render_size.0, render_size.1);

        self.update_font_texture(false);
        self.update_font_texture(true);

        self.batches.clear();
        self.batches.push(RenderBatch::new(100));
    }

    pub fn render_frame(&mut self) {
        self.ctx.begin_frame();
        self.ctx.render(&self.batches);
        self.ctx.end_frame();

        self.batches.clear();
        self.batches.push(RenderBatch::new(100));
        self.begin_font_frame();
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
