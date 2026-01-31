use crate::os::Window;
use x11::xlib;

pub struct CPUContextHandle {
    display: *mut xlib::Display,
    win: u64,
    gc: xlib::GC,
    depth: i32,
    visual: *mut xlib::Visual,
}

pub fn cpu_create_context(win: &Window) -> CPUContextHandle {
    unsafe {
        let screen = xlib::XDefaultScreen(win.display);
        let gc = xlib::XDefaultGC(win.display, screen);
        let depth = xlib::XDefaultDepth(win.display, screen);
        let visual = xlib::XDefaultVisual(win.display, screen);

        CPUContextHandle {
            display: win.display,
            win: win.win,
            gc,
            depth,
            visual,
        }
    }
}

pub fn cpu_swapbuffers(ctx: &mut CPUContextHandle, framebuffer: &[u32], width: usize, height: usize) {
    if width == 0 || height == 0 {
        return;
    }

    unsafe {
        // Create XImage from framebuffer
        let image = xlib::XCreateImage(
            ctx.display,
            ctx.visual,
            ctx.depth as u32,
            xlib::ZPixmap,
            0,
            framebuffer.as_ptr() as *mut i8,
            width as u32,
            height as u32,
            32,
            0,
        );

        if !image.is_null() {
            xlib::XPutImage(
                ctx.display,
                ctx.win,
                ctx.gc,
                image,
                0,
                0,
                0,
                0,
                width as u32,
                height as u32,
            );

            // Don't free the data - it belongs to our framebuffer
            (*image).data = std::ptr::null_mut();
            xlib::XDestroyImage(image);
        }

        xlib::XFlush(ctx.display);
    }
}
