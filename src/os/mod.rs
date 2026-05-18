#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "windows"),
    not(target_os = "android")
))]
compile_error!("Support for targeted OS is not implemented!",);

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
#[cfg_attr(target_os = "android", path = "android.rs")]
mod os_impl;

use super::imui::Point;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OSEventType {
    MouseMove,
    Press,
    Release,
    Scroll,
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
    pub delta: f32,
    pub flags: Option<OSEventFlag>,
}

impl OSEvent {
    pub fn mouse_move(pos: Point) -> Self {
        Self {
            ty: OSEventType::MouseMove,
            key: OSKey::LeftMouseButton,
            pos: Some(pos),
            chars: None,
            delta: 0.0,
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
            delta: 0.0,
            flags,
        }
    }

    pub fn release(key: OSKey, pos: Option<Point>) -> Self {
        Self {
            ty: OSEventType::Release,
            key,
            pos,
            chars: None,
            delta: 0.0,
            flags: None,
        }
    }

    pub fn scroll(pos: Point, delta: f32) -> Self {
        Self {
            ty: OSEventType::Scroll,
            key: OSKey::LeftMouseButton,
            pos: Some(pos),
            chars: None,
            delta,
            flags: None,
        }
    }

    pub fn text(ch: char) -> Self {
        Self {
            ty: OSEventType::Press,
            key: OSKey::Keyboard(OSKeyCode::KeySpace),
            pos: None,
            chars: Some(ch),
            delta: 0.0,
            flags: None,
        }
    }
}

/// Cursor types for the mouse pointer
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OSCursor {
    #[default]
    Arrow, // Default pointer
    IBeam,   // Text cursor
    Hand,    // Clickable/pointer
    ResizeH, // Horizontal resize
    ResizeV, // Vertical resize
}

pub type Window = os_impl::Window;

pub fn timer_init() -> f64 {
    os_impl::timer_init()
}
pub fn timer_value() -> u64 {
    os_impl::timer_value()
}
