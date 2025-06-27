use std::{cell::RefCell, rc::Rc};

use crate::render::{Extra, Rect2DInst, RectCoords, RenderBatch, Renderer, V4f32};

pub mod color {
    use crate::render::V4f32;
    pub const BLACK: V4f32 = V4f32 {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const TMP: V4f32 = V4f32 {
        r: 1.0,
        g: 0.4,
        b: 0.5,
        a: 1.0,
    };
    pub const TMP2: V4f32 = V4f32 {
        r: 1.0,
        g: 0.5,
        b: 0.6,
        a: 1.0,
    };
    pub const FPS: V4f32 = V4f32 {
        r: 1.0,
        g: 0.2,
        b: 0.2,
        a: 1.0,
    };
    pub const WHITE: V4f32 = V4f32 {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
}

pub struct Drawer {
    pub renderer: Rc<RefCell<Renderer>>,
}

/// Drawer class - its purpose is to provide a draw API
/// that will translate it into commands for the renderer.
impl Drawer {
    pub fn new(renderer: Rc<RefCell<Renderer>>) -> Self {
        Drawer { renderer }
    }

    pub fn draw_rect(&self, coords: &RectCoords, color: V4f32) {
        let mut renderer = self.renderer.borrow_mut();
        let mut batch = RenderBatch::new(1);
        let rect = Rect2DInst {
            dst: *coords,
            src: RectCoords {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            },
            colors: [color, color, color, color],
            extra: Extra::new(true),
        };
        batch.add_rect(rect);
        renderer.add_batch(batch);
    }

    pub fn draw_text(&self, x: f32, y: f32, size: u32, text: &str, length: usize, color: V4f32) {
        let mut renderer = self.renderer.borrow_mut();
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
        let y = y + size as f32;
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

            // xarkes: push a rect instruction for each character
            let w = (glyph.width) as f32;
            let h = (glyph.height) as f32;
            let xpos = x + glyph.xoff;
            let ypos = y + glyph.yoff;
            batch.add_rect(Rect2DInst {
                dst: RectCoords {
                    x0: xpos,
                    y0: ypos,
                    x1: xpos + w,
                    y1: ypos + h,
                },
                src: RectCoords {
                    x0: glyph.x0,
                    y0: glyph.y0,
                    x1: glyph.x1,
                    y1: glyph.y1,
                },
                colors: [color, color, color, color],
                extra: Extra::new(false),
            });
            x += glyph.advance;
        }
        renderer.add_batch(batch);
    }
}
