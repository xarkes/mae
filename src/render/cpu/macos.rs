use crate::os::Window;

pub struct CPUContextHandle {
    // TODO: Implement macOS CPU rendering
}

pub fn cpu_create_context(_win: &Window) -> CPUContextHandle {
    todo!("macOS CPU renderer not yet implemented")
}

pub fn cpu_swapbuffers(
    _ctx: &mut CPUContextHandle,
    _framebuffer: &[u32],
    _width: usize,
    _height: usize,
) {
    todo!("macOS CPU renderer not yet implemented")
}
