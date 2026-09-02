use std::rc::Rc;

use crate::render::{
    Extra, Rect2DInst, RectCoords, Renderer, V4f32,
    font_cache::{ATLAS_SIZE, TextPiece},
};

pub struct Drawer {
    pub renderer: Renderer,
    text_pieces: Vec<TextPiece>,
    text_texture_ids: Vec<u32>,
    text_color_texture_ids: Vec<u32>,
}

/// Drawer class - its purpose is to provide a draw API
/// that will translate it into commands for the renderer.
impl Drawer {
    pub fn new(renderer: Renderer) -> Self {
        Drawer {
            renderer,
            text_pieces: Vec::new(),
            text_texture_ids: Vec::new(),
            text_color_texture_ids: Vec::new(),
        }
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

    /// Draw an RGBA image texture into `full`, clipped to `clip` (the visible
    /// region after scroll/clip). The texture is mapped 1:1 over `full`; the
    /// uv is adjusted so the clipped portion samples correctly. White vertex
    /// colors leave the image untinted.
    pub fn draw_image(&mut self, full: &RectCoords, clip: &RectCoords, texture_id: u32) {
        if texture_id == 0 {
            return;
        }
        let x0 = full.x0.max(clip.x0);
        let y0 = full.y0.max(clip.y0);
        let x1 = full.x1.min(clip.x1);
        let y1 = full.y1.min(clip.y1);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let fw = (full.x1 - full.x0).max(1e-3);
        let fh = (full.y1 - full.y0).max(1e-3);
        let scale_factor = self.renderer.win.dpi;
        let white = V4f32 {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        let rect = Rect2DInst {
            dst: RectCoords { x0, y0, x1, y1 }.mul(scale_factor),
            src: RectCoords {
                x0: (x0 - full.x0) / fw,
                y0: (y0 - full.y0) / fh,
                x1: (x1 - full.x0) / fw,
                y1: (y1 - full.y0) / fh,
            },
            colors: [white, white, white, white],
            extra: Extra::with_color(false, 0.0, true),
        };
        self.renderer.add_rect(rect, Some(texture_id));
    }

    pub fn get_text_size(&self, size: f32, text: &str, length: usize) -> (f32, f32) {
        self.get_text_size_for_font(size, text, length, false)
    }

    /// Baseline-to-baseline line height (including leading) for the main font.
    pub fn line_height(&self, size: f32) -> f32 {
        self.renderer.font_cache.borrow_mut().line_height(size)
    }

    /// Per-char advances (main font) for caret geometry. See FontCache::char_advances.
    pub fn char_advances(&self, size: f32, text: &str, out: &mut Vec<f32>) {
        self.renderer
            .font_cache
            .borrow_mut()
            .char_advances(size, text, out);
    }

    pub fn get_text_size_for_font(
        &self,
        size: f32,
        text: &str,
        length: usize,
        font_icon: bool,
    ) -> (f32, f32) {
        let cache = match font_icon {
            true => &self.renderer.icon_font_cache,
            false => &self.renderer.font_cache,
        };
        cache.borrow_mut().get_text_size(size, text, length)
    }

    pub fn draw_text(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        text: &str,
        length: usize,
        xmin: f32,
        ymin: f32,
        xmax: f32,
        ymax: f32,
        color: V4f32,
        underflow: bool,
        font_icon: bool,
    ) -> f32 {
        let scale_factor = self.renderer.win.dpi;
        let size = size * scale_factor;
        let xstart = x * scale_factor;
        let xmin = xmin * scale_factor;
        let ymin = ymin * scale_factor;
        let xmax = xmax * scale_factor;
        let ymax = ymax * scale_factor;

        let cache = match font_icon {
            true => Rc::clone(&self.renderer.icon_font_cache),
            false => Rc::clone(&self.renderer.font_cache),
        };
        let run_dim;
        let vertical_clip_pad;
        {
            let mut cache = cache.borrow_mut();
            run_dim =
                cache.run_from_text_into(size, text, length, scale_factor, &mut self.text_pieces);
            vertical_clip_pad = (run_dim.1 - size).max(0.0);
        }
        self.renderer.update_font_texture(font_icon);

        let y = y * scale_factor + size;
        self.text_texture_ids.clear();
        self.text_color_texture_ids.clear();
        {
            let cache = cache.borrow();
            for atlas_index in 0..cache.atlas_count() {
                self.text_texture_ids
                    .push(cache.atlas_texture_id(atlas_index));
            }
            for atlas_index in 0..cache.color_atlas_count() {
                self.text_color_texture_ids
                    .push(cache.color_atlas_texture_id(atlas_index));
            }
        }

        for i in 0..self.text_pieces.len() {
            let piece = self.text_pieces[i];
            let w = piece.subrect_px.width as f32;
            let h = piece.subrect_px.height as f32;
            let texture_id = if piece.color {
                self.text_color_texture_ids[piece.atlas_index]
            } else {
                self.text_texture_ids[piece.atlas_index]
            };
            if w <= 0.0 || h <= 0.0 || texture_id == 0 {
                continue;
            }

            let xpos = xstart + piece.offset_px.0;
            let ypos = y + piece.offset_px.1;
            if xpos >= xmax {
                break;
            }
            if xpos + w <= xmin || ypos >= ymax + vertical_clip_pad || ypos + h <= ymin {
                continue;
            }

            {
                let left_clip = (xmin - xpos).clamp(0.0, w);
                let top_clip = (ymin - ypos).clamp(0.0, h);
                let width_trunc = (xmax - (xpos + left_clip)).min(w - left_clip);
                let height_trunc =
                    ((ymax + vertical_clip_pad) - (ypos + top_clip)).min(h - top_clip);
                if width_trunc <= 0.0 || height_trunc <= 0.0 {
                    continue;
                }
                let (u0, v0, u1, v1) = piece.subrect_px.uv(ATLAS_SIZE, ATLAS_SIZE);
                // xarkes: handle aligned truncation
                let src = match underflow {
                    false => RectCoords {
                        x0: u0 + left_clip / ATLAS_SIZE as f32,
                        y0: v0 + top_clip / ATLAS_SIZE as f32,
                        x1: u0 + (left_clip + width_trunc) / ATLAS_SIZE as f32,
                        y1: v0 + (top_clip + height_trunc) / ATLAS_SIZE as f32,
                    },
                    // XXX: This is wrong but I think I need to rework the whole API anyways :')
                    true => RectCoords {
                        x0: u1 - (left_clip + width_trunc) / ATLAS_SIZE as f32,
                        y0: v1 - (top_clip + height_trunc) / ATLAS_SIZE as f32,
                        x1: u1,
                        y1: v1,
                    },
                };
                let rect = Rect2DInst {
                    dst: RectCoords {
                        x0: xpos + left_clip,
                        y0: ypos + top_clip,
                        x1: xpos + left_clip + width_trunc,
                        y1: ypos + top_clip + height_trunc,
                    },
                    src,
                    colors: [color, color, color, color],
                    extra: Extra::with_color(false, 0.0, piece.color),
                };
                self.renderer.add_rect(rect, Some(texture_id));
            }
        }
        run_dim.0
    }
}
