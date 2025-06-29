use crate::os::Window;

pub type GLStringPtr = *const i8;
pub struct GLContextHandle {}

pub fn ogl_os_create_context(win: &Window) -> GLContextHandle {
    panic!("OpenGL is not yet implemented for Windows");
    GLContextHandle {}
}

pub fn ogl_os_resize(ctx: &GLContextHandle) {
    // TODO(xarkes)
}
pub fn ogl_os_swapbuffers(ctx: &GLContextHandle) {
    // TODO(xarkes)
}
pub fn ogl_os_toggle_vsync(ctx: &GLContextHandle, enable: bool) {
    // TODO(xarkes)
}
