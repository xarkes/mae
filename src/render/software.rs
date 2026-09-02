use std::collections::HashMap;

use super::{Rect2DInst, RenderBatch, V4f32};

pub const DEFAULT_CLEAR_COLOR: u32 = 0xFF758B99;

#[derive(Clone, Debug)]
pub struct Texture {
    pub width: usize,
    pub height: usize,
    /// 1 for an alpha (R8) atlas, 4 for a color (RGBA8) atlas.
    pub bytes_per_pixel: usize,
    pub data: Vec<u8>,
}

impl Texture {
    pub fn update_region(&mut self, x: usize, y: usize, width: usize, height: usize, data: &[u8]) {
        let bpp = self.bytes_per_pixel;
        assert!(x + width <= self.width);
        assert!(y + height <= self.height);
        assert_eq!(data.len(), width * height * bpp);
        let row_bytes = width * bpp;
        for row in 0..height {
            let dst = ((y + row) * self.width + x) * bpp;
            let src = row * row_bytes;
            self.data[dst..dst + row_bytes].copy_from_slice(&data[src..src + row_bytes]);
        }
    }

    pub fn is_color(&self) -> bool {
        self.bytes_per_pixel == 4
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoftwareSurface {
    width: usize,
    height: usize,
    pixels: Vec<u32>,
}

impl SoftwareSurface {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.pixels
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.pixels.resize(width * height, 0);
    }

    pub fn clear(&mut self, argb: u32) {
        self.pixels.fill(argb);
    }
}

pub fn render_batches(
    surface: &mut SoftwareSurface,
    batches: &[RenderBatch],
    textures: &HashMap<u32, Texture>,
) {
    for batch in batches.iter() {
        let texture = batch.texture().and_then(|id| textures.get(&id));
        for inst in batch.rects().iter() {
            draw_rect(surface, inst, texture);
        }
    }
}

fn draw_rect(surface: &mut SoftwareSurface, inst: &Rect2DInst, texture: Option<&Texture>) {
    let dst = &inst.dst;
    let src = &inst.src;
    let colors = &inst.colors;
    let omit_texture = inst.extra.omit_texture > 0.5;
    let radius = inst.extra.corner_radius_px.max(0.0);

    let x0 = (dst.x0.max(0.0) as usize).min(surface.width);
    let y0 = (dst.y0.max(0.0) as usize).min(surface.height);
    let x1 = (dst.x1.max(0.0).ceil() as usize).min(surface.width);
    let y1 = (dst.y1.max(0.0).ceil() as usize).min(surface.height);

    if x0 >= x1 || y0 >= y1 {
        return;
    }

    if omit_texture {
        if let Some(color) = solid_color(colors) {
            draw_solid_rect(surface, inst, color, x0, y0, x1, y1, radius);
            return;
        }
    }

    let dst_w = dst.x1 - dst.x0;
    let dst_h = dst.y1 - dst.y0;

    for py in y0..y1 {
        for px in x0..x1 {
            let sample_x = px as f32 + 0.5;
            let sample_y = py as f32 + 0.5;
            let coverage = if radius > 0.0 {
                rounded_rect_coverage(sample_x, sample_y, inst)
            } else {
                1.0
            };
            if coverage <= 0.0 {
                continue;
            }

            let t_x = if dst_w > 0.0 {
                (sample_x - dst.x0) / dst_w
            } else {
                0.0
            };
            let t_y = if dst_h > 0.0 {
                (sample_y - dst.y0) / dst_h
            } else {
                0.0
            };

            let color = bilinear_color(colors, t_x, t_y);
            let final_color = if !omit_texture {
                if let Some(tex) = texture {
                    let tex_u = src.x0 + t_x * (src.x1 - src.x0);
                    let tex_v = src.y0 + t_y * (src.y1 - src.y0);
                    if tex.is_color() {
                        // Color glyph (emoji): use RGBA directly, modulated by tint opacity.
                        let c = sample_texture_rgba(tex, tex_u, tex_v);
                        V4f32 {
                            r: c.r,
                            g: c.g,
                            b: c.b,
                            a: c.a * color.a,
                        }
                    } else {
                        let alpha = sample_texture(tex, tex_u, tex_v);
                        V4f32 {
                            r: color.r,
                            g: color.g,
                            b: color.b,
                            a: color.a * alpha,
                        }
                    }
                } else {
                    color
                }
            } else {
                color
            };

            let mut final_color = final_color;
            final_color.a *= coverage;

            let idx = py * surface.width + px;
            if idx < surface.pixels.len() {
                surface.pixels[idx] = blend_pixel(surface.pixels[idx], &final_color);
            }
        }
    }
}

fn draw_solid_rect(
    surface: &mut SoftwareSurface,
    inst: &Rect2DInst,
    color: V4f32,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    radius: f32,
) {
    if color.a >= 1.0 {
        let packed = pack_color(
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            255,
        );
        if radius <= 0.0 {
            for py in y0..y1 {
                let row = py * surface.width;
                surface.pixels[row + x0..row + x1].fill(packed);
            }
            return;
        }

        for py in y0..y1 {
            let row = py * surface.width;
            for px in x0..x1 {
                let coverage = rounded_rect_coverage(px as f32 + 0.5, py as f32 + 0.5, inst);
                if coverage >= 1.0 {
                    surface.pixels[row + px] = packed;
                } else if coverage > 0.0 {
                    // Edge pixel: blend the opaque colour in proportionally
                    // rather than snapping it fully on or fully off.
                    let mut edge = color;
                    edge.a *= coverage;
                    surface.pixels[row + px] = blend_pixel(surface.pixels[row + px], &edge);
                }
            }
        }
        return;
    }

    for py in y0..y1 {
        let row = py * surface.width;
        for px in x0..x1 {
            let coverage = if radius <= 0.0 {
                1.0
            } else {
                rounded_rect_coverage(px as f32 + 0.5, py as f32 + 0.5, inst)
            };
            if coverage <= 0.0 {
                continue;
            }
            let mut blended = color;
            blended.a *= coverage;
            surface.pixels[row + px] = blend_pixel(surface.pixels[row + px], &blended);
        }
    }
}

fn solid_color(colors: &[V4f32; 4]) -> Option<V4f32> {
    let color = colors[0];
    colors[1..]
        .iter()
        .all(|other| same_color(color, *other))
        .then_some(color)
}

fn same_color(a: V4f32, b: V4f32) -> bool {
    a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a
}

/// How much of the pixel at `(x, y)` the rounded rect covers, `0.0..=1.0`.
///
/// A one-pixel linear ramp across the corner arc, matching
/// `opengl/fragment.glsl` so the two backends antialias identically. Only the
/// arcs need this — the straight edges are already exact, being the loop bounds.
fn rounded_rect_coverage(x: f32, y: f32, inst: &Rect2DInst) -> f32 {
    let rect = inst.dst;
    let radius = inst
        .extra
        .corner_radius_px
        .max(0.0)
        .min(rect.width().abs() * 0.5)
        .min(rect.height().abs() * 0.5);
    if radius <= 0.0 {
        return 1.0;
    }
    // Distance from the nearest corner circle's centre, minus its radius: the
    // signed distance to the boundary, in pixels.
    let cx = x.clamp(rect.x0 + radius, rect.x1 - radius);
    let cy = y.clamp(rect.y0 + radius, rect.y1 - radius);
    let distance = (x - cx).hypot(y - cy) - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

#[inline]
pub fn pack_color(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

#[inline]
fn unpack_color(pixel: u32) -> (u8, u8, u8, u8) {
    let a = ((pixel >> 24) & 0xFF) as u8;
    let r = ((pixel >> 16) & 0xFF) as u8;
    let g = ((pixel >> 8) & 0xFF) as u8;
    let b = (pixel & 0xFF) as u8;
    (r, g, b, a)
}

#[inline]
fn bilinear_color(colors: &[V4f32; 4], t_x: f32, t_y: f32) -> V4f32 {
    let top = lerp_color(&colors[0], &colors[1], t_x);
    let bottom = lerp_color(&colors[2], &colors[3], t_x);
    lerp_color(&top, &bottom, t_y)
}

#[inline]
fn lerp_color(a: &V4f32, b: &V4f32, t: f32) -> V4f32 {
    V4f32 {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

#[inline]
fn sample_texture(tex: &Texture, u: f32, v: f32) -> f32 {
    let tx = ((u * tex.width as f32) as usize).min(tex.width.saturating_sub(1));
    let ty = ((v * tex.height as f32) as usize).min(tex.height.saturating_sub(1));
    let idx = ty * tex.width + tx;
    if idx < tex.data.len() {
        tex.data[idx] as f32 / 255.0
    } else {
        0.0
    }
}

fn sample_texture_rgba(tex: &Texture, u: f32, v: f32) -> V4f32 {
    let tx = ((u * tex.width as f32) as usize).min(tex.width.saturating_sub(1));
    let ty = ((v * tex.height as f32) as usize).min(tex.height.saturating_sub(1));
    let idx = (ty * tex.width + tx) * 4;
    if idx + 3 < tex.data.len() {
        V4f32 {
            r: tex.data[idx] as f32 / 255.0,
            g: tex.data[idx + 1] as f32 / 255.0,
            b: tex.data[idx + 2] as f32 / 255.0,
            a: tex.data[idx + 3] as f32 / 255.0,
        }
    } else {
        V4f32::default()
    }
}

#[inline]
fn blend_pixel(existing: u32, color: &V4f32) -> u32 {
    let (er, eg, eb, _ea) = unpack_color(existing);
    let alpha = color.a;

    if alpha >= 1.0 {
        pack_color(
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            255,
        )
    } else if alpha <= 0.0 {
        existing
    } else {
        let inv_alpha = 1.0 - alpha;
        let r = (color.r * 255.0 * alpha + er as f32 * inv_alpha) as u8;
        let g = (color.g * 255.0 * alpha + eg as f32 * inv_alpha) as u8;
        let b = (color.b * 255.0 * alpha + eb as f32 * inv_alpha) as u8;
        pack_color(r, g, b, 255)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_region_update_only_changes_target_bytes() {
        let mut texture = Texture {
            width: 4,
            height: 4,
            bytes_per_pixel: 1,
            data: vec![0; 16],
        };
        texture.update_region(1, 1, 2, 2, &[1, 2, 3, 4]);
        assert_eq!(texture.data[5], 1);
        assert_eq!(texture.data[6], 2);
        assert_eq!(texture.data[9], 3);
        assert_eq!(texture.data[10], 4);
        assert_eq!(texture.data.iter().filter(|&&b| b != 0).count(), 4);
    }
}
