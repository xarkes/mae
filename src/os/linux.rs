extern crate x11;

fn create_window(width: u32, height: u32) -> (*mut x11::xlib::Display, u64) {
    unsafe {
        let display = x11::xlib::XOpenDisplay(std::ptr::null());
        if display.is_null() {
            panic!("Could not open X display!");
        }

        let root = x11::xlib::XRootWindow(display, 0);
        // TODO(xarkes): I don't know what colormap is for and if this API is useful
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
            x11::xlib::ExposureMask
                | x11::xlib::PointerMotionMask
                | x11::xlib::ButtonPressMask
                | x11::xlib::ButtonReleaseMask,
        );

        (display, win)
    }
}

impl Window {
    pub fn new(width: u32, height: u32) -> Self {
        let (display, win) = create_window(width, height);
        Window {
            size: (width as f32, height as f32),
            display,
            win,
        }
    }

    pub fn get_size(&self) -> (f32, f32) {
        self.size
    }

    pub fn get_events(&mut self) -> Vec<OSEvent> {
        let mut events = Vec::new();
        unsafe {
            while x11::xlib::XPending(self.display) != 0 {
                let mut event = x11::xlib::XEvent { type_: 0 };
                x11::xlib::XNextEvent(self.display, &mut event as *mut x11::xlib::XEvent);
                let ev: OSEvent;
                match event.type_ {
                    x11::xlib::Expose => {
                        self.size = (event.expose.width as f32, event.expose.height as f32);
                        continue;
                    }
                    x11::xlib::MotionNotify => {
                        ev = OSEvent {
                            ty: OSEventType::MouseMove,
                            key: OSKey::LeftMouseButton,
                            pos: (event.motion.x as f32, event.motion.y as f32),
                        };
                    }
                    x11::xlib::ButtonPress => {
                        ev = OSEvent {
                            ty: OSEventType::Press,
                            key: match event.button.button {
                                1 => OSKey::LeftMouseButton,
                                _ => {
                                    println!("Unhandled mouse press!");
                                    OSKey::Unknown
                                }
                            },
                            pos: (event.button.x as f32, event.button.y as f32),
                        };
                    }
                    x11::xlib::ButtonRelease => {
                        ev = OSEvent {
                            ty: OSEventType::Release,
                            key: match event.button.button {
                                1 => OSKey::LeftMouseButton,
                                _ => {
                                    println!("Unhandled mouse press!");
                                    OSKey::Unknown
                                }
                            },
                            pos: (event.button.x as f32, event.button.y as f32),
                        };
                    }
                    _ => {
                        println!("Unhandled event: {:?}", event);
                        continue;
                    }
                }
                events.push(ev);
            }
        }
        events
    }
}

// XXX(xarkes): Not window related, but lazy to add another file. Rename window_linux to os_linux?
#[repr(C)]
struct Timespec {
    tv_sec: u64,
    tv_nsec: u64,
}
unsafe extern "C" {
    fn clock_gettime(id: u64, timespec: *const std::ffi::c_void);
}
pub fn timer_init() -> f64 {
    // xarkes: no init required with current implem on Linux
    1.
}
pub fn timer_value() -> u64 {
    let CLOCK_MONOTONIC_RAW = 4;
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        clock_gettime(
            CLOCK_MONOTONIC_RAW,
            std::ptr::from_ref(&ts) as *const std::ffi::c_void,
        );
    }
    ts.tv_sec * 1_000_000_000 + ts.tv_nsec
}
