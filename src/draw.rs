use std::{cell::RefCell, rc::Weak};

use crate::render::{Rect2DInst, RenderBatch, Renderer};

pub struct Drawer {
    renderer: Weak<RefCell<Renderer>>,
}

/// Drawer class - its purpose is to provide a draw API
/// that will translate it into commands for the renderer.
impl Drawer {
    pub fn new(renderer: Weak<RefCell<Renderer>>) -> Self {
        Drawer { renderer }
    }

    pub fn draw_rect(&self, x: u32, y: u32, width: u32, height: u32) {
        let rc = self.renderer.upgrade().unwrap();
        let mut renderer = rc.borrow_mut();
        let mut batch = RenderBatch::new(1);
        let x = x as f32;
        let y = y as f32;
        let width = width as f32;
        let height = height as f32;
    }

    pub fn draw_text(&self, x: u32, y: u32, size: u32, text: &str, length: usize) {
        let rc = self.renderer.upgrade().unwrap();
        let mut renderer = rc.borrow_mut();
        let mut batch = RenderBatch::new(text.len());

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
        for (i, c) in text.char_indices() {
            if i >= length {
                break;
            }
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
            let w = (glyph.width) as f32;
            let h = (glyph.height) as f32;
            let xpos = x + glyph.xoff;
            let ypos = y + glyph.yoff;
            let inst = Rect2DInst {
                x: xpos,
                y: ypos + h,
                tex_x: glyph.tl_x,
                tex_y: glyph.br_y,
                x2: xpos,
                y2: ypos,
                tex_x2: glyph.tl_x,
                tex_y2: glyph.tl_y,
                x3: xpos + w,
                y3: ypos,
                tex_x3: glyph.br_x,
                tex_y3: glyph.tl_y,
                x4: xpos,
                y4: ypos + h,
                tex_x4: glyph.tl_x,
                tex_y4: glyph.br_y,
                x5: xpos + w,
                y5: ypos,
                tex_x5: glyph.br_x,
                tex_y5: glyph.tl_y,
                x6: xpos + w,
                y6: ypos + h,
                tex_x6: glyph.br_x,
                tex_y6: glyph.br_y,
            };
            batch.add_rect(inst);
            x += glyph.advance;
        }
        renderer.add_batch(batch);
    }
}
