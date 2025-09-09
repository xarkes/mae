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

    pub fn draw_empty_rect(&mut self, coords: &RectCoords, color: V4f32, line_width: f32) {
        let scale_factor = self.renderer.win.dpi;
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
            self.renderer.add_rect(rect, None);
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

    pub fn get_text_size(&self, size: f32, text: &str, length: usize) -> (f32, f32) {
        // let scale_factor = self.renderer.win.dpi;
        // let size = size * scale_factor;
        let (width, height) = self
            .renderer
            .font_cache
            .borrow_mut()
            .get_text_size(size, text, length);
        (width, height)
    }

    pub fn draw_text(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        text: &str,
        length: usize,
        xmax: f32,
        ymax: f32,
        color: V4f32,
        underflow: bool,
        font_icon: bool,
    ) -> f32 {
        let scale_factor = self.renderer.win.dpi;
        let size = size * scale_factor;
        let xstart = x * scale_factor;
        let xmax = xmax * scale_factor;
        let ymax = ymax * scale_factor;

        // We update the font texture before drawing chars
        // Drawing chars may update the atlas, and thus the next frame would use a wrong atlas
        // But we accept it and wait for next draw call to update the texture
        self.renderer.update_font_texture(font_icon);

        let mut x = xstart;
        let y = y * scale_factor + size;
        for (i, c) in text.char_indices() {
            if i >= length {
                break;
            }
            if x >= xmax {
                break;
            }
            if c == '\t' {
                x += size;
                continue;
            }
            let (glyph, texture_id) = match font_icon {
                true => {
                    let mut fc = self.renderer.icon_font_cache.borrow_mut();
                    let texture = fc.texture_id;
                    (fc.get(c, size).clone(), texture)
                }
                false => {
                    let mut fc = self.renderer.font_cache.borrow_mut();
                    let texture = fc.texture_id;
                    (fc.get(c, size).clone(), texture)
                }
            };

            // xarkes: push a rect instruction for each character
            let w = (glyph.width) as f32;
            let h = (glyph.height) as f32;
            let xpos = x + glyph.xoff;
            let ypos = y + glyph.yoff;
            {
                let avail_width = xmax - xpos;
                let width_trunc = f32::min(w, avail_width);
                let avail_height = ymax - ypos;
                let height_trunc = f32::min(h, avail_height);
                // xarkes: handle aligned truncation
                let src = match underflow {
                    false => RectCoords {
                        x0: glyph.x0,
                        y0: glyph.y0,
                        // XXX: we use here atlas relative coords, maybe renderer should rework texture coordinates?
                        x1: glyph.x0 + width_trunc / 1024.,
                        y1: glyph.y0 + height_trunc / 1024.,
                    },
                    // XXX: This is wrong but I think I need to rework the whole API anyways :')
                    true => RectCoords {
                        x0: glyph.x1 - width_trunc / 1024.,
                        y0: glyph.y1 - height_trunc / 1024.,
                        // XXX: we use here atlas relative coords, maybe renderer should rework texture coordinates?
                        x1: glyph.x1,
                        y1: glyph.y1,
                    },
                };
                let rect = Rect2DInst {
                    dst: RectCoords {
                        x0: xpos,
                        y0: ypos,
                        x1: xpos + width_trunc,
                        y1: ypos + height_trunc,
                    },
                    src,
                    colors: [color, color, color, color],
                    extra: Extra::new(false),
                };
                self.renderer.add_rect(rect, Some(texture_id));
            }
            x += glyph.advance;
        }
        x - xstart
    }
}
