mod font_cache;
#[cfg(feature = "opengl")]
mod opengl;

use crate::os::Window;
use font_cache::FontCache;

pub(crate) struct RenderCommand {
    data: [f32; 24],
}

impl RenderCommand {
    pub fn new(data: [f32; 24]) -> Self {
        RenderCommand { data }
    }
}

pub(crate) struct RenderRun {
    commands: Vec<RenderCommand>,
}

impl RenderRun {
    pub fn new(prealloc: usize) -> Self {
        RenderRun {
            commands: Vec::with_capacity(prealloc),
        }
    }

    pub fn add_command(&mut self, cmd: RenderCommand) {
        self.commands.push(cmd);
    }
}

pub struct Renderer {
    pub win: Window,
    pub ctx: Box<opengl::GLContext>,
    pub font_cache: FontCache,
    runs: Vec<RenderRun>,
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
            runs: Vec::new(),
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
        self.ctx.render(&self.runs);
        self.ctx.end_frame();
        self.runs.clear();
    }

    pub fn add_run(&mut self, run: RenderRun) {
        self.runs.push(run);
    }
}
