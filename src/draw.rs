use crate::render::{Extra, Rect2DInst, RectCoords, Renderer, V4f32, font_cache::ATLAS_SIZE};

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
                extra: Extra::new(true, 0.0),
            };
            self.renderer.add_rect(rect, None);
        }
    }
    pub fn draw_rect(&mut self, coords: &RectCoords, color: V4f32, corner_radius: f32) {
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
            extra: Extra::new(true, (corner_radius * scale_factor).max(0.0)),
        };
        batch.add_rect(rect);
    }

    pub fn get_text_size(&self, size: f32, text: &str, length: usize) -> (f32, f32) {
        self.renderer
            .font_cache
            .borrow_mut()
            .get_text_size(size, text, length)
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

        {
            let cache = match font_icon {
                true => &self.renderer.icon_font_cache,
                false => &self.renderer.font_cache,
            };
            cache
                .borrow_mut()
                .run_from_text(size, text, length, scale_factor);
        }
        self.renderer.update_font_texture(font_icon);

        let y = y * scale_factor + size;
        let vertical_clip_pad = match font_icon {
            true => self.renderer.icon_font_cache.borrow_mut().line_height(size) - size,
            false => self.renderer.font_cache.borrow_mut().line_height(size) - size,
        }
        .max(0.0);

        let run = {
            let cache = match font_icon {
                true => &self.renderer.icon_font_cache,
                false => &self.renderer.font_cache,
            };
            cache
                .borrow_mut()
                .run_from_text(size, text, length, scale_factor)
        };

        for piece in &run.pieces {
            let w = piece.subrect_px.width as f32;
            let h = piece.subrect_px.height as f32;
            if w <= 0.0 || h <= 0.0 || piece.texture_id == 0 {
                continue;
            }

            let xpos = xstart + piece.offset_px.0;
            let ypos = y + piece.offset_px.1;
            if xpos >= xmax {
                break;
            }

            {
                let avail_width = xmax - xpos;
                let width_trunc = f32::min(w, avail_width);
                let avail_height = (ymax + vertical_clip_pad) - ypos;
                let height_trunc = f32::min(h, avail_height);
                if width_trunc <= 0.0 || height_trunc <= 0.0 {
                    continue;
                }
                let (u0, v0, u1, v1) = piece.subrect_px.uv(ATLAS_SIZE, ATLAS_SIZE);
                // xarkes: handle aligned truncation
                let src = match underflow {
                    false => RectCoords {
                        x0: u0,
                        y0: v0,
                        x1: u0 + width_trunc / ATLAS_SIZE as f32,
                        y1: v0 + height_trunc / ATLAS_SIZE as f32,
                    },
                    // XXX: This is wrong but I think I need to rework the whole API anyways :')
                    true => RectCoords {
                        x0: u1 - width_trunc / ATLAS_SIZE as f32,
                        y0: v1 - height_trunc / ATLAS_SIZE as f32,
                        x1: u1,
                        y1: v1,
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
                    extra: Extra::new(false, 0.0),
                };
                self.renderer.add_rect(rect, Some(piece.texture_id));
            }
        }
        run.dim.0
    }
}
