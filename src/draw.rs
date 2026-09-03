use std::rc::Rc;

use crate::render::{
    Extra, Rect2DInst, RectCoords, Renderer, V4f32,
    font_cache::{ATLAS_SIZE, TextPiece},
};

/// Vertical placement of one glyph quad, in device pixels: the row it starts
/// on, how much of its top is clipped away, and how many rows survive — all
/// snapped to whole pixels. `None` when the glyph is entirely outside the clip.
///
/// The snapping is the point. A glyph mask is rasterized once, at an integer
/// pixel size, and blitted at 1:1 texel-to-pixel — so it is only reproduced
/// faithfully if it lands on the pixel grid. Off the grid, the OpenGL backend
/// samples the atlas with `GL_LINEAR` (`render/opengl/mod.rs`), which reads
/// each row of the mask as a blend of two, softening the glyph and thinning
/// its top and bottom edges to where they can disappear. (The software backend
/// happens not to show this: it truncates its destination rect to whole pixels
/// on its own, so it is the OpenGL path this protects.)
///
/// The baseline is fractional whenever the view is scrolled by a non-integer
/// amount, which on macOS is *any* trackpad scroll: `NSEvent::deltaY` is a
/// CGFloat of sub-pixel precision, so `scroll.y` — and every box position
/// derived from it — lands between pixels and the artifact drifts with the
/// scroll offset. A wheel notch elsewhere gives whole numbers, which is why it
/// shows up there and not on Windows or Linux.
///
/// Horizontal placement is deliberately left alone: glyph x positions come
/// from fractional shaping advances, and the editor's caret and selection
/// geometry (`cum_x`) is measured against those same advances. Snapping x
/// would crispen the glyphs and then put the caret in the wrong place.
fn glyph_rows(
    baseline_y: f32,
    glyph_top: f32,
    height: f32,
    ymin: f32,
    ymax: f32,
) -> Option<GlyphRows> {
    let y0 = (baseline_y + glyph_top).round();
    // Round the near edge and floor the extent, so both the destination rect
    // and the atlas sub-rect it samples stay on whole pixels/texels; flooring
    // also keeps a partly-clipped glyph from spilling past the clip edge.
    let top_clip = (ymin - y0).clamp(0.0, height).round();
    let rows = (ymax - (y0 + top_clip)).min(height - top_clip).floor();
    if rows <= 0.0 {
        return None;
    }
    Some(GlyphRows { y0, top_clip, rows })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GlyphRows {
    /// Device-space y the glyph's own bitmap starts at, before clipping.
    y0: f32,
    /// Rows of the bitmap hidden above `ymin`.
    top_clip: f32,
    /// Rows actually drawn.
    rows: f32,
}

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

        // Snapped to the pixel grid — see `glyph_rows`.
        let y = (y * scale_factor + size).round();
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
            if xpos >= xmax {
                break;
            }
            if xpos + w <= xmin {
                continue;
            }
            let Some(rows) = glyph_rows(y, piece.offset_px.1, h, ymin, ymax + vertical_clip_pad)
            else {
                continue;
            };
            let ypos = rows.y0;

            {
                let left_clip = (xmin - xpos).clamp(0.0, w);
                let top_clip = rows.top_clip;
                let width_trunc = (xmax - (xpos + left_clip)).min(w - left_clip);
                let height_trunc = rows.rows;
                if width_trunc <= 0.0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Fractional baselines are the normal case while scrolling on macOS, and
    /// every one of them must still put the glyph bitmap on whole pixels — the
    /// atlas is blitted 1:1, so anything else resamples the mask.
    #[test]
    fn a_glyph_lands_on_whole_pixels_from_any_baseline() {
        for step in 0..64 {
            let baseline = 100.0 + step as f32 / 64.0;
            let rows = glyph_rows(baseline, -11.0, 14.0, 0.0, 1000.0).expect("glyph is visible");
            assert_eq!(rows.y0.fract(), 0.0, "baseline {baseline}");
            assert_eq!(rows.top_clip.fract(), 0.0, "baseline {baseline}");
            assert_eq!(rows.rows.fract(), 0.0, "baseline {baseline}");
            // Unclipped, the whole bitmap is drawn.
            assert_eq!(rows.rows, 14.0, "baseline {baseline}");
        }
    }

    /// The same, at a clip edge: a partly-scrolled-off line still samples whole
    /// texel rows, which is where the "cut by a pixel or two" showed up.
    #[test]
    fn a_clipped_glyph_still_samples_whole_texel_rows() {
        for step in 0..64 {
            let frac = step as f32 / 64.0;
            let rows = glyph_rows(100.0 + frac, -11.0, 14.0, 92.0 + frac, 200.0)
                .expect("glyph is partly visible");
            assert_eq!(rows.y0.fract(), 0.0);
            assert_eq!(rows.top_clip.fract(), 0.0);
            assert_eq!(rows.rows.fract(), 0.0);
            assert!(rows.top_clip > 0.0, "the top should be clipped here");
            // Never drawn past the clip edge.
            assert!(rows.y0 + rows.top_clip + rows.rows <= 200.0);
        }
    }

    #[test]
    fn a_glyph_fully_outside_the_clip_is_dropped() {
        assert!(glyph_rows(100.0, -11.0, 14.0, 200.0, 300.0).is_none());
        assert!(glyph_rows(100.0, -11.0, 14.0, 0.0, 50.0).is_none());
    }
}
