#[cfg(feature = "opengl")]
mod opengl;

use crate::os::Window;
use opengl::GLContext;

pub struct Renderer {
    pub win: Window,
    pub ctx: Box<GLContext>,
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
        let ctx = match renderer {
            "opengl" => Box::new(GLContext::new(&win)),
            _ => {
                panic!("Renderer not implemented!");
            }
        };
        Renderer { win, ctx }
    }

    pub fn update(&self) {
        self.ctx.update();
    }
}
