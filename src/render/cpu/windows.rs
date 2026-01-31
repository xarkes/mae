use crate::os::Window;

pub struct CPUContextHandle {
    // TODO: Implement Windows CPU rendering
}

pub fn cpu_create_context(_win: &Window) -> CPUContextHandle {
    todo!("Windows CPU renderer not yet implemented")
}

pub fn cpu_swapbuffers(_ctx: &mut CPUContextHandle, _framebuffer: &[u32], _width: usize, _height: usize) {
    todo!("Windows CPU renderer not yet implemented")
}
