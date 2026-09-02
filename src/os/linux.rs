extern crate x11;

use super::{OSCursor, OSEvent, OSEventFlag, OSEventType, OSKey, OSKeyCode, Point};
use std::{
    os::raw::{c_char, c_int, c_short, c_ulong, c_void},
    sync::{Mutex, OnceLock},
};
use x11::xlib::{XIMPreeditNothing, XIMStatusNothing, XNInputStyle};

const RTLD_LAZY: i32 = 1;
static XRANDR_API: OnceLock<Option<XrandrApi>> = OnceLock::new();

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

pub struct Window {
    size: (f32, f32),
    pub display: *mut x11::xlib::Display,
    pub win: u64,
    xic: x11::xlib::XIC,
    wm_delete_window: x11::xlib::Atom,
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
                | x11::xlib::StructureNotifyMask
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
    // IME (preedit/candidate positioning) is implemented on macOS only for now.
    pub fn ime_preedit(&self) -> Option<String> {
        None
    }

    pub fn set_ime_caret_rect(&self, _x: f32, _y: f32, _width: f32, _height: f32) {}

    pub fn new(width: u32, height: u32, title: &str) -> Self {
        let (display, win) = create_window(width, height);
        let oswindow = unsafe {
            let mut wm_delete_window =
                x11::xlib::XInternAtom(display, c"WM_DELETE_WINDOW".as_ptr(), x11::xlib::False);
            x11::xlib::XSetWMProtocols(display, win, &mut wm_delete_window, 1);

            // Remember the display/window and intern the atoms used to serve and
            // request the CLIPBOARD selection (the one driven by Ctrl+C / Ctrl+V).
            let _ = CLIPBOARD_CTX.set(ClipboardCtx {
                display,
                win,
                clipboard: x11::xlib::XInternAtom(display, c"CLIPBOARD".as_ptr(), x11::xlib::False),
                utf8: x11::xlib::XInternAtom(display, c"UTF8_STRING".as_ptr(), x11::xlib::False),
                targets: x11::xlib::XInternAtom(display, c"TARGETS".as_ptr(), x11::xlib::False),
                prop: x11::xlib::XInternAtom(display, c"ENKR_CLIPBOARD".as_ptr(), x11::xlib::False),
            });

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
                wm_delete_window,
                dpi: 1.0, // TODO(xarkes): Do that better
                current_cursor: OSCursor::Arrow,
            }
        };
        oswindow.set_title(title);
        oswindow
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

    pub fn refresh_rate_hz(&self) -> f32 {
        unsafe {
            let Some(xrandr) = XrandrApi::get() else {
                return 60.0;
            };

            let config = (xrandr.get_screen_info)(self.display, self.win);
            if config.is_null() {
                return 60.0;
            }

            let rate = (xrandr.config_current_rate)(config);
            (xrandr.free_screen_config_info)(config);
            if rate > 0 { rate as f32 } else { 60.0 }
        }
    }

    /// Set the window's title bar text. Not yet implemented on X11.
    pub fn set_title(&self, _title: &str) {
        // !TODO
    }

    /// Set the application/window icon from PNG bytes. Not yet implemented on X11.
    pub fn set_app_icon(&self, _png_bytes: &[u8]) {
        // !TODO
    }

    /// Set the mouse cursor shape
    pub fn set_cursor(&mut self, cursor: OSCursor) {
        if self.current_cursor == cursor {
            return;
        }
        self.current_cursor = cursor;

        // X11 cursor font glyph indices
        let cursor_shape = match cursor {
            OSCursor::Arrow => 68,      // XC_left_ptr
            OSCursor::IBeam => 152,     // XC_xterm
            OSCursor::Hand => 60,       // XC_hand2
            OSCursor::ResizeH => 108,   // XC_sb_h_double_arrow
            OSCursor::ResizeV => 116,   // XC_sb_v_double_arrow
            OSCursor::ResizeNWSE => 14, // XC_bottom_right_corner
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
                        // Coalesce the burst of Expose events: only redraw once,
                        // on the last one (count == 0).
                        if event.expose.count != 0 {
                            continue;
                        }
                        ev = OSEvent {
                            ty: OSEventType::Repaint,
                            key: OSKey::LeftMouseButton,
                            pos: None,
                            chars: None,
                            deltax: 0.0,
                            deltay: 0.0,
                            flags: None,
                        };
                    }
                    x11::xlib::ConfigureNotify => {
                        let w = event.configure.width as f32;
                        let h = event.configure.height as f32;
                        // ConfigureNotify also fires on window moves; ignore
                        // anything that isn't an actual size change.
                        if (w, h) == self.size {
                            continue;
                        }
                        self.size = (w, h);
                        ev = OSEvent {
                            ty: OSEventType::Resize,
                            key: OSKey::LeftMouseButton,
                            pos: None,
                            chars: None,
                            deltax: w,
                            deltay: h,
                            flags: None,
                        };
                    }
                    x11::xlib::ReparentNotify => {
                        continue;
                    }
                    x11::xlib::MapNotify => {
                        continue;
                    }
                    x11::xlib::MotionNotify => {
                        ev = OSEvent {
                            ty: OSEventType::MouseMove,
                            key: OSKey::LeftMouseButton,
                            pos: Some(Point::new(event.motion.x as f32, event.motion.y as f32)),
                            chars: None,
                            deltax: 0.0,
                            deltay: 0.0,
                            flags: None,
                        };
                    }
                    x11::xlib::ButtonPress => {
                        let pos = Point::new(event.button.x as f32, event.button.y as f32);
                        if let Some(delta) = x11_scroll_delta(event.button.button) {
                            ev = OSEvent::scroll_with_flags(
                                pos,
                                delta,
                                x11_state_to_flags(event.button.state),
                            );
                        } else if let Some(key) = x11_button_to_oskey(event.button.button) {
                            ev = OSEvent::press(key, Some(pos));
                        } else {
                            println!("Unhandled mouse press!");
                            continue;
                        }
                    }
                    x11::xlib::ButtonRelease => {
                        let pos = Point::new(event.button.x as f32, event.button.y as f32);
                        if x11_scroll_delta(event.button.button).is_some() {
                            continue;
                        } else if let Some(key) = x11_button_to_oskey(event.button.button) {
                            ev = OSEvent::release(key, Some(pos));
                        } else {
                            println!("Unhandled mouse release!");
                            continue;
                        }
                    }
                    x11::xlib::KeyPress => {
                        let buffer = vec![0u8, 0, 0, 0];
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
                            deltax: 0.0,
                            deltay: 0.0,
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
                            deltax: 0.0,
                            deltay: 0.0,
                            flags: None,
                        };
                    }
                    x11::xlib::ClientMessage => {
                        if event.client_message.data.get_long(0) as u64 == self.wm_delete_window {
                            ev = OSEvent::quit();
                        } else {
                            continue;
                        }
                    }
                    x11::xlib::SelectionRequest => {
                        // Another app is pasting our clipboard contents: hand them over.
                        handle_selection_request(&event.selection_request);
                        continue;
                    }
                    x11::xlib::SelectionClear => {
                        // We lost ownership of the clipboard; drop our stale copy.
                        CLIPBOARD_TEXT.lock().unwrap().clear();
                        continue;
                    }
                    x11::xlib::SelectionNotify => {
                        // Replies are consumed synchronously in `clipboard_get`.
                        continue;
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

#[derive(Clone, Copy)]
struct XrandrApi {
    get_screen_info:
        unsafe extern "C" fn(*mut x11::xlib::Display, x11::xlib::Window) -> *mut c_void,
    config_current_rate: unsafe extern "C" fn(*mut c_void) -> c_short,
    free_screen_config_info: unsafe extern "C" fn(*mut c_void),
}

impl XrandrApi {
    fn get() -> Option<&'static Self> {
        XRANDR_API.get_or_init(|| unsafe { Self::load() }).as_ref()
    }

    unsafe fn load() -> Option<Self> {
        let handle = unsafe { dlopen(c"libXrandr.so.2".as_ptr(), RTLD_LAZY) };
        if handle.is_null() {
            return None;
        }

        let get_screen_info = unsafe { dlsym(handle, c"XRRGetScreenInfo".as_ptr()) };
        let config_current_rate = unsafe { dlsym(handle, c"XRRConfigCurrentRate".as_ptr()) };
        let free_screen_config_info = unsafe { dlsym(handle, c"XRRFreeScreenConfigInfo".as_ptr()) };
        if get_screen_info.is_null()
            || config_current_rate.is_null()
            || free_screen_config_info.is_null()
        {
            return None;
        }

        Some(Self {
            get_screen_info: unsafe { std::mem::transmute(get_screen_info) },
            config_current_rate: unsafe { std::mem::transmute(config_current_rate) },
            free_screen_config_info: unsafe { std::mem::transmute(free_screen_config_info) },
        })
    }
}

fn x11_button_to_oskey(button: u32) -> Option<OSKey> {
    match button {
        1 => Some(OSKey::LeftMouseButton),
        3 => Some(OSKey::RightMouseButton),
        _ => None,
    }
}

fn x11_scroll_delta(button: u32) -> Option<f32> {
    match button {
        // X11 reports vertical wheel movement as virtual buttons:
        // 4 is wheel up, 5 is wheel down.
        4 => Some(1.0),
        5 => Some(-1.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_vertical_wheel_buttons_map_to_scroll_delta() {
        assert_eq!(x11_scroll_delta(4), Some(1.0));
        assert_eq!(x11_scroll_delta(5), Some(-1.0));
        assert_eq!(x11_scroll_delta(1), None);
    }

    #[test]
    fn x11_pointer_buttons_map_to_mouse_keys() {
        assert_eq!(x11_button_to_oskey(1), Some(OSKey::LeftMouseButton));
        assert_eq!(x11_button_to_oskey(3), Some(OSKey::RightMouseButton));
        assert_eq!(x11_button_to_oskey(4), None);
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
    const CLOCK_MONOTONIC_RAW: u64 = 4;
    let ts = Timespec {
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

/// Display, window and atoms needed to serve and request the X11 CLIPBOARD selection.
struct ClipboardCtx {
    display: *mut x11::xlib::Display,
    win: x11::xlib::Window,
    clipboard: x11::xlib::Atom,
    utf8: x11::xlib::Atom,
    targets: x11::xlib::Atom,
    prop: x11::xlib::Atom,
}
// The context is only ever touched from the single UI thread (window creation and
// event handling), so sharing the raw display pointer through a static is sound.
unsafe impl Send for ClipboardCtx {}
unsafe impl Sync for ClipboardCtx {}

static CLIPBOARD_CTX: OnceLock<ClipboardCtx> = OnceLock::new();
/// Our copy of the clipboard text, served to other apps while we own the selection.
static CLIPBOARD_TEXT: Mutex<String> = Mutex::new(String::new());

/// Take ownership of the CLIPBOARD selection and remember `text` to hand out on request.
pub fn clipboard_set(text: &str) {
    let Some(ctx) = CLIPBOARD_CTX.get() else {
        return;
    };
    *CLIPBOARD_TEXT.lock().unwrap() = text.to_string();
    unsafe {
        x11::xlib::XSetSelectionOwner(ctx.display, ctx.clipboard, ctx.win, x11::xlib::CurrentTime);
        x11::xlib::XFlush(ctx.display);
    }
}

/// Read the CLIPBOARD selection as UTF-8, asking the current owner to convert it.
pub fn clipboard_get() -> Option<String> {
    let ctx = CLIPBOARD_CTX.get()?;
    unsafe {
        let owner = x11::xlib::XGetSelectionOwner(ctx.display, ctx.clipboard);
        if owner == 0 {
            return None;
        }
        if owner == ctx.win {
            // We own it; answering our own request over X11 would deadlock the event
            // loop, so return the stored copy directly.
            let text = CLIPBOARD_TEXT.lock().unwrap().clone();
            return (!text.is_empty()).then_some(text);
        }

        // Ask the owner to place the UTF-8 selection on our window's `prop` property.
        x11::xlib::XConvertSelection(
            ctx.display,
            ctx.clipboard,
            ctx.utf8,
            ctx.prop,
            ctx.win,
            x11::xlib::CurrentTime,
        );
        x11::xlib::XFlush(ctx.display);

        // Wait for the reply, leaving any other queued events untouched.
        let mut event = x11::xlib::XEvent { type_: 0 };
        x11::xlib::XIfEvent(
            ctx.display,
            &mut event,
            Some(is_selection_notify),
            ctx.win as x11::xlib::XPointer,
        );
        if event.selection.property == 0 {
            return None;
        }
        read_clipboard_property(ctx)
    }
}

/// `XIfEvent` predicate matching the SelectionNotify reply for our requestor window.
unsafe extern "C" fn is_selection_notify(
    _display: *mut x11::xlib::Display,
    event: *mut x11::xlib::XEvent,
    arg: x11::xlib::XPointer,
) -> x11::xlib::Bool {
    unsafe {
        let event = &*event;
        if event.type_ == x11::xlib::SelectionNotify
            && event.selection.requestor == arg as x11::xlib::Window
        {
            x11::xlib::True
        } else {
            x11::xlib::False
        }
    }
}

/// Read (and clear) the property holding a converted selection, as a UTF-8 string.
unsafe fn read_clipboard_property(ctx: &ClipboardCtx) -> Option<String> {
    let mut actual_type: x11::xlib::Atom = 0;
    let mut actual_format: c_int = 0;
    let mut nitems: c_ulong = 0;
    let mut bytes_after: c_ulong = 0;
    let mut data: *mut u8 = std::ptr::null_mut();
    unsafe {
        let status = x11::xlib::XGetWindowProperty(
            ctx.display,
            ctx.win,
            ctx.prop,
            0,
            0x7fff_ffff,
            x11::xlib::False,
            x11::xlib::AnyPropertyType as x11::xlib::Atom,
            &mut actual_type,
            &mut actual_format,
            &mut nitems,
            &mut bytes_after,
            &mut data,
        );
        if status != x11::xlib::Success as c_int || data.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(data, nitems as usize);
        let text = String::from_utf8_lossy(bytes).into_owned();
        x11::xlib::XFree(data as *mut c_void);
        x11::xlib::XDeleteProperty(ctx.display, ctx.win, ctx.prop);
        Some(text)
    }
}

/// Answer a SelectionRequest from another client by writing our text to its property.
fn handle_selection_request(req: &x11::xlib::XSelectionRequestEvent) {
    let Some(ctx) = CLIPBOARD_CTX.get() else {
        return;
    };
    // Obsolete clients pass property == None; fall back to the target atom.
    let property = if req.property != 0 {
        req.property
    } else {
        req.target
    };
    unsafe {
        let mut notify: x11::xlib::XSelectionEvent = std::mem::zeroed();
        notify.type_ = x11::xlib::SelectionNotify;
        notify.display = req.display;
        notify.requestor = req.requestor;
        notify.selection = req.selection;
        notify.target = req.target;
        notify.time = req.time;
        notify.property = 0; // Refuse unless we recognise the requested target.

        if req.target == ctx.targets {
            let targets = [ctx.targets, ctx.utf8, x11::xlib::XA_STRING];
            x11::xlib::XChangeProperty(
                ctx.display,
                req.requestor,
                property,
                x11::xlib::XA_ATOM,
                32,
                x11::xlib::PropModeReplace,
                targets.as_ptr() as *const u8,
                targets.len() as c_int,
            );
            notify.property = property;
        } else if req.target == ctx.utf8 || req.target == x11::xlib::XA_STRING {
            let text = CLIPBOARD_TEXT.lock().unwrap();
            x11::xlib::XChangeProperty(
                ctx.display,
                req.requestor,
                property,
                req.target,
                8,
                x11::xlib::PropModeReplace,
                text.as_ptr(),
                text.len() as c_int,
            );
            notify.property = property;
        }

        let mut ev = x11::xlib::XEvent { selection: notify };
        x11::xlib::XSendEvent(ctx.display, req.requestor, x11::xlib::False, 0, &mut ev);
        x11::xlib::XFlush(ctx.display);
    }
}

/// Convert X11 key state (modifier mask) to OSEventFlag
fn x11_state_to_flags(state: u32) -> Option<OSEventFlag> {
    // X11 modifier masks
    const SHIFT_MASK: u32 = 1 << 0; // ShiftMask
    const CONTROL_MASK: u32 = 1 << 2; // ControlMask
    const MOD1_MASK: u32 = 1 << 3; // Mod1Mask (Alt)

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

/// Image clipboard read - not yet implemented on this platform.
pub fn clipboard_get_image() -> Option<Vec<u8>> {
    None
}

/// Native image file picker - not yet implemented on this platform.
pub fn open_image_file_dialog() -> Option<std::path::PathBuf> {
    None
}
