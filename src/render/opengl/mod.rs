#[cfg(target_os = "macos")]
include!("opengl_macos.rs");

#[cfg(all(not(target_os = "macos")))]
compile_error!("OpenGL not implemented for target OS!");

extern crate gl;
use gl::types::*;

use crate::os::Window;

static RECT_VERTEX_SHADER: &'static str = "
#version 330 core
in vec3 aPos;
void main() {
    gl_Position = vec4(aPos.x, aPos.y, aPos.z, 1.0);
}
";

static RECT_FRAGMENT_SHADER: &'static str = "
#version 330 core
out vec4 FragColor;
void main()
{
 FragColor = vec4(1.0f, 0.5f, 0.2f, 1.0f);
}
";

pub struct GLContext {
    ctx: GLContextHandle,
    program: u32,
    vao: u32,
}

impl GLContext {
    pub fn new(win: &Window) -> Self {
        let ctx = ogl_os_create_context(win);

        let vs = compile_shader(RECT_VERTEX_SHADER, gl::VERTEX_SHADER);
        let fs = compile_shader(RECT_FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let program = link_program(vs, fs);

        let mut vao = 0;
        let mut vbo = 0;

        let vertex_data: [GLfloat; 9] = [-0.5, -0.5, 0., 0.5, 0.5, 0., 0., 0.5, 0.];
        // let vertex_data: [GLfloat; 9] = [-1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0];

        unsafe {
            // Create Vertex Array Object
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);

            // Create a Vertex Buffer Object and copy the vertex data to it
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertex_data.len() * std::mem::size_of::<GLfloat>()) as GLsizeiptr,
                std::mem::transmute(&vertex_data[0]),
                gl::STATIC_DRAW,
            );

            gl::UseProgram(program);
            gl::VertexAttribPointer(
                0,
                3,
                gl::FLOAT,
                gl::FALSE,
                (3 as usize * std::mem::size_of::<GLfloat>()) as i32,
                std::ptr::null_mut(),
            );
            gl::EnableVertexAttribArray(0);
            gl::ClearColor(0.17, 0.71, 0.56, 0.);
        }

        // enable gl debugging
        // XXX: Not available on macos
        #[cfg(not(target_os = "macos"))]
        unsafe {
            gl::Enable(gl::DEBUG_OUTPUT);
            gl::DebugMessageCallback(Some(gl_debug_func), std::ptr::null());
        }

        GLContext { ctx, program, vao }
    }

    pub fn update(&self) {
        unsafe {
            // XXX: It seems to be window size x2 on current setup on MacOS, why?
            gl::Viewport(0, 0, 600, 600);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            gl::UseProgram(self.program);
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 3);
            ogl_os_swapbuffers(self.ctx);
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
