use std::{cell::RefCell, rc::Weak};

use crate::render::{RenderCommand, RenderRun, Renderer};

pub struct Drawer {
    renderer: Weak<RefCell<Renderer>>,
}

/// Drawer class - its purpose is to provide a draw API
/// that will translate it into commands for the renderer.
impl Drawer {
    pub fn new(renderer: Weak<RefCell<Renderer>>) -> Self {
        Drawer { renderer }
    }

    pub fn draw_text(&self, x: u32, y: u32, size: u32, text: &str) {
        let rc = self.renderer.upgrade().unwrap();
        let mut renderer = rc.borrow_mut();

        let mut run = RenderRun::new(text.len());

        // xarkes: Generate glyph for each string character and update texture if needed
        // This is likely dumb, but that's it for now
        {
            let mut should_update = false;
            for c in text.chars() {
                if c == '\t' {
                    continue;
                }
                let (_, added) = renderer.font_cache.get(c);
                should_update |= added;
            }
            if should_update {
                renderer.update_font_texture();
            }
        }

        let mut x = x as f32;
        let y = y as f32;
        for c in text.chars() {
            if c == '\t' {
                x += size as f32;
                continue;
            }
            let (glyph, _) = renderer.font_cache.get(c);
            if glyph.is_none() {
                continue;
            }
            let glyph = glyph.unwrap();

            // xarkes: Update VBO for each character
            // TODO(xarkes): Would batching this in one command be a better thing to do?
            let w = (glyph.width) as f32;
            let h = (glyph.height) as f32;
            let xpos = x + glyph.xoff;
            let ypos = y + glyph.yoff;
            let vbo_data: [f32; 24] = [
                xpos,
                ypos + h,
                glyph.tl_x,
                glyph.br_y,
                xpos,
                ypos,
                glyph.tl_x,
                glyph.tl_y,
                xpos + w,
                ypos,
                glyph.br_x,
                glyph.tl_y,
                xpos,
                ypos + h,
                glyph.tl_x,
                glyph.br_y,
                xpos + w,
                ypos,
                glyph.br_x,
                glyph.tl_y,
                xpos + w,
                ypos + h,
                glyph.br_x,
                glyph.br_y,
            ];
            run.add_command(RenderCommand::new(vbo_data));
            x += glyph.advance;
        }

        // TODO(xarkes): Cache this run when possible, many texts do not need to be redrawn per frame
        renderer.add_run(run);
    }
}
