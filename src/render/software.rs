use std::collections::HashMap;

use super::{Rect2DInst, RenderBatch, V4f32};

pub const DEFAULT_CLEAR_COLOR: u32 = 0xFF758B99;

#[derive(Clone, Debug)]
pub struct Texture {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
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

    let dst_w = dst.x1 - dst.x0;
    let dst_h = dst.y1 - dst.y0;

    for py in y0..y1 {
        for px in x0..x1 {
            let sample_x = px as f32 + 0.5;
            let sample_y = py as f32 + 0.5;
            if radius > 0.0 && !rounded_rect_contains(sample_x, sample_y, inst) {
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
                    let alpha = sample_texture(tex, tex_u, tex_v);
                    V4f32 {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: color.a * alpha,
                    }
                } else {
                    color
                }
            } else {
                color
            };

            let idx = py * surface.width + px;
            if idx < surface.pixels.len() {
                surface.pixels[idx] = blend_pixel(surface.pixels[idx], &final_color);
            }
        }
    }
}

fn rounded_rect_contains(x: f32, y: f32, inst: &Rect2DInst) -> bool {
    let rect = inst.dst;
    let radius = inst
        .extra
        .corner_radius_px
        .max(0.0)
        .min(rect.width().abs() * 0.5)
        .min(rect.height().abs() * 0.5);
    if radius <= 0.0 {
        return true;
    }

    let cx = x.clamp(rect.x0 + radius, rect.x1 - radius);
    let cy = y.clamp(rect.y0 + radius, rect.y1 - radius);
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= radius * radius
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
