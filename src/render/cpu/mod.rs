#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "windows"),
))]
compile_error!("CPU renderer: Support for targeted OS is not implemented!");

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod os_impl;

use crate::os::Window;
use std::collections::HashMap;

use super::software::{self, SoftwareSurface, Texture};

pub struct CPUContext {
    surface: SoftwareSurface,
    ctx: os_impl::CPUContextHandle,
    textures: HashMap<u32, Texture>,
    next_texture_id: u32,
}

impl CPUContext {
    pub fn new(win: &Window) -> Self {
        let (width, height) = win.get_size();
        let width = width as usize;
        let height = height as usize;
        let ctx = os_impl::cpu_create_context(win);

        println!("CPU software renderer initialized ({}x{})", width, height);

        CPUContext {
            surface: SoftwareSurface::new(width, height),
            ctx,
            textures: HashMap::new(),
            next_texture_id: 1,
        }
    }

    pub fn update_font_texture(&mut self, atlas: &crate::render::font_cache::Atlas) -> u32 {
        let texture_id = self.next_texture_id;
        self.next_texture_id += 1;

        self.textures.insert(
            texture_id,
            Texture {
                width: atlas.width,
                height: atlas.height,
                data: atlas.data.clone(),
            },
        );

        texture_id
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        self.surface.resize(w as usize, h as usize);
    }

    pub fn begin_frame(&mut self) {
        self.surface.clear(software::DEFAULT_CLEAR_COLOR);
    }

    pub fn end_frame(&mut self) {
        os_impl::cpu_swapbuffers(
            &mut self.ctx,
            self.surface.pixels(),
            self.surface.width(),
            self.surface.height(),
        );
    }

    pub fn vsync(&mut self, _enable: bool) {
        // vsync not applicable for CPU renderer
    }

    pub fn render(&mut self, batches: &Vec<super::RenderBatch>) {
        software::render_batches(&mut self.surface, batches, &self.textures);
    }
}

impl super::RenderBackend for CPUContext {
    fn update_font_texture(&mut self, atlas: &super::font_cache::Atlas) -> u32 {
        CPUContext::update_font_texture(self, atlas)
    }

    fn resize(&mut self, w: f32, h: f32) {
        CPUContext::resize(self, w, h)
    }

    fn begin_frame(&mut self) {
        CPUContext::begin_frame(self)
    }

    fn end_frame(&mut self) {
        CPUContext::end_frame(self)
    }

    fn render(&mut self, batches: &Vec<super::RenderBatch>) {
        CPUContext::render(self, batches)
    }

    fn vsync(&mut self, enable: bool) {
        CPUContext::vsync(self, enable)
    }
}
