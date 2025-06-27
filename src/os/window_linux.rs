extern crate x11;

fn create_window(width: u32, height: u32) -> (*mut x11::xlib::Display, u64) {
    unsafe {
        let display = x11::xlib::XOpenDisplay(std::ptr::null());
        if display.is_null() {
            panic!("Could not open X display!");
        }

        let root = x11::xlib::XRootWindow(display, 0);
        // let colormap = x11::xlib::XCreateColormap(display, root, core::ptr::null_mut(), 0);
        let mut swa: x11::xlib::XSetWindowAttributes = std::mem::zeroed();
        swa.event_mask = x11::xlib::StructureNotifyMask;
        // swa.colormap = colormap;
        let win = x11::xlib::XCreateWindow(
            display,
            root,
            0,
            0,
            width,
            height,
            0,
            0,
            x11::xlib::InputOutput as u32,
            core::ptr::null_mut(),
            x11::xlib::CWColormap | x11::xlib::CWEventMask,
            &mut swa,
        );
        if win == 0 {
            panic!("XCreateWindow failed!");
        }

        x11::xlib::XMapWindow(display, win);
        x11::xlib::XSelectInput(
            display,
            win,
            // x11::xlib::ExposureMask | x11::xlib::KeyPressMask, // TODO: Handle keyboard
            x11::xlib::ExposureMask,
        );

        (display,win)
    }
}

impl Window {
    pub fn new(width: u32, height: u32) -> Self {
        let (display, win) = create_window(width, height);
        Window {
            display,
            win
        }
    }

    pub fn get_size(&self) -> (f32, f32) {
        (0., 0.)
    }

    pub fn get_events(&self) -> Vec<OSEvent> {
        // TODO(xarkes): implement
        // unsafe {
            // loop {
            //     while x11::xlib::XPending(self.display) != 0 {
            //         let mut event = x11::xlib::XEvent { type_: 0 };
            //         x11::xlib::XNextEvent(self.display, &mut event as *mut x11::xlib::XEvent);
            //         println!("Unhandled event: {:?}", event)
            //     }
            // }
        // }
        Vec::new()
    }
}

// XXX(xarkes): Not window related, but lazy to add another file. Rename window_linux to os_linux?
pub fn timer_init() -> f64 {
    // TODO(xarkes)
    0.
}
pub fn timer_value() -> u64 {
    // TODO(xarkes)
    0
}
