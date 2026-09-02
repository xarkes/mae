use crate::os::Window;
use crate::render::RendererError;
use std::ffi::CString;
use windows::Win32::Graphics::Gdi::{GetDC, HDC};
use windows::Win32::Graphics::OpenGL::{
    ChoosePixelFormat, HGLRC, PFD_DRAW_TO_WINDOW, PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA,
    PIXELFORMATDESCRIPTOR, SetPixelFormat, glFlush, wglCreateContext, wglDeleteContext,
    wglGetCurrentContext, wglGetProcAddress, wglMakeCurrent,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows::core::PCSTR;

pub type GLStringPtr = *const i8;
pub struct GLContextHandle {}

type WglCreateContextAttribsARB =
    unsafe extern "system" fn(hdc: HDC, share: HGLRC, attribs: *const i32) -> HGLRC;

pub fn ogl_create_context(win: &Window) -> Result<GLContextHandle, RendererError> {
    let device_context = unsafe { GetDC(Some(win.handle)) };
    let mut pixel_format_desc = PIXELFORMATDESCRIPTOR::default();
    pixel_format_desc.nSize = std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16;
    pixel_format_desc.nVersion = 1;
    pixel_format_desc.dwFlags = PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL;
    pixel_format_desc.iPixelType = PFD_TYPE_RGBA;
    pixel_format_desc.cColorBits = 32;
    pixel_format_desc.cAlphaBits = 8;
    pixel_format_desc.cDepthBits = 24;
    let pixel_format = unsafe { ChoosePixelFormat(device_context, &pixel_format_desc) };
    if pixel_format == 0 {
        return Err(RendererError::OGLInitFailed(
            "ChoosePixelFormat".to_string(),
        ));
    }

    if unsafe { SetPixelFormat(device_context, pixel_format, &pixel_format_desc) }.is_err() {
        return Err(RendererError::OGLInitFailed("SetPixelFormat".to_string()));
    }

    unsafe {
        // xarkes: create bootstrap context
        let render_context = unsafe { wglCreateContext(device_context) };
        let bootstrap_ctx = render_context.expect("wglCreateContext failed");
        unsafe { wglMakeCurrent(device_context, bootstrap_ctx) }.unwrap();
        let opengl_lib = match unsafe {
            LoadLibraryA(PCSTR::from_raw(
                CString::new("opengl32.dll").unwrap().as_ptr() as *const u8,
            ))
        } {
            Ok(handle) => handle,
            Err(_) => {
                return Err(RendererError::OGLInitFailed(
                    "opengl32.dll not found".to_string(),
                ));
            }
        };

        // xarkes: fetch new functions
        let name = CString::new("wglCreateContextAttribsARB").unwrap();
        let proc = wglGetProcAddress(PCSTR(name.as_ptr() as *const u8))
            .expect("wglCreateContextAttribsARB not found");
        let wglCreateContextAttribsARB: WglCreateContextAttribsARB = std::mem::transmute(proc);

        // xarkes: create real context
        const WGL_CONTEXT_MAJOR_VERSION_ARB: i32 = 0x2091;
        const WGL_CONTEXT_MINOR_VERSION_ARB: i32 = 0x2092;
        let attribs = [
            WGL_CONTEXT_MAJOR_VERSION_ARB,
            3,
            WGL_CONTEXT_MINOR_VERSION_ARB,
            3,
            0,
        ];
        let real_context = wglCreateContextAttribsARB(
            device_context,
            HGLRC(core::ptr::null_mut()),
            attribs.as_ptr(),
        );
        if real_context.0 == core::ptr::null_mut() {
            return Err(RendererError::OGLInitFailed(
                "wglCreateContextAttribsARB".to_string(),
            ));
        }
        wglMakeCurrent(device_context, HGLRC(core::ptr::null_mut())).unwrap();
        wglDeleteContext(bootstrap_ctx).unwrap();
        wglMakeCurrent(device_context, real_context).unwrap();

        // xarkes: load functions
        gl::load_with(|symbol| {
            let symbol = CString::new(symbol).unwrap();
            let addr = match wglGetProcAddress(PCSTR::from_raw(symbol.as_ptr() as *const u8)) {
                Some(addr) => addr as *const std::ffi::c_void,
                None => {
                    match GetProcAddress(opengl_lib, PCSTR::from_raw(symbol.as_ptr() as *const u8))
                    {
                        Some(addr) => addr as *const std::ffi::c_void,
                        None => std::ptr::null(),
                    }
                }
            };
            addr
        });
    }

    Ok(GLContextHandle {})
}

pub fn ogl_resize(ctx: &GLContextHandle) {}

pub fn ogl_swapbuffers(ctx: &GLContextHandle) {
    unsafe { glFlush() };
}
pub fn ogl_toggle_vsync(ctx: &GLContextHandle, enable: bool) {
    // TODO(xarkes):
    // wglSwapIntervalEXT(enable);
}

pub fn ogl_destroy_context(_ctx: &mut GLContextHandle) {
    // TODO(xarkes): Store HDC/HGLRC in GLContextHandle and call wglDeleteContext.
}
