#[cfg(target_os = "macos")]
include!("opengl_macos.rs");

#[cfg(all(not(target_os = "macos")))]
compile_error!("OpenGL not implemented for target OS!");

extern crate gl;
use super::RenderCommand;
use crate::os::Window;
use gl::types::*;
use std::ffi::CString;

static RECT_VERTEX_SHADER: &'static str = "
#version 330 core

in vec4 vertex; // <vec2 pos, vec2 tex>

uniform vec2 u_viewport_size_px;

out vec2 tex_coords;

void main() {
  // xarkes: convert position from pixels coords (with top left 0,0) to gl viewport
  gl_Position = vec4(2 * vertex.x / u_viewport_size_px.x - 1, 2 * (1 - vertex.y / u_viewport_size_px.y) - 1, 0.0, 1.0);
  tex_coords = vertex.zw;
}
";

static RECT_FRAGMENT_SHADER: &'static str = "
#version 330 core

in vec2 tex_coords;

uniform sampler2D text;
uniform vec3 u_color;

out vec4 color;

void main()
{
  vec4 sampled = vec4(1.0, 1.0, 1.0, texture(text, tex_coords).r);
  color = vec4(u_color.xyz, 1.0f) * sampled;
}
";

pub struct GLContext {
    width: f32,
    height: f32,
    ctx: GLContextHandle,
    program: u32,
    vao: u32,
    vbo: u32,
    font_texture: u32,
}

impl GLContext {
    pub fn new(win: &Window) -> Self {
        let ctx = ogl_os_create_context(win);

        let vs = compile_shader(RECT_VERTEX_SHADER, gl::VERTEX_SHADER);
        let fs = compile_shader(RECT_FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let program = link_program(vs, fs);

        let mut vao = 0;
        let mut vbo = 0;

        unsafe {
            // Enable blending for our text textures
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            // Create Vertex Array Object
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);

            // Create a Vertex Buffer Object and copy the vertex data to it
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (size_of::<GLfloat>() * 6 * 4) as GLsizeiptr,
                std::ptr::null(),
                gl::DYNAMIC_DRAW,
            );

            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(
                0,
                4,
                gl::FLOAT,
                gl::FALSE,
                (4 as usize * size_of::<GLfloat>()) as i32,
                std::ptr::null(),
            );

            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            gl::ClearColor(0.07, 0.31, 0.26, 0.);
            // gl::ClearColor(0., 0., 0., 0.);
        }

        // enable gl debugging
        // XXX: Not available on macos
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
        ogl_os_resize(self.ctx);
    }

    pub fn begin_frame(&mut self) {
        unsafe {
            // Begin frame
            let mut width = self.width;
            let mut height = self.height;
            if false {
                // TODO: It seems it could be window size x2 on MacOS default settings.
                // I think this could be related to the way it handles DPI or similar.
                width *= 2.0;
                height *= 2.0;
            }
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::Uniform2f(
                gl::GetUniformLocation(
                    self.program,
                    CString::new("u_viewport_size_px").unwrap().as_ptr(),
                ),
                width,
                height,
            );
            gl::Viewport(0, 0, width as i32, height as i32);
            gl::UseProgram(self.program);
        }
    }

    pub fn end_frame(&mut self) {
        ogl_os_swapbuffers(self.ctx);
    }

    pub fn render_rect(&self, cmd: &RenderCommand) {
        unsafe {
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindVertexArray(self.vao);
            gl::BindTexture(gl::TEXTURE_2D, self.font_texture);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                (cmd.data.len() * std::mem::size_of::<GLfloat>()) as GLsizeiptr,
                cmd.data.as_ptr() as *const _,
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            if false {
                // xarkes: Display triangles bounds
                gl::Uniform3f(
                    gl::GetUniformLocation(self.program, CString::new("u_color").unwrap().as_ptr()),
                    1.0,
                    0.4,
                    0.4,
                );
                gl::Disable(gl::BLEND);
                gl::PolygonMode(gl::FRONT_AND_BACK, gl::LINE);
                gl::DrawArrays(gl::TRIANGLES, 0, 6);
            }
            gl::Uniform3f(
                gl::GetUniformLocation(self.program, CString::new("u_color").unwrap().as_ptr()),
                1.0,
                1.0,
                1.0,
            );
            gl::Enable(gl::BLEND);
            gl::PolygonMode(gl::FRONT_AND_BACK, gl::FILL);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
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
        gl::LinkProgram(program);
        // Get the link status
        let mut status = gl::FALSE as GLint;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);

        // Fail on error
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
        decoded = std::ffi::CStr::from_ptr(message as *mut i8)
            .to_str()
            .expect("<decode error>");
    }
    println!("Opengl error: {} {}", src, decoded)
}
