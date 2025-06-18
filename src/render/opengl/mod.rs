#[cfg(target_os = "macos")]
include!("opengl_macos.rs");

#[cfg(all(not(target_os = "macos")))]
compile_error!("OpenGL not implemented for target OS!");

extern crate gl;
use gl::types::*;

use crate::os::Window;

static RECT_VERTEX_SHADER: &'static str = "
#version 330 core
in vec4 vertex; // <vec2 pos, vec2 tex>
out vec2 tex_coords;

void main() {
  gl_Position = vec4(vertex.xy, 0.0, 1.0);
  tex_coords = vertex.zw;
}
";

static RECT_FRAGMENT_SHADER: &'static str = "
#version 330 core
in vec2 tex_coords;
out vec4 color;

uniform sampler2D text;

void main()
{
  vec4 sampled = vec4(1.0, 1.0, 1.0, texture(text, tex_coords).r);
  color = vec4(1.0f, 1.0f, 1.0f, 1.0f) * sampled;
}
";

pub struct GLContext {
    width: u32,
    height: u32,
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

            gl::ClearColor(0.17, 0.71, 0.56, 0.);
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

    fn render_triangles(&self) {
        unsafe {
            gl::Disable(gl::BLEND);
            gl::BindVertexArray(self.vao);
            let vertex_data: [GLfloat; 12] =
                [-0.5, -0.5, 0., 0., 0.5, 0.5, 0., 0., 0., 0.5, 0., 0.];
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                (vertex_data.len() * std::mem::size_of::<GLfloat>()) as GLsizeiptr,
                std::mem::transmute(&vertex_data[0]),
            );
            gl::DrawArrays(gl::TRIANGLES, 0, 3);
            let vertex_data: [GLfloat; 12] =
                [-0.8, -0.8, 0., 0., -0.4, -0.7, 0., 0., 0.3, -0.3, 0., 0.];
            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                (vertex_data.len() * std::mem::size_of::<GLfloat>()) as GLsizeiptr,
                std::mem::transmute(&vertex_data[0]),
            );
            gl::DrawArrays(gl::TRIANGLES, 0, 3);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }
    }

    fn render_text(&mut self, font_cache: &mut crate::render::font_cache::FontCache) {
        unsafe {
            gl::Enable(gl::BLEND);
            let text = String::from("ABCDEFGHIJKLMNOPQRSTUVWXYZ");
            let text = String::from("abcdefghijklmnopqrstuvwxyz");
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindVertexArray(self.vao);

            let mut x: f32 = -1.0;
            let y: f32 = 1.0;

            // XXX: This is dumb, but I'm lazy atm. Rewrite this :)
            {
                let mut should_update = false;
                for c in text.chars() {
                    let (_, added) = font_cache.get(c);
                    should_update |= added;
                }
                if should_update {
                    self.update_font_texture(font_cache.atlas());
                }
            }

            for c in text.chars() {
                let (glyph, _) = font_cache.get(c);
                if glyph.is_none() {
                    continue;
                }
                let glyph = glyph.unwrap();

                // TODO(xarkes): It would probably be more readable to have the shader
                // normalize to the window size rather than doing it here

                // Update VBO for each character
                let font_size = 75.0;
                let h = font_size / self.width as f32;
                let w = font_size / self.height as f32;
                let xpos = x;
                let ypos = y - h - (glyph.yoff * font_size / self.height as f32);
                let vbo_data: [GLfloat; 24] = [
                    xpos,
                    ypos + h,
                    glyph.tl_x,
                    glyph.tl_y,
                    xpos,
                    ypos,
                    glyph.tl_x,
                    glyph.br_y,
                    xpos + w,
                    ypos,
                    glyph.br_x,
                    glyph.br_y,
                    xpos,
                    ypos + h,
                    glyph.tl_x,
                    glyph.tl_y,
                    xpos + w,
                    ypos,
                    glyph.br_x,
                    glyph.br_y,
                    xpos + w,
                    ypos + h,
                    glyph.br_x,
                    glyph.tl_y,
                ];
                gl::BindTexture(gl::TEXTURE_2D, self.font_texture);
                gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
                gl::BufferSubData(
                    gl::ARRAY_BUFFER,
                    0,
                    (vbo_data.len() * std::mem::size_of::<GLfloat>()) as GLsizeiptr,
                    vbo_data.as_ptr() as *const _,
                );
                gl::BindBuffer(gl::ARRAY_BUFFER, 0);
                gl::DrawArrays(gl::TRIANGLES, 0, 6);

                x += glyph.xoff * font_size / self.width as f32;
            }
            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        self.width = w;
        self.height = h;
        unsafe {
            gl::Viewport(0, 0, w as i32, h as i32);
        }
    }

    pub fn update(&mut self, font_cache: &mut crate::render::font_cache::FontCache) {
        unsafe {
            let mut width = self.width;
            let mut height = self.height;
            if false {
                // TODO: It seems it could be window size x2 on MacOS default settings.
                // I think this could be related to the way it handles DPI or similar.
                width *= 2;
                height *= 2;
            }
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::Viewport(0, 0, width as i32, height as i32);
            gl::UseProgram(self.program);

            // Draw some triangles
            self.render_triangles();
            // Draw some text
            self.render_text(font_cache);

            ogl_os_swapbuffers(self.ctx);
        }
    }

    pub fn update_font_texture(&mut self, atlas: &crate::render::font_cache::Atlas) {
        if self.font_texture != u32::MAX {
            // XXX: If the font texture already exist, is it fine to just reallocate it?
            // Does OpenGL handle it or should we do things differently
            // println!("FIXME: Not handled atm");
        }

        unsafe { gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1) };

        // Create texture for font
        println!("TEXTURE CREATION ======");
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
