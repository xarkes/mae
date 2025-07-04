#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as os_impl;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as os_impl;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows::*;

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "windows")
))]
compile_error!("OpenGL not implemented for target OS!");

extern crate gl;
use crate::os::Window;
use gl::types::{GLchar, GLenum, GLint, GLuint};
use std::ffi::CString;

use super::Rect2DInst;

static ATTRIBS: [(u32, i32, u32, &str); 7] = [
    (0, 4, gl::FLOAT, "c2v_dst_rect"),
    (1, 4, gl::FLOAT, "c2v_src_rect"),
    (2, 4, gl::FLOAT, "c2v_color_0"),
    (3, 4, gl::FLOAT, "c2v_color_1"),
    (4, 4, gl::FLOAT, "c2v_color_2"),
    (5, 4, gl::FLOAT, "c2v_color_3"),
    (6, 4, gl::FLOAT, "c2v_extra"),
];

static ATTRIBS_OUT: [(u32, &str); 1] = [(0, "final_color")];

static RECT_VERTEX_SHADER: &'static str = include_str!("./vertex.glsl");
static RECT_FRAGMENT_SHADER: &'static str = include_str!("./fragment.glsl");

pub struct GLContext {
    width: f32,
    height: f32,
    ctx: os_impl::GLContextHandle,
    program: u32,
    vao: u32,
    vbo: u32,
    font_texture: u32,
}

impl GLContext {
    pub fn new(win: &Window) -> Self {
        let ctx = os_impl::ogl_create_context(win);
        // SAFETY(xarkes): The pointers come back from OpenGL library.
        // We also assume the GetString function was resolved earlier, if not it will simply result in a null deref.
        unsafe {
            let vendor = gl::GetString(gl::VENDOR) as os_impl::GLStringPtr;
            let version = gl::GetString(gl::VERSION) as os_impl::GLStringPtr;
            if vendor != std::ptr::null_mut() && version != std::ptr::null_mut() {
                let vendorstr = std::ffi::CStr::from_ptr(vendor).to_str().expect("<err>");
                let versionstr = std::ffi::CStr::from_ptr(version).to_str().expect("<err>");
                println!("OpenGL vendor: {} - version: {}", vendorstr, versionstr);
            } else {
                println!("Could not retrieve OpenGL vendor and version!");
            }
        }

        let vs = compile_shader(RECT_VERTEX_SHADER, gl::VERTEX_SHADER);
        let fs = compile_shader(RECT_FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let program = link_program(vs, fs);

        let mut vao = 0;
        let mut vbo = 0;

        unsafe {
            // xarkes: enable blending for our text textures
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            // xarkes: create Vertex Array Object
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);

            // xarkes: create a Vertex Buffer Object and copy the vertex data to it
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                64 * 1024,
                std::ptr::null(),
                gl::DYNAMIC_DRAW,
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            // gl::ClearColor(0.07, 0.31, 0.26, 0.);
            gl::ClearColor(0., 0., 0., 0.);

            // xarkes: disable VSync
            os_impl::ogl_toggle_vsync(&ctx, false);
        }

        // xarkes: enable gl debugging
        #[cfg(not(target_os = "macos"))]
        unsafe {
            gl::Enable(gl::DEBUG_OUTPUT);
            gl::DebugMessageCallback(Some(gl_debug_func), std::ptr::null());
        }

        let (width, height) = win.get_size();

        GLContext {
            width,
            height,
            ctx,
            program,
            vao,
            vbo,
            font_texture: u32::MAX,
        }
    }

    pub fn update_font_texture(&mut self, atlas: &crate::render::font_cache::Atlas) {
        if self.font_texture != u32::MAX {
            // XXX(xarkes): If the font texture already exist, is it fine to just reallocate it?
            // Does OpenGL handle it or should we do things differently
        }

        unsafe { gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1) };

        // xarkes: Create texture for font
        let mut font_texture = u32::MAX;
        unsafe {
            gl::GenTextures(1, &mut font_texture);
            gl::BindTexture(gl::TEXTURE_2D, font_texture);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RED as i32,
                atlas.width as i32,
                atlas.height as i32,
                0,
                gl::RED,
                gl::UNSIGNED_BYTE,
                atlas.data.as_ptr() as *const _,
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
        }
        self.font_texture = font_texture;
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        self.width = w;
        self.height = h;
        os_impl::ogl_resize(&self.ctx);
    }

    pub fn begin_frame(&mut self) {
        unsafe {
            // Begin frame
            let mut width = self.width;
            let mut height = self.height;
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::Uniform2f(
                gl::GetUniformLocation(
                    self.program,
                    CString::new("u_viewport_size_px").unwrap().as_ptr(),
                ),
                width,
                height,
            );
            // TODO(xarkes): We need a proper way of detecting dpi and scaling later
            let highdpi = false;
            if highdpi {
                // TODO(xarkes): For now it works to only upgrade the viewport and keep the uniform scaling right in the shader, but is it okay in terms of resolution, will we have scaling artifacts, blurs, ..?
                width *= 2.0;
                height *= 2.0;
            }
            gl::Viewport(0, 0, width as i32, height as i32);
            gl::UseProgram(self.program);
        }
    }

    pub fn end_frame(&mut self) {
        os_impl::ogl_swapbuffers(&self.ctx);
    }

    pub fn render(&self, batches: &Vec<super::RenderBatch>) {
        // xarkes: draw one rectangle batch group
        for batch in batches.iter() {
            unsafe {
                gl::BindVertexArray(self.vao);
                gl::ActiveTexture(gl::TEXTURE0);
                gl::BindTexture(gl::TEXTURE_2D, self.font_texture);
                gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            }

            // xarkes: fill vertex buffer
            let mut off = 0isize;
            unsafe {
                gl::BufferSubData(
                    gl::ARRAY_BUFFER,
                    off,
                    batch.bytes_count,
                    batch.data.as_ptr() as *const _,
                );
            }
            off += batch.bytes_count;
            if off > 64 * 1024 {
                // TODO(xarkes): We will likely need bigger buffers at some point
                println!("WARNING: Buffer is too small! Handle this!");
            }

            // xarkes: bind input attributes
            let mut atoff = 0usize;
            for attr in ATTRIBS {
                unsafe {
                    gl::EnableVertexAttribArray(attr.0);
                    gl::VertexAttribDivisor(attr.0, 1);
                    gl::VertexAttribPointer(
                        attr.0,
                        attr.1,
                        attr.2,
                        gl::FALSE,
                        std::mem::size_of::<Rect2DInst>() as i32,
                        atoff as *const _,
                    );
                    atoff += attr.1 as usize * std::mem::size_of::<f32>();
                }
            }

            // xarkes: draw buffer
            unsafe {
                // NOTE(xarkes): I wonder if in terms of performances this is similar to storing the triangles directly in memory and calling DrawArrays only once.
                let inst_count = (off as usize / std::mem::size_of::<Rect2DInst>()) as i32;
                gl::DrawArraysInstanced(gl::TRIANGLE_STRIP, 0, 4, inst_count);
            }

            // xarkes: unbind data
            unsafe {
                gl::BindVertexArray(0);
                gl::BindTexture(gl::TEXTURE_2D, 0);
                gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            }
        }
    }
}

fn compile_shader(src: &str, ty: GLenum) -> GLuint {
    let shader;
    unsafe {
        shader = gl::CreateShader(ty);
        if shader == 0 {
            panic!("Shader allocation failed!");
        }
        // Attempt to compile the shader
        let c_str = std::ffi::CString::new(src.as_bytes()).unwrap();
        gl::ShaderSource(shader, 1, &c_str.as_ptr(), std::ptr::null());
        gl::CompileShader(shader);
        // Get the compile status
        let mut status = gl::FALSE as GLint;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);
        // Fail on error
        if status != (gl::TRUE as GLint) {
            let mut buf = Vec::with_capacity(4096);
            gl::GetShaderInfoLog(
                shader,
                buf.len() as i32,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut GLchar,
            );
            panic!(
                "Shader compilation error:\n'{}'\n------------------\n'{}'",
                src,
                std::str::from_utf8(&buf)
                    .ok()
                    .expect("ShaderInfoLog not valid utf8")
            );
        }
    }
    shader
}

fn link_program(vs: GLuint, fs: GLuint) -> GLuint {
    unsafe {
        let program = gl::CreateProgram();
        gl::AttachShader(program, vs);
        gl::AttachShader(program, fs);

        // xarkes: bind vertex input variables
        for attr in ATTRIBS {
            let c_str = std::ffi::CString::new(attr.3.as_bytes()).unwrap();
            gl::BindAttribLocation(program, attr.0, c_str.as_ptr());
        }

        // xarkes: explicitely bind output variables
        for attr in ATTRIBS_OUT {
            let c_str = std::ffi::CString::new(attr.1.as_bytes()).unwrap();
            gl::BindFragDataLocation(program, attr.0, c_str.as_ptr());
        }

        gl::LinkProgram(program);
        gl::ValidateProgram(program);

        // xarkes: verify link status
        let mut status = gl::FALSE as GLint;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);

        // xarkes: fail on error
        if status != (gl::TRUE as GLint) {
            let mut len: GLint = 0;
            gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf = Vec::with_capacity(len as usize);
            buf.set_len((len as usize) - 1); // subtract 1 to skip the trailing null character
            gl::GetProgramInfoLog(
                program,
                len,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut GLchar,
            );
            panic!(
                "{}",
                std::str::from_utf8(&buf)
                    .ok()
                    .expect("ProgramInfoLog not valid utf8")
            );
        }
        program
    }
}

#[cfg(not(target_os = "macos"))]
use gl::types::GLsizei;
#[cfg(not(target_os = "macos"))]
extern "system" fn gl_debug_func(
    src: GLenum,
    _type: GLenum,
    _id: GLuint,
    _severity: GLenum,
    _length: GLsizei,
    message: *const GLchar,
    _userparam: *mut std::ffi::c_void,
) {
    let decoded;
    unsafe {
        decoded = std::ffi::CStr::from_ptr(message as GLStringPtr)
            .to_str()
            .expect("<decode error>");
    }
    println!("Opengl error: {} {}", src, decoded)
}
