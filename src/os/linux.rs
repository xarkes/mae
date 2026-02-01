extern crate x11;

use super::{OSCursor, OSEvent, OSEventFlag, OSEventType, OSKey, OSKeyCode, Point};
use x11::xlib::{XIMPreeditNothing, XIMStatusNothing, XNInputStyle};

pub struct Window {
    size: (f32, f32),
    pub display: *mut x11::xlib::Display,
    pub win: u64,
    xic: x11::xlib::XIC,
    pub dpi: f32,
    current_cursor: OSCursor,
}

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
                | x11::xlib::ButtonReleaseMask
                | x11::xlib::KeyPressMask
                | x11::xlib::KeyReleaseMask,
        );

        (display, win)
    }
}

impl Window {
    pub fn new(width: u32, height: u32) -> Self {
        let (display, win) = create_window(width, height);
        unsafe {
            let xim = x11::xlib::XOpenIM(
                display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            let input_style = std::ffi::CString::new(XNInputStyle).unwrap();
            let xic = x11::xlib::XCreateIC(
                xim,
                input_style.as_ptr(),
                XIMPreeditNothing | XIMStatusNothing,
                0,
            );
            Window {
                size: (width as f32, height as f32),
                display,
                win,
                xic,
                dpi: 1.0, // TODO(xarkes): Do that better
                current_cursor: OSCursor::Arrow,
            }
        }
    }

    pub fn get_size(&self) -> (f32, f32) {
        self.size
    }

    // TODO(xarkes): This API should probably be generic
    pub fn get_render_size(&self) -> (f32, f32) {
        // TODO -> compute dpi from screen
        // (self.size.0 * self.dpi, self.size.1 * self.dpi)
        (self.size.0, self.size.1)
    }

    /// Set the mouse cursor shape
    pub fn set_cursor(&mut self, cursor: OSCursor) {
        if self.current_cursor == cursor {
            return;
        }
        self.current_cursor = cursor;

        // X11 cursor font glyph indices
        let cursor_shape = match cursor {
            OSCursor::Arrow => 68,    // XC_left_ptr
            OSCursor::IBeam => 152,   // XC_xterm
            OSCursor::Hand => 60,     // XC_hand2
            OSCursor::ResizeH => 108, // XC_sb_h_double_arrow
            OSCursor::ResizeV => 116, // XC_sb_v_double_arrow
        };

        unsafe {
            let cursor_font = x11::xlib::XCreateFontCursor(self.display, cursor_shape);
            x11::xlib::XDefineCursor(self.display, self.win, cursor_font);
            x11::xlib::XFreeCursor(self.display, cursor_font);
        }
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
                            pos: Some(Point::new(event.motion.x as f32, event.motion.y as f32)),
                            chars: None,
                            delta: 0.0,
                            flags: None,
                        };
                    }
                    x11::xlib::ButtonPress => {
                        ev = OSEvent {
                            ty: OSEventType::Press,
                            key: match event.button.button {
                                1 => OSKey::LeftMouseButton,
                                _ => {
                                    println!("Unhandled mouse press!");
                                    OSKey::LeftMouseButton
                                }
                            },
                            pos: Some(Point::new(event.button.x as f32, event.button.y as f32)),
                            chars: None,
                            delta: 0.0,
                            flags: None,
                        };
                    }
                    x11::xlib::ButtonRelease => {
                        ev = OSEvent {
                            ty: OSEventType::Release,
                            key: match event.button.button {
                                1 => OSKey::LeftMouseButton,
                                _ => {
                                    println!("Unhandled mouse press!");
                                    OSKey::LeftMouseButton
                                }
                            },
                            pos: Some(Point::new(event.button.x as f32, event.button.y as f32)),
                            chars: None,
                            delta: 0.0,
                            flags: None,
                        };
                    }
                    x11::xlib::KeyPress => {
                        let mut buffer = vec![0u8, 0, 0, 0];
                        let mut ignore = 0u64;
                        let mut return_status = 0i32;
                        x11::xlib::Xutf8LookupString(
                            self.xic,
                            &mut event.key as *mut x11::xlib::XKeyEvent,
                            buffer.as_ptr() as *mut i8,
                            buffer.len() as i32,
                            &mut ignore as *mut u64,
                            &mut return_status as *mut i32,
                        );
                        let chars_str = std::str::from_utf8(buffer.as_slice()).unwrap();
                        let first_char = chars_str.chars().next().filter(|c| *c != '\0');
                        let ks =
                            x11::xlib::XKeycodeToKeysym(self.display, event.key.keycode as u8, 0)
                                as u32;
                        let flags = x11_state_to_flags(event.key.state);
                        ev = OSEvent {
                            ty: OSEventType::Press,
                            key: x11_keysym_to_oskey(ks),
                            pos: Some(Point::new(event.key.x as f32, event.key.y as f32)),
                            chars: first_char,
                            delta: 0.0,
                            flags,
                        };
                    }
                    x11::xlib::KeyRelease => {
                        let ks =
                            x11::xlib::XKeycodeToKeysym(self.display, event.key.keycode as u8, 0)
                                as u32;
                        ev = OSEvent {
                            ty: OSEventType::Release,
                            key: x11_keysym_to_oskey(ks),
                            pos: Some(Point::new(event.key.x as f32, event.key.y as f32)),
                            chars: None,
                            delta: 0.0,
                            flags: None,
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
    // return nanoseconds per second (must match timer_value units)
    1_000_000_000.
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

/// Convert X11 key state (modifier mask) to OSEventFlag
fn x11_state_to_flags(state: u32) -> Option<OSEventFlag> {
    // X11 modifier masks
    const SHIFT_MASK: u32 = 1 << 0;   // ShiftMask
    const CONTROL_MASK: u32 = 1 << 2; // ControlMask
    const MOD1_MASK: u32 = 1 << 3;    // Mod1Mask (Alt)

    let ctrl = (state & CONTROL_MASK) != 0;
    let alt = (state & MOD1_MASK) != 0;
    let shift = (state & SHIFT_MASK) != 0;

    match (ctrl, alt, shift) {
        (true, true, true) => Some(OSEventFlag::ControlAltShift),
        (true, true, false) => Some(OSEventFlag::ControlAlt),
        (true, false, true) => Some(OSEventFlag::ControlShift),
        (true, false, false) => Some(OSEventFlag::Control),
        (false, true, true) => Some(OSEventFlag::AltShift),
        (false, true, false) => Some(OSEventFlag::Alt),
        (false, false, true) => Some(OSEventFlag::Shift),
        (false, false, false) => None,
    }
}

fn x11_keysym_to_oskey(keysym: u32) -> OSKey {
    match keysym {
        x11::keysym::XK_Home => OSKey::Keyboard(OSKeyCode::KeyHome),
        x11::keysym::XK_Left => OSKey::Keyboard(OSKeyCode::KeyLeftArrow),
        x11::keysym::XK_Up => OSKey::Keyboard(OSKeyCode::KeyUpArrow),
        x11::keysym::XK_Right => OSKey::Keyboard(OSKeyCode::KeyRightArrow),
        x11::keysym::XK_Down => OSKey::Keyboard(OSKeyCode::KeyDownArrow),
        // x11::keysym::XK_Prior => OSKey::Keyboard(OSKeyCode::KeyPrior),
        x11::keysym::XK_Page_Up => OSKey::Keyboard(OSKeyCode::KeyPageUp),
        // x11::keysym::XK_Next => OSKey::Keyboard(OSKeyCode::),
        x11::keysym::XK_Page_Down => OSKey::Keyboard(OSKeyCode::KeyPageDown),
        x11::keysym::XK_End => OSKey::Keyboard(OSKeyCode::KeyEnd),
        // x11::keysym::XK_Begin => OSKey::Keyboard(OSKeyCode::KeyBegin),
        x11::keysym::XK_space => OSKey::Keyboard(OSKeyCode::KeySpace),

        // x11::keysym::XK_exclam => OSKey::Keyboard(OSKeyCode::Key1),
        // x11::keysym::XK_quotedbl => OSKey::Keyboard(OSKeyCode::Key2),
        // x11::keysym::XK_numbersign => OSKey::Keyboard(OSKeyCode::Key3),
        // x11::keysym::XK_dollar => OSKey::Keyboard(OSKeyCode::Key4),
        // x11::keysym::XK_percent => OSKey::Keyboard(OSKeyCode::Key5),
        // x11::keysym::XK_ampersand => OSKey::Keyboard(OSKeyCode::Key6),
        x11::keysym::XK_apostrophe => OSKey::Keyboard(OSKeyCode::KeyApostrophe),
        x11::keysym::XK_quoteright => OSKey::Keyboard(OSKeyCode::KeyApostrophe),
        // x11::keysym::XK_parenleft => OSKey::Keyboard(OSKeyCode::Key9),
        // x11::keysym::XK_parenright => OSKey::Keyboard(OSKeyCode::Key0),
        // x11::keysym::XK_asterisk => OSKey::Keyboard(OSKeyCode::),
        // x11::keysym::XK_plus => OSKey::Keyboard(OSKeyCode::KeyEqual),
        x11::keysym::XK_comma => OSKey::Keyboard(OSKeyCode::KeyComma),
        x11::keysym::XK_minus => OSKey::Keyboard(OSKeyCode::KeyMinus),
        x11::keysym::XK_period => OSKey::Keyboard(OSKeyCode::KeyPeriod),
        x11::keysym::XK_slash => OSKey::Keyboard(OSKeyCode::KeySlash),
        x11::keysym::XK_0 => OSKey::Keyboard(OSKeyCode::Key0),
        x11::keysym::XK_1 => OSKey::Keyboard(OSKeyCode::Key1),
        x11::keysym::XK_2 => OSKey::Keyboard(OSKeyCode::Key2),
        x11::keysym::XK_3 => OSKey::Keyboard(OSKeyCode::Key3),
        x11::keysym::XK_4 => OSKey::Keyboard(OSKeyCode::Key4),
        x11::keysym::XK_5 => OSKey::Keyboard(OSKeyCode::Key5),
        x11::keysym::XK_6 => OSKey::Keyboard(OSKeyCode::Key6),
        x11::keysym::XK_7 => OSKey::Keyboard(OSKeyCode::Key7),
        x11::keysym::XK_8 => OSKey::Keyboard(OSKeyCode::Key8),
        x11::keysym::XK_9 => OSKey::Keyboard(OSKeyCode::Key9),
        x11::keysym::XK_colon => OSKey::Keyboard(OSKeyCode::KeySemicolon),
        x11::keysym::XK_semicolon => OSKey::Keyboard(OSKeyCode::KeySemicolon),
        x11::keysym::XK_less => OSKey::Keyboard(OSKeyCode::KeyComma),
        x11::keysym::XK_equal => OSKey::Keyboard(OSKeyCode::KeyEqual),
        x11::keysym::XK_greater => OSKey::Keyboard(OSKeyCode::KeyPeriod),
        x11::keysym::XK_question => OSKey::Keyboard(OSKeyCode::KeySlash),
        // x11::keysym::XK_at => OSKey::Keyboard(OSKeyCode::),
        x11::keysym::XK_A => OSKey::Keyboard(OSKeyCode::KeyA),
        x11::keysym::XK_B => OSKey::Keyboard(OSKeyCode::KeyB),
        x11::keysym::XK_C => OSKey::Keyboard(OSKeyCode::KeyC),
        x11::keysym::XK_D => OSKey::Keyboard(OSKeyCode::KeyD),
        x11::keysym::XK_E => OSKey::Keyboard(OSKeyCode::KeyE),
        x11::keysym::XK_F => OSKey::Keyboard(OSKeyCode::KeyF),
        x11::keysym::XK_G => OSKey::Keyboard(OSKeyCode::KeyG),
        x11::keysym::XK_H => OSKey::Keyboard(OSKeyCode::KeyH),
        x11::keysym::XK_I => OSKey::Keyboard(OSKeyCode::KeyI),
        x11::keysym::XK_J => OSKey::Keyboard(OSKeyCode::KeyJ),
        x11::keysym::XK_K => OSKey::Keyboard(OSKeyCode::KeyK),
        x11::keysym::XK_L => OSKey::Keyboard(OSKeyCode::KeyL),
        x11::keysym::XK_M => OSKey::Keyboard(OSKeyCode::KeyM),
        x11::keysym::XK_N => OSKey::Keyboard(OSKeyCode::KeyN),
        x11::keysym::XK_O => OSKey::Keyboard(OSKeyCode::KeyO),
        x11::keysym::XK_P => OSKey::Keyboard(OSKeyCode::KeyP),
        x11::keysym::XK_Q => OSKey::Keyboard(OSKeyCode::KeyQ),
        x11::keysym::XK_R => OSKey::Keyboard(OSKeyCode::KeyR),
        x11::keysym::XK_S => OSKey::Keyboard(OSKeyCode::KeyS),
        x11::keysym::XK_T => OSKey::Keyboard(OSKeyCode::KeyT),
        x11::keysym::XK_U => OSKey::Keyboard(OSKeyCode::KeyU),
        x11::keysym::XK_V => OSKey::Keyboard(OSKeyCode::KeyV),
        x11::keysym::XK_W => OSKey::Keyboard(OSKeyCode::KeyW),
        x11::keysym::XK_X => OSKey::Keyboard(OSKeyCode::KeyX),
        x11::keysym::XK_Y => OSKey::Keyboard(OSKeyCode::KeyY),
        x11::keysym::XK_Z => OSKey::Keyboard(OSKeyCode::KeyZ),
        x11::keysym::XK_bracketleft => OSKey::Keyboard(OSKeyCode::KeyLeftBracket),
        x11::keysym::XK_backslash => OSKey::Keyboard(OSKeyCode::KeyBackslash),
        x11::keysym::XK_bracketright => OSKey::Keyboard(OSKeyCode::KeyRightBracket),
        // x11::keysym::XK_asciicircum => OSKey::Keyboard(OSKeyCode::),
        x11::keysym::XK_underscore => OSKey::Keyboard(OSKeyCode::KeyMinus),
        x11::keysym::XK_grave => OSKey::Keyboard(OSKeyCode::KeyGraveAccent),
        // x11::keysym::XK_quoteleft => OSKey::Keyboard(OSKeyCode::),
        x11::keysym::XK_a => OSKey::Keyboard(OSKeyCode::KeyA),
        x11::keysym::XK_b => OSKey::Keyboard(OSKeyCode::KeyB),
        x11::keysym::XK_c => OSKey::Keyboard(OSKeyCode::KeyC),
        x11::keysym::XK_d => OSKey::Keyboard(OSKeyCode::KeyD),
        x11::keysym::XK_e => OSKey::Keyboard(OSKeyCode::KeyE),
        x11::keysym::XK_f => OSKey::Keyboard(OSKeyCode::KeyF),
        x11::keysym::XK_g => OSKey::Keyboard(OSKeyCode::KeyG),
        x11::keysym::XK_h => OSKey::Keyboard(OSKeyCode::KeyH),
        x11::keysym::XK_i => OSKey::Keyboard(OSKeyCode::KeyI),
        x11::keysym::XK_j => OSKey::Keyboard(OSKeyCode::KeyJ),
        x11::keysym::XK_k => OSKey::Keyboard(OSKeyCode::KeyK),
        x11::keysym::XK_l => OSKey::Keyboard(OSKeyCode::KeyL),
        x11::keysym::XK_m => OSKey::Keyboard(OSKeyCode::KeyM),
        x11::keysym::XK_n => OSKey::Keyboard(OSKeyCode::KeyN),
        x11::keysym::XK_o => OSKey::Keyboard(OSKeyCode::KeyO),
        x11::keysym::XK_p => OSKey::Keyboard(OSKeyCode::KeyP),
        x11::keysym::XK_q => OSKey::Keyboard(OSKeyCode::KeyQ),
        x11::keysym::XK_r => OSKey::Keyboard(OSKeyCode::KeyR),
        x11::keysym::XK_s => OSKey::Keyboard(OSKeyCode::KeyS),
        x11::keysym::XK_t => OSKey::Keyboard(OSKeyCode::KeyT),
        x11::keysym::XK_u => OSKey::Keyboard(OSKeyCode::KeyU),
        x11::keysym::XK_v => OSKey::Keyboard(OSKeyCode::KeyV),
        x11::keysym::XK_w => OSKey::Keyboard(OSKeyCode::KeyW),
        x11::keysym::XK_x => OSKey::Keyboard(OSKeyCode::KeyX),
        x11::keysym::XK_y => OSKey::Keyboard(OSKeyCode::KeyY),
        x11::keysym::XK_z => OSKey::Keyboard(OSKeyCode::KeyZ),
        x11::keysym::XK_braceleft => OSKey::Keyboard(OSKeyCode::KeyLeftBracket),
        x11::keysym::XK_bar => OSKey::Keyboard(OSKeyCode::KeySpace),
        x11::keysym::XK_braceright => OSKey::Keyboard(OSKeyCode::KeyRightBracket),
        // x11::keysym::XK_asciitilde => OSKey::Keyboard(OSKeyCode::),,
        x11::keysym::XK_BackSpace => OSKey::Keyboard(OSKeyCode::KeyBackspace),
        x11::keysym::XK_Tab => OSKey::Keyboard(OSKeyCode::KeyTab),
        x11::keysym::XK_Return => OSKey::Keyboard(OSKeyCode::KeyEnter),
        x11::keysym::XK_Escape => OSKey::Keyboard(OSKeyCode::KeyEscape),
        x11::keysym::XK_Delete => OSKey::Keyboard(OSKeyCode::KeyDelete),

        x11::keysym::XK_Shift_L => OSKey::Keyboard(OSKeyCode::KeyLeftShift),
        x11::keysym::XK_Shift_R => OSKey::Keyboard(OSKeyCode::KeyRightShift),
        x11::keysym::XK_Control_L => OSKey::Keyboard(OSKeyCode::KeyLeftCtrl),
        x11::keysym::XK_Control_R => OSKey::Keyboard(OSKeyCode::KeyRightCtrl),
        x11::keysym::XK_Caps_Lock => OSKey::Keyboard(OSKeyCode::KeyCapsLock),

        _ => {
            println!("Warning: keyboard key not handled: {:?}", keysym);
            OSKey::Keyboard(OSKeyCode::KeyA)
        }
    }
}
