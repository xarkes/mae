#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "windows"),
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
compile_error!("Support for targeted OS is not implemented!",);

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_os = "android", path = "android.rs")]
#[cfg_attr(target_arch = "wasm32", path = "wasm.rs")]
mod os_impl;

mod signals;

use super::imui::Point;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OSEventType {
    MouseMove,
    Press,
    Release,
    Scroll,
    Quit,
    Resize,
    Repaint,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OSKeyCode {
    KeyA,
    KeyS,
    KeyD,
    KeyF,
    KeyH,
    KeyG,
    KeyZ,
    KeyX,
    KeyC,
    KeyV,
    KeyB,
    KeyQ,
    KeyW,
    KeyE,
    KeyR,
    KeyY,
    KeyT,
    Key1,
    Key2,
    Key3,
    Key4,
    Key6,
    Key5,
    KeyEqual,
    Key9,
    Key7,
    KeyMinus,
    Key8,
    Key0,
    KeyRightBracket,
    KeyO,
    KeyU,
    KeyLeftBracket,
    KeyI,
    KeyP,
    KeyL,
    KeyJ,
    KeyApostrophe,
    KeyK,
    KeySemicolon,
    KeyBackslash,
    KeyComma,
    KeySlash,
    KeyN,
    KeyM,
    KeyPeriod,
    KeyGraveAccent,
    KeyKeypadDecimal,
    KeyKeypadMultiply,
    KeyKeypadAdd,
    KeyNumLock,
    KeyKeypadDivide,
    KeyKeypadEnter,
    KeyKeypadSubtract,
    KeyKeypadEqual,
    KeyKeypad0,
    KeyKeypad1,
    KeyKeypad2,
    KeyKeypad3,
    KeyKeypad4,
    KeyKeypad5,
    KeyKeypad6,
    KeyKeypad7,
    KeyKeypad8,
    KeyKeypad9,
    KeyEnter,
    KeyTab,
    KeySpace,
    KeyBackspace,
    KeyEscape,
    KeyCapsLock,
    KeyLeftCtrl,
    KeyLeftShift,
    KeyLeftAlt,
    KeyLeftSuper,
    KeyRightCtrl,
    KeyRightShift,
    KeyRightAlt,
    KeyRightSuper,
    KeyF1,
    KeyF2,
    KeyF3,
    KeyF4,
    KeyF5,
    KeyF6,
    KeyF7,
    KeyF8,
    KeyF9,
    KeyF10,
    KeyF11,
    KeyF12,
    KeyF13,
    KeyF14,
    KeyF15,
    KeyF16,
    KeyF17,
    KeyF18,
    KeyF19,
    KeyF20,
    KeyMenu,
    KeyInsert,
    KeyHome,
    KeyPageUp,
    KeyDelete,
    KeyEnd,
    KeyPageDown,
    KeyLeftArrow,
    KeyRightArrow,
    KeyDownArrow,
    KeyUpArrow,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OSKey {
    LeftMouseButton,
    RightMouseButton,
    Keyboard(OSKeyCode),
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OSEventFlag {
    Control = 1,
    Alt = 2,
    Shift = 4,
    Super = 8,

    ControlAlt = 3,
    ControlShift = 5,
    AltShift = 6,
    ControlAltShift = 7,
    ControlSuper = 9,
    AltSuper = 10,
    ControlAltSuper = 11,
    ShiftSuper = 12,
    ControlShiftSuper = 13,
    AltShiftSuper = 14,
    ControlAltShiftSuper = 15,
}
impl OSEventFlag {
    /// The platform's primary shortcut modifier — the "command" key: ⌘ Command
    /// on macOS, Control on every other OS. Use this for application keyboard
    /// shortcuts so they match each platform's convention (`Cmd+F` on macOS,
    /// `Ctrl+F` on Linux/Windows) from a single binding.
    #[cfg(not(target_arch = "wasm32"))]
    pub const fn command() -> Self {
        #[cfg(target_os = "macos")]
        {
            OSEventFlag::Super
        }
        #[cfg(not(target_os = "macos"))]
        {
            OSEventFlag::Control
        }
    }

    /// The web build asks the *browser*, at runtime, rather than reading
    /// `target_os` like every other target does.
    ///
    /// `wasm32-unknown-unknown` has no OS: `target_os` is `"unknown"` there
    /// whatever machine the page is open on, so the compile-time answer above
    /// is Control unconditionally — which is right on Linux and Windows and
    /// wrong on every Mac, where ⌘+F is what a user presses and Ctrl+F is
    /// what nobody presses. One `navigator` read, cached, decides it instead.
    #[cfg(target_arch = "wasm32")]
    pub fn command() -> Self {
        if os_impl::is_apple_platform() {
            OSEventFlag::Super
        } else {
            OSEventFlag::Control
        }
    }

    /// How this platform's primary modifier is *written* in a shortcut hint:
    /// `⌘` where [`Self::command`] is ⌘ Command, `Ctrl` where it is Control.
    /// Same runtime answer on the web as `command` itself, so a tooltip can
    /// never name a different key from the one that works.
    pub fn command_label() -> &'static str {
        if Self::command() == OSEventFlag::Super {
            "\u{2318}"
        } else {
            "Ctrl"
        }
    }

    /// This modifier combined (bitwise union) with `other`, e.g.
    /// `OSEventFlag::command().with(OSEventFlag::Shift)` for `Cmd/Ctrl+Shift`.
    pub fn with(self, other: OSEventFlag) -> OSEventFlag {
        // Every 0..=15 bit combination is a variant, so this never fails.
        OSEventFlag::try_from((self as i32) | (other as i32)).unwrap_or(self)
    }
}

impl TryFrom<i32> for OSEventFlag {
    type Error = ();

    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            1 => Ok(OSEventFlag::Control),
            2 => Ok(OSEventFlag::Alt),
            3 => Ok(OSEventFlag::ControlAlt),
            4 => Ok(OSEventFlag::Shift),
            5 => Ok(OSEventFlag::ControlShift),
            6 => Ok(OSEventFlag::AltShift),
            7 => Ok(OSEventFlag::ControlAltShift),
            8 => Ok(OSEventFlag::Super),
            9 => Ok(OSEventFlag::ControlSuper),
            10 => Ok(OSEventFlag::AltSuper),
            11 => Ok(OSEventFlag::ControlAltSuper),
            12 => Ok(OSEventFlag::ShiftSuper),
            13 => Ok(OSEventFlag::ControlShiftSuper),
            14 => Ok(OSEventFlag::AltShiftSuper),
            15 => Ok(OSEventFlag::ControlAltShiftSuper),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy)]
pub struct OSEvent {
    pub ty: OSEventType,
    pub key: OSKey,
    pub pos: Option<Point>,
    pub chars: Option<char>,
    pub deltax: f32,
    pub deltay: f32,
    pub flags: Option<OSEventFlag>,
}

impl OSEvent {
    pub fn mouse_move(pos: Point) -> Self {
        Self {
            ty: OSEventType::MouseMove,
            key: OSKey::LeftMouseButton,
            pos: Some(pos),
            chars: None,
            deltax: 0.0,
            deltay: 0.0,
            flags: None,
        }
    }

    pub fn press(key: OSKey, pos: Option<Point>) -> Self {
        Self::press_with_flags(key, pos, None)
    }

    pub fn press_with_flags(key: OSKey, pos: Option<Point>, flags: Option<OSEventFlag>) -> Self {
        Self {
            ty: OSEventType::Press,
            key,
            pos,
            chars: None,
            deltax: 0.0,
            deltay: 0.0,
            flags,
        }
    }

    pub fn release(key: OSKey, pos: Option<Point>) -> Self {
        Self {
            ty: OSEventType::Release,
            key,
            pos,
            chars: None,
            deltax: 0.0,
            deltay: 0.0,
            flags: None,
        }
    }

    pub fn scroll(pos: Point, delta: f32) -> Self {
        Self::scroll_with_flags(pos, delta, None)
    }

    pub fn scroll_with_flags(pos: Point, delta: f32, flags: Option<OSEventFlag>) -> Self {
        Self {
            ty: OSEventType::Scroll,
            key: OSKey::LeftMouseButton,
            pos: Some(pos),
            chars: None,
            deltax: 0.,
            deltay: delta,
            flags,
        }
    }

    pub fn text(ch: char) -> Self {
        Self {
            ty: OSEventType::Press,
            key: OSKey::Keyboard(OSKeyCode::KeySpace),
            pos: None,
            chars: Some(ch),
            deltax: 0.0,
            deltay: 0.0,
            flags: None,
        }
    }

    pub fn quit() -> Self {
        Self {
            ty: OSEventType::Quit,
            key: OSKey::LeftMouseButton,
            pos: None,
            chars: None,
            deltax: 0.0,
            deltay: 0.0,
            flags: None,
        }
    }
}

/// Cursor types for the mouse pointer
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OSCursor {
    #[default]
    Arrow, // Default pointer
    IBeam,      // Text cursor
    Hand,       // Clickable/pointer
    ResizeH,    // Horizontal resize
    ResizeV,    // Vertical resize
    ResizeNWSE, // Diagonal (top-left ↔ bottom-right) corner resize
}

pub type Window = os_impl::Window;

pub fn timer_init() -> f64 {
    os_impl::timer_init()
}
pub fn timer_value() -> u64 {
    os_impl::timer_value()
}

/// Replace the system clipboard's contents with `text`.
pub fn clipboard_set(text: &str) {
    os_impl::clipboard_set(text);
}

/// Read the system clipboard's plain-text contents, or `None` if it holds no string
/// (or the platform has no clipboard integration yet).
pub fn clipboard_get() -> Option<String> {
    os_impl::clipboard_get()
}

/// Read an image off the system clipboard as encoded bytes (PNG or TIFF), or
/// `None` if it holds no image (or the platform has no integration yet). The
/// caller decodes the bytes (e.g. via the `image` crate, which sniffs format).
pub fn clipboard_get_image() -> Option<Vec<u8>> {
    os_impl::clipboard_get_image()
}

/// Whether a quit signal (Ctrl+C / `kill`) has arrived since the last call, so
/// the event loop can turn it into a `Quit` event and take the app's normal
/// shutdown path instead of being killed where it stands. See `signals.rs`.
pub fn take_quit_signal() -> bool {
    signals::take_quit_signal()
}

/// Open a native file picker for choosing an image (png/jpg/jpeg). Returns the
/// chosen path, or `None` if cancelled / unsupported on this platform.
pub fn open_image_file_dialog() -> Option<std::path::PathBuf> {
    os_impl::open_image_file_dialog()
}

#[cfg(test)]
mod tests {
    use super::OSEventFlag;

    #[test]
    fn command_maps_to_platform_primary_modifier() {
        let cmd = OSEventFlag::command();
        if cfg!(target_os = "macos") {
            assert_eq!(cmd, OSEventFlag::Super);
        } else {
            assert_eq!(cmd, OSEventFlag::Control);
        }
    }

    #[test]
    fn with_combines_modifiers_into_the_union_variant() {
        // command + Shift is the platform "primary+shift" chord.
        let chord = OSEventFlag::command().with(OSEventFlag::Shift);
        if cfg!(target_os = "macos") {
            assert_eq!(chord, OSEventFlag::ShiftSuper);
        } else {
            assert_eq!(chord, OSEventFlag::ControlShift);
        }
        // Bitwise union is order-independent and idempotent.
        assert_eq!(
            OSEventFlag::Control.with(OSEventFlag::Alt),
            OSEventFlag::ControlAlt
        );
        assert_eq!(
            OSEventFlag::Shift.with(OSEventFlag::Shift),
            OSEventFlag::Shift
        );
    }
}
