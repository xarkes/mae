extern crate gl;
use gl::types::*;

use x11::glx;
use x11::xlib;

use crate::fui::window::Window;

pub struct GLContext {
    program: u32,
    vao: u32,
    vbo: u32,
}

impl GLContext {
    pub fn render(&self, win: &Window) {
        let vertex_data: [GLfloat; 6] = [0.0, 0.5, 0.5, -0.5, -0.5, -0.5];
        unsafe {
            gl::ClearColor(0.17, 0.71, 0.56, 0.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertex_data.len() * std::mem::size_of::<GLfloat>()) as GLsizeiptr,
                std::mem::transmute(&vertex_data[0]),
                gl::STATIC_DRAW,
            );

            gl::UseProgram(self.program);
            gl::DrawArrays(gl::TRIANGLES, 0, 3);

            draw_text("Hello world");

            glx::glXSwapBuffers(win.display, win.win);
        }
    }
}

// https://github.com/brendanzab/gl-rs/blob/master/gl/examples/triangle.rs

/*
static VS_SRC: &'static str = "
#version 330 core
layout (location = 0) in vec2 Coords;
layout (location = 1) in vec4 InColor;
uniform mat4 ProjMatrix;
out vec4 FragColor;
void main()
{
  FragColor = InColor;
  gl_Position = ProjMatrix * vec4(Coords.xy, 0, 1.0);
}";
//gl_Position = vec4(Coords.xy, 0, 1.0);

static FS_SRC: &'static str = "#version 330 core
in vec4 FragColor;
out vec4 OutFragColor;
void main()
{
  OutFragColor = FragColor;
}";
**/
static VS_SRC: &'static str = "
#version 150
in vec2 position;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
}";

static FS_SRC: &'static str = "
#version 150
out vec4 out_color;

void main() {
    out_color = vec4(1.0, 1.0, 1.0, 1.0);
}";

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

fn draw_text(text: &str) {
    // TODO: Implement me
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

#[cfg(target_os = "linux")]
pub fn create(win: &Window) -> GLContext {
    let glcontext;
    unsafe {
        // XXX: We may be using Wayland
        let screen = x11::xlib::XDefaultScreen(win.display);
        let visual_attribs = vec![
            glx::GLX_X_RENDERABLE,
            1,
            glx::GLX_DRAWABLE_TYPE,
            glx::GLX_WINDOW_BIT,
            glx::GLX_RENDER_TYPE,
            glx::GLX_RGBA_BIT,
            glx::GLX_X_VISUAL_TYPE,
            glx::GLX_TRUE_COLOR,
            glx::GLX_RED_SIZE,
            8,
            glx::GLX_GREEN_SIZE,
            8,
            glx::GLX_BLUE_SIZE,
            8,
            glx::GLX_ALPHA_SIZE,
            8,
            glx::GLX_DEPTH_SIZE,
            24,
            glx::GLX_STENCIL_SIZE,
            8,
            glx::GLX_DOUBLEBUFFER,
            1,
            0,
        ];

        let mut fbcount: i32 = 0;
        let fbconfig =
            glx::glXChooseFBConfig(win.display, screen, visual_attribs.as_ptr(), &mut fbcount);
        if fbconfig.is_null() {
            panic!("Could not get FrameBuffer config!");
        }

        let mut best_fbc = -1;
        let mut worst_fbc = -1;
        let mut best_num_samp = -1;
        let mut worst_num_samp = 9999;

        for i in 0..fbcount {
            let fbi = *fbconfig.add(i as usize);
            let vi = glx::glXGetVisualFromFBConfig(win.display, fbi);
            if !vi.is_null() {
                let mut buf: i32 = 0;
                let mut samples: i32 = 0;
                glx::glXGetFBConfigAttrib(win.display, fbi, glx::GLX_SAMPLE_BUFFERS, &mut buf);
                glx::glXGetFBConfigAttrib(win.display, fbi, glx::GLX_SAMPLES, &mut samples);
                // println!(
                //     "Matching fbconfig {}, visual ID {}: SAMPLE_BUFFERS = {}, SAMPLES = {}",
                //     i,
                //     (*vi).visualid,
                //     buf,
                //     samples
                // );
                if best_fbc < 0 || buf > 0 && samples > best_num_samp {
                    best_fbc = i;
                    best_num_samp = samples;
                }
                if worst_fbc < 0 || buf == 0 || samples < worst_num_samp {
                    worst_fbc = i;
                    worst_num_samp = samples;
                }
            }
            xlib::XFree(vi as *mut std::ffi::c_void);
        }

        let fbc: glx::GLXFBConfig = *fbconfig.add(best_fbc as usize);
        xlib::XFree(*fbconfig as *mut std::ffi::c_void);

        let vi = glx::glXGetVisualFromFBConfig(win.display, fbc);

        glcontext = glx::glXCreateContext(win.display, vi, std::ptr::null_mut(), 1);
        if glcontext.is_null() {
            panic!("GLContext creation failed!");
        }
        glx::glXMakeCurrent(win.display, win.win, glcontext);
        xlib::XSync(win.display, 0);

        gl::load_with(|symbol| {
            let symbol = std::ffi::CString::new(symbol).unwrap();
            glx::glXGetProcAddress(symbol.as_ptr() as *const u8).unwrap() as *const std::ffi::c_void
        });

        let vendor = gl::GetString(gl::VENDOR) as *mut i8;
        let version = gl::GetString(gl::VERSION) as *mut i8;
        let vendorstr = std::ffi::CStr::from_ptr(vendor).to_str().expect("<err>");
        let versionstr = std::ffi::CStr::from_ptr(version).to_str().expect("<err>");
        println!("OpenGL vendor: {} - version: {}", vendorstr, versionstr);
    }

    /////////////////////////
    ///////////////
    unsafe { glx::glXMakeCurrent(win.display, win.win, glcontext) };

    unsafe {
        // enable gl debugging
        gl::Enable(gl::DEBUG_OUTPUT);
        gl::DebugMessageCallback(Some(gl_debug_func), std::ptr::null());
    }

    let vs = compile_shader(VS_SRC, gl::VERTEX_SHADER);
    let fs = compile_shader(FS_SRC, gl::FRAGMENT_SHADER);
    let program = link_program(vs, fs);

    let mut vao = 0;
    let mut vbo = 0;

    let vertex_data: [GLfloat; 6] = [0.0, 0.5, 0.5, -0.5, -0.5, -0.5];

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

        // Use shader program
        gl::UseProgram(program);
        let col = std::ffi::CString::new("out_color").unwrap();
        gl::BindFragDataLocation(program, 0, col.as_ptr());

        // Specify the layout of the vertex data
        let pos = std::ffi::CString::new("position").unwrap();
        let pos_attr = gl::GetAttribLocation(program, pos.as_ptr());
        gl::EnableVertexAttribArray(pos_attr as GLuint);
        gl::VertexAttribPointer(
            pos_attr as GLuint,
            2,
            gl::FLOAT,
            gl::FALSE as GLboolean,
            0,
            std::ptr::null(),
        );
    }

    GLContext { program, vao, vbo }
}
