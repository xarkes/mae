#[cfg(feature = "cpu")]
mod cpu;
pub mod font_cache;
mod font_fallback;
#[cfg(feature = "opengl")]
mod opengl;
#[cfg(feature = "png_capture")]
pub mod png;
pub mod software;

use std::{cell::RefCell, rc::Rc};

use crate::os::Window;
use font_cache::{FontCache, FontTag};

#[derive(Debug)]
enum RendererError {
    OGLInitFailed(String),
}

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
        format: TextureFormat,
    );
    fn remove_texture(&mut self, id: u32);
    fn resize(&mut self, w: f32, h: f32);
    fn begin_frame(&mut self);
    /// Colour the framebuffer is cleared to at the start of each frame.
    ///
    /// Anything the UI does not paint shows through as this — including a box
    /// faded below full opacity, which composites against it. Left at the
    /// default transparent black, a fading view dissolves toward black no
    /// matter how light the theme is, which is why this follows
    /// `UITheme::app_bg`.
    fn set_clear_color(&mut self, color: V4f32);
    fn end_frame(&mut self);
    fn render(&mut self, batches: &Vec<RenderBatch>);
    fn vsync(&mut self, enable: bool);
    fn backend(&self) -> Backend;
    /// Capture the current framebuffer as RGBA bytes (row-major, top-to-bottom).
    #[cfg(feature = "png_capture")]
    fn capture_framebuffer(&mut self) -> (Vec<u8>, usize, usize);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFormat {
    R8,
    Rgba8,
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
    /// 1.0 if the sampled texture is a color (RGBA) glyph to use directly rather
    /// than an alpha mask to tint. Maps to `c2v_extra.z` in the shaders.
    pub is_color: f32,
    pub _unused: f32,
}
impl Extra {
    pub fn new(omit_texture: bool, corner_radius_px: f32) -> Self {
        Self::with_color(omit_texture, corner_radius_px, false)
    }

    pub fn with_color(omit_texture: bool, corner_radius_px: f32, is_color: bool) -> Self {
        let omit = match omit_texture {
            true => 1.0,
            false => 0.0,
        };
        Extra {
            omit_texture: omit,
            corner_radius_px,
            is_color: if is_color { 1.0 } else { 0.0 },
            _unused: 0.0,
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
    pub fn as_str(self) -> &'static str {
        match self {
            #[cfg(feature = "opengl")]
            Backend::OpenGL => "OpenGL",
            #[cfg(feature = "cpu")]
            Backend::CPU => "CPU",
        }
    }

    /// Returns a vec of vailable backends
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
    pub fn from_env() -> Option<Self> {
        match std::env::var("MAE_RENDERER").as_deref() {
            #[cfg(feature = "opengl")]
            Ok("opengl") => Some(Backend::OpenGL),
            #[cfg(feature = "cpu")]
            Ok("cpu") => Some(Backend::CPU),
            _ => None,
        }
    }
}

pub struct Renderer {
    pub win: Window,
    ctx: Box<dyn RenderBackend>,
    pub font_cache: Rc<RefCell<FontCache>>,
    pub icon_font_cache: Rc<RefCell<FontCache>>,
    batches: Vec<RenderBatch>,
    #[cfg(feature = "png_capture")]
    pending_capture: Option<String>,
}

impl Renderer {
    pub fn new(win: Window) -> Self {
        let backend = Backend::from_env();

        let ctx: Box<dyn RenderBackend> = Self::create_backend(&win, backend);

        let font_cache = Rc::new(RefCell::new(FontCache::new_with_tag(
            FontTag::Main,
            include_bytes!("../../assets/NotoSans-Regular.ttf"),
        )));

        let icon_font_cache = Rc::new(RefCell::new(FontCache::new_with_tag(
            FontTag::Icon,
            include_bytes!("../../assets/MaterialIcons-Regular.ttf"),
        )));

        let batches = vec![RenderBatch::new(100)];
        let mut renderer = Renderer {
            win,
            ctx,
            font_cache,
            icon_font_cache,
            batches,
            #[cfg(feature = "png_capture")]
            pending_capture: None,
        };
        renderer.update_font_texture(false);
        renderer.update_font_texture(true);
        renderer
    }

    fn create_backend(win: &Window, backend: Option<Backend>) -> Box<dyn RenderBackend> {
        let mut backends = Backend::available();
        if backend.is_some() {
            backends = vec![backend.unwrap()];
        }
        for backend in backends {
            // Explicit annotation: with neither `opengl` nor `cpu` enabled (the DOM
            // backend's build — see `imui/lifecycle.rs::new_dom`, which never calls
            // `Renderer::new` at all), `Backend` is uninhabited and this match has no
            // arms, leaving the compiler nothing to infer the type from even though
            // the loop body is unreachable.
            let ctx: Result<Box<dyn RenderBackend>, RendererError> = match backend {
                #[cfg(feature = "opengl")]
                Backend::OpenGL => opengl::GLContext::new(win),
                #[cfg(feature = "cpu")]
                Backend::CPU => cpu::CPUContext::new(win),
            };
            match ctx {
                Ok(c) => {
                    return c;
                }
                Err(e) => {
                    println!("Backend initialization failed: {:?}", e);
                }
            }
        }
        panic!("No backend could be initialized.");
    }

    /// Upload an RGBA8 image as a GPU texture and return its id. Used for
    /// inline document images; the caller owns the lifetime and frees it with
    /// [`Renderer::remove_image_texture`].
    pub fn create_image_texture(&mut self, width: usize, height: usize, rgba: &[u8]) -> u32 {
        let id = self.ctx.create_texture(width, height, TextureFormat::Rgba8);
        self.ctx
            .update_texture_region(id, 0, 0, width, height, rgba, TextureFormat::Rgba8);
        id
    }

    pub fn remove_image_texture(&mut self, id: u32) {
        if id != 0 {
            self.ctx.remove_texture(id);
        }
    }

    pub fn update_font_texture(&mut self, font_icon: bool) {
        let cache = match font_icon {
            true => &self.icon_font_cache,
            false => &self.font_cache,
        };
        if !cache.borrow().needs_texture_update() {
            return;
        }

        let mut fc = match font_icon {
            true => self.icon_font_cache.borrow_mut(),
            false => self.font_cache.borrow_mut(),
        };

        // Alpha (R8) atlases.
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
                    TextureFormat::R8,
                );
            }
        }

        // Color (RGBA) atlases.
        for atlas_index in 0..fc.color_atlas_count() {
            if fc.color_atlas_texture_id(atlas_index) == 0 {
                let atlas = fc.color_atlas(atlas_index);
                let texture_id =
                    self.ctx
                        .create_texture(atlas.width, atlas.height, TextureFormat::Rgba8);
                fc.color_atlas_mut(atlas_index).set_texture_id(texture_id);
                let atlas = fc.color_atlas(atlas_index);
                self.ctx.update_texture_region(
                    texture_id,
                    0,
                    0,
                    atlas.width,
                    atlas.height,
                    &atlas.data,
                    TextureFormat::Rgba8,
                );
            }
        }

        for upload in fc.take_pending_uploads() {
            let (texture_id, format) = if upload.color {
                (
                    fc.color_atlas_texture_id(upload.atlas_index),
                    TextureFormat::Rgba8,
                )
            } else {
                (fc.atlas_texture_id(upload.atlas_index), TextureFormat::R8)
            };
            if texture_id != 0 {
                self.ctx.update_texture_region(
                    texture_id,
                    upload.x,
                    upload.y,
                    upload.width,
                    upload.height,
                    &upload.data,
                    format,
                );
            }
        }
        fc.mark_textures_clean();
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
        self.ctx.backend()
    }

    pub fn set_backend(&mut self, backend: Backend) {
        if self.ctx.backend() == backend {
            return;
        }

        self.remove_font_textures(false);
        self.remove_font_textures(true);
        self.ctx = Self::create_backend(&self.win, Some(backend));

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

        #[cfg(feature = "png_capture")]
        if let Some(path) = self.pending_capture.take() {
            let (rgba, w, h) = self.ctx.capture_framebuffer();
            match png::write_png(&path, w, h, &rgba) {
                Ok(()) => println!("PNG captured to: {path}"),
                Err(e) => eprintln!("PNG capture failed: {e}"),
            }
        }

        self.ctx.end_frame();

        self.batches.clear();
        self.batches.push(RenderBatch::new(100));
        self.begin_font_frame();
    }

    #[cfg(feature = "png_capture")]
    pub fn request_capture(&mut self, path: String) {
        self.pending_capture = Some(path);
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

    /// See [`RenderBackend::set_clear_color`].
    pub fn set_clear_color(&mut self, color: V4f32) {
        self.ctx.set_clear_color(color);
    }
}
