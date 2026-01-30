use crate::os::Window;
use std::ffi::CString;
use windows::Win32::Graphics::Gdi::GetDC;
use windows::Win32::Graphics::OpenGL::{
    ChoosePixelFormat, PFD_DRAW_TO_WINDOW, PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA,
    PIXELFORMATDESCRIPTOR, SetPixelFormat, glFlush, wglCreateContext, wglGetCurrentContext,
    wglGetProcAddress, wglMakeCurrent,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows::core::PCSTR;

pub type GLStringPtr = *const i8;
pub struct GLContextHandle {}

pub fn ogl_create_context(win: &Window) -> GLContextHandle {
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
        panic!("ChoosePixelFormat failed!");
    }

    if unsafe { SetPixelFormat(device_context, pixel_format, &pixel_format_desc) }.is_err() {
        panic!("SetPixelFormat failed!");
    }

    let render_context = unsafe { wglCreateContext(device_context) };
    let render_context = render_context.expect("wglCreateContext failed");
    unsafe { wglMakeCurrent(device_context, render_context) }.unwrap();
    let opengl_lib = unsafe {
        LoadLibraryA(PCSTR::from_raw(
            CString::new("opengl32.dll").unwrap().as_ptr() as *const u8,
        ))
    }
    .expect("opengl32.dll not found!");
    gl::load_with(|symbol| unsafe {
        let symbol = CString::new(symbol).unwrap();
        let addr = match wglGetProcAddress(PCSTR::from_raw(symbol.as_ptr() as *const u8)) {
            Some(addr) => addr as *const std::ffi::c_void,
            None => match GetProcAddress(opengl_lib, PCSTR::from_raw(symbol.as_ptr() as *const u8))
            {
                Some(addr) => addr as *const std::ffi::c_void,
                None => std::ptr::null(),
            },
        };
        addr
    });
    GLContextHandle {}
}

pub fn ogl_resize(ctx: &GLContextHandle) {}

pub fn ogl_swapbuffers(ctx: &GLContextHandle) {
    unsafe { glFlush() };
}
pub fn ogl_toggle_vsync(ctx: &GLContextHandle, enable: bool) {
    // TODO(xarkes)
}
