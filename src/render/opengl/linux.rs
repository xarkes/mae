use crate::os::Window;
use crate::render::RendererError;
use x11::glx;
use x11::glx::__GLXcontextRec;
use x11::xlib;

pub type GLStringPtr = *const i8;

pub struct GLContextHandle {
    ctx: *mut __GLXcontextRec,
    display: *mut x11::xlib::Display,
    win: u64,
}

type GLXSwapIntervalMESA = unsafe extern "C" fn(_0: i32);

pub fn ogl_create_context(win: &Window) -> Result<GLContextHandle, RendererError> {
    let glcontext;
    unsafe {
        // TODO(xarkes): Support native wayland
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
            return Err(RendererError::OGLInitFailed(
                "glXChoosFBConfig failed".to_string(),
            ));
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
        println!("{:?}", vi);

        glcontext = glx::glXCreateContext(win.display, vi, std::ptr::null_mut(), 1);
        if glcontext.is_null() {
            return Err(RendererError::OGLInitFailed(
                "glXCreateContext failed".to_string(),
            ));
        }
        glx::glXMakeCurrent(win.display, win.win, glcontext);
        gl::load_with(|symbol| {
            let symbol = std::ffi::CString::new(symbol).unwrap();
            glx::glXGetProcAddress(symbol.as_ptr() as *const u8).unwrap() as *const std::ffi::c_void
        });
    }
    Ok(GLContextHandle {
        ctx: glcontext,
        display: win.display,
        win: win.win,
    })
}

pub fn ogl_resize(_ctx: &GLContextHandle) {
    // TODO(xarkes): Currently it works without it, not sure if it's a hack
}
pub fn ogl_swapbuffers(ctx: &GLContextHandle) {
    unsafe {
        glx::glXSwapBuffers(ctx.display, ctx.win);
    }
}
pub fn ogl_toggle_vsync(_ctx: &GLContextHandle, enable: bool) {
    let val = match enable {
        true => 1i32,
        false => 0i32,
    };
    unsafe {
        let glx_swap_interval_mesa = glx::glXGetProcAddress(
            std::ffi::CString::new("glXSwapIntervalMESA")
                .unwrap()
                .as_ptr() as *const u8,
        );
        if let Some(func) = glx_swap_interval_mesa {
            let swap_interval_func: GLXSwapIntervalMESA = std::mem::transmute(func);
            swap_interval_func(val);
        } else {
            println!("vsync toggle failure");
        }
    }
    // unsafe { xlib::XSync(ctx.display, val) };
}

pub fn ogl_destroy_context(ctx: &mut GLContextHandle) {
    if ctx.ctx.is_null() {
        return;
    }
    unsafe {
        glx::glXMakeCurrent(ctx.display, 0, std::ptr::null_mut());
        glx::glXDestroyContext(ctx.display, ctx.ctx);
        ctx.ctx = std::ptr::null_mut();
    }
}
