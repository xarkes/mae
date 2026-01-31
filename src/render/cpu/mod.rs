#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "windows"),
))]
compile_error!("CPU renderer: Support for targeted OS is not implemented!");

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod os_impl;

use crate::os::Window;
use std::collections::HashMap;

use super::V4f32;

pub struct Texture {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

pub struct CPUContext {
    width: usize,
    height: usize,
    framebuffer: Vec<u32>,
    ctx: os_impl::CPUContextHandle,
    textures: HashMap<u32, Texture>,
    next_texture_id: u32,
}

impl CPUContext {
    pub fn new(win: &Window) -> Self {
        let (width, height) = win.get_size();
        let width = width as usize;
        let height = height as usize;
        let ctx = os_impl::cpu_create_context(win);

        println!("CPU software renderer initialized ({}x{})", width, height);

        CPUContext {
            width,
            height,
            framebuffer: vec![0; width * height],
            ctx,
            textures: HashMap::new(),
            next_texture_id: 1,
        }
    }

    pub fn update_font_texture(&mut self, atlas: &crate::render::font_cache::Atlas) -> u32 {
        let texture_id = self.next_texture_id;
        self.next_texture_id += 1;

        self.textures.insert(
            texture_id,
            Texture {
                width: atlas.width,
                height: atlas.height,
                data: atlas.data.clone(),
            },
        );

        texture_id
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        self.width = w as usize;
        self.height = h as usize;
        self.framebuffer.resize(self.width * self.height, 0);
    }

    pub fn begin_frame(&mut self) {
        // Clear framebuffer with background color (same as OpenGL: rgb(117, 139, 153))
        let bg_color = pack_color(117, 139, 153, 255);
        self.framebuffer.fill(bg_color);
    }

    pub fn end_frame(&mut self) {
        os_impl::cpu_swapbuffers(&mut self.ctx, &self.framebuffer, self.width, self.height);
    }

    #[cfg(debug_assertions)]
    pub fn vsync(&mut self, _enable: bool) {
        // vsync not applicable for CPU renderer
    }

    pub fn render(&mut self, batches: &Vec<super::RenderBatch>) {
        for batch in batches.iter() {
            let texture = batch.texture.and_then(|id| self.textures.get(&id));

            for inst in batch.data.iter() {
                draw_rect(
                    &mut self.framebuffer,
                    self.width,
                    self.height,
                    inst,
                    texture,
                );
            }
        }
    }
}

impl super::RenderBackend for CPUContext {
    fn update_font_texture(&mut self, atlas: &super::font_cache::Atlas) -> u32 {
        CPUContext::update_font_texture(self, atlas)
    }

    fn resize(&mut self, w: f32, h: f32) {
        CPUContext::resize(self, w, h)
    }

    fn begin_frame(&mut self) {
        CPUContext::begin_frame(self)
    }

    fn end_frame(&mut self) {
        CPUContext::end_frame(self)
    }

    fn render(&mut self, batches: &Vec<super::RenderBatch>) {
        CPUContext::render(self, batches)
    }

    #[cfg(debug_assertions)]
    fn vsync(&mut self, enable: bool) {
        CPUContext::vsync(self, enable)
    }
}

fn draw_rect(
    framebuffer: &mut [u32],
    fb_width: usize,
    fb_height: usize,
    inst: &super::Rect2DInst,
    texture: Option<&Texture>,
) {
    let dst = &inst.dst;
    let src = &inst.src;
    let colors = &inst.colors;
    let omit_texture = inst.extra.omit_texture > 0.5;

    // Clamp to screen bounds
    let x0 = (dst.x0.max(0.0) as usize).min(fb_width);
    let y0 = (dst.y0.max(0.0) as usize).min(fb_height);
    let x1 = (dst.x1.max(0.0).ceil() as usize).min(fb_width);
    let y1 = (dst.y1.max(0.0).ceil() as usize).min(fb_height);

    if x0 >= x1 || y0 >= y1 {
        return;
    }

    let dst_w = dst.x1 - dst.x0;
    let dst_h = dst.y1 - dst.y0;

    for py in y0..y1 {
        for px in x0..x1 {
            // Compute normalized position within the rect (0..1)
            let t_x = if dst_w > 0.0 {
                (px as f32 - dst.x0) / dst_w
            } else {
                0.0
            };
            let t_y = if dst_h > 0.0 {
                (py as f32 - dst.y0) / dst_h
            } else {
                0.0
            };

            // Bilinear interpolation of corner colors
            let color = bilinear_color(colors, t_x, t_y);

            // Sample texture if present and not omitted
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

            // Alpha blend with existing pixel
            let idx = py * fb_width + px;
            if idx < framebuffer.len() {
                let existing = framebuffer[idx];
                framebuffer[idx] = blend_pixel(existing, &final_color);
            }
        }
    }
}

#[inline]
fn pack_color(r: u8, g: u8, b: u8, a: u8) -> u32 {
    // ARGB format (common for X11)
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
    // colors[0] = top-left, colors[1] = top-right, colors[2] = bottom-left, colors[3] = bottom-right
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
