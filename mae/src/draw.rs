use crate::render::{Extra, Rect2DInst, RectCoords, Renderer, V4f32};

pub struct Drawer {
    pub renderer: Renderer,
}

/// Drawer class - its purpose is to provide a draw API
/// that will translate it into commands for the renderer.
impl Drawer {
    pub fn new(renderer: Renderer) -> Self {
        Drawer { renderer }
    }

    pub fn draw_empty_rect(
        &mut self,
        coords: &RectCoords,
        color: V4f32,
        line_width: f32,
        debug: bool,
    ) {
        let scale_factor = self.renderer.win.dpi;
        let batch = match debug {
            false => self.renderer.current_batch(),
            true => self.renderer.debug_batch(),
        };
        let bounds = [
            RectCoords {
                x0: coords.x0,
                x1: coords.x1,
                y0: coords.y0,
                y1: coords.y0 + line_width,
            }
            .mul(scale_factor),
            RectCoords {
                x0: coords.x0,
                x1: coords.x1,
                y0: coords.y1,
                y1: coords.y1 - line_width,
            }
            .mul(scale_factor),
            RectCoords {
                x0: coords.x0,
                x1: coords.x0 + line_width,
                y0: coords.y0,
                y1: coords.y1,
            }
            .mul(scale_factor),
            RectCoords {
                x0: coords.x1,
                x1: coords.x1 - line_width,
                y0: coords.y0,
                y1: coords.y1,
            }
            .mul(scale_factor),
        ];
        for rectbounds in bounds {
            let rect = Rect2DInst {
                dst: rectbounds,
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
        }
    }
    pub fn draw_rect(&mut self, coords: &RectCoords, color: V4f32) {
        let scale_factor = self.renderer.win.dpi;
        let batch = self.renderer.current_batch();
        let rect = Rect2DInst {
            dst: coords.mul(scale_factor),
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
    }

    pub fn get_text_size(&mut self, size: u32, text: &str, length: usize) -> (f32, f32) {
        let (should_update, width, height) = self
            .renderer
            .font_cache
            .borrow_mut()
            .get_text_size(size, text, length);
        if should_update {
            self.renderer.update_font_texture();
        }
        (width, height)
    }

    pub fn draw_text(
        &mut self,
        x: f32,
        y: f32,
        size: u32,
        text: &str,
        length: usize,
        color: V4f32,
    ) -> f32 {
        let scale_factor = self.renderer.win.dpi;
        let size = size as f32 * scale_factor;
        // xarkes: Generate glyph for each string character and update texture if needed
        // This is likely dumb, but that's it for now
        {
            let mut should_update = false;
            for c in text.chars() {
                if c == '\t' {
                    continue;
                }
                let mut fc = self.renderer.font_cache.borrow_mut();
                let (_, added) = fc.get(c, size);
                should_update |= added;
            }
            if should_update {
                self.renderer.update_font_texture();
            }
        }

        let xstart = x as f32;
        let mut x = x as f32;
        let y = y + size;
        for (i, c) in text.char_indices() {
            if i >= length {
                break;
            }
            if c == '\t' {
                x += size;
                continue;
            }
            let glyph = {
                let mut fc = self.renderer.font_cache.borrow_mut();
                let (glyph, _) = fc.get(c, size);
                if glyph.is_none() {
                    continue;
                }
                *glyph.unwrap()
            };

            // xarkes: push a rect instruction for each character
            let w = (glyph.width) as f32;
            let h = (glyph.height) as f32;
            let xpos = x + glyph.xoff;
            let ypos = y + glyph.yoff;
            {
                self.renderer.current_batch().add_rect(Rect2DInst {
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
            }
            x += glyph.advance;
        }
        x - xstart
    }
}
