use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{
            LibraryLoader::GetModuleHandleA,
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
        },
        UI::WindowsAndMessaging::*,
    },
    core::*,
};

use crate::{
    imui::Point,
    os::{OSEventType, OSKey, OSKeyCode},
};

use super::OSEvent;

pub struct Window {
    viewport: RECT,
    pub handle: HWND,
    pub dpi: f32,
}

impl Window {
    pub fn new(width: u32, height: u32) -> Self {
        unsafe {
            let instance = GetModuleHandleA(None).unwrap();

            let window_class = s!("window");

            let wc = WNDCLASSA {
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hInstance: instance.into(),
                lpszClassName: window_class,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),

                ..Default::default()
            };

            let atom = RegisterClassA(&wc);
            debug_assert!(atom != 0);

            let win = CreateWindowExA(
                WINDOW_EX_STYLE::default(),
                window_class,
                s!("This is a sample window"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width as i32,
                height as i32,
                None,
                None,
                Some((instance as HMODULE).into()),
                None,
            )
            .expect("Create Window failed");

            SetWindowDisplayAffinity(win, WDA_MONITOR);
            Window {
                viewport: RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                handle: win,
                dpi: 1.,
            }
        }
    }

    pub fn get_size(&self) -> (f32, f32) {
        let width = (self.viewport.right - self.viewport.left) as f32;
        let height = (self.viewport.bottom - self.viewport.top) as f32;
        (width, height)
    }

    pub fn get_render_size(&self) -> (f32, f32) {
        let (width, height) = self.get_size();
        (width * self.dpi, height * self.dpi)
    }

    fn translate_loc(&self, point: POINT) -> Point {
        let mut point = point;
        unsafe { ScreenToClient(self.handle, &mut point) };
        Point::new(point.x as f32, point.y as f32)
    }

    pub fn get_events(&mut self) -> Vec<OSEvent> {
        // XXX: This should not be here
        unsafe { GetClientRect(self.handle, &mut self.viewport) };

        let mut events = Vec::new();

        let mut message = MSG::default();
        let mut last_keydown = OSKey::Keyboard(OSKeyCode::KeyA);
        loop {
            unsafe {
                if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    break;
                }
            };
            let event = match message.message {
                WM_MOUSEMOVE => Some(OSEvent {
                    ty: OSEventType::MouseMove,
                    key: OSKey::LeftMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    delta: 0.,
                }),
                WM_LBUTTONDOWN => Some(OSEvent {
                    ty: OSEventType::Press,
                    key: OSKey::LeftMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    delta: 0.,
                }),
                WM_LBUTTONUP => Some(OSEvent {
                    ty: OSEventType::Release,
                    key: OSKey::LeftMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    delta: 0.,
                }),
                WM_RBUTTONDOWN => Some(OSEvent {
                    ty: OSEventType::Press,
                    key: OSKey::RightMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    delta: 0.,
                }),
                WM_RBUTTONUP => Some(OSEvent {
                    ty: OSEventType::Release,
                    key: OSKey::RightMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    delta: 0.,
                }),
                WM_KEYDOWN => {
                    // XXX: we assume WM_CHAR always comes after WM_KEYDOWN
                    // This is because for now we do not handle properly having a
                    // WM_KEYDOWN then a WM_CHAR for special characters like Return, Backspace, etc.
                    // Atm it means we push two events for the same character, and the text input doesn't handle that.
                    last_keydown = windows_keycode_to_oskey(message.wParam);
                    None
                    //Some(OSEvent {
                    //    ty: OSEventType::Press,
                    //    key: last_keydown,
                    //    pos: Some(self.translate_loc(message.pt)),
                    //    chars: None,
                    //    delta: 0.,
                    //})
                }
                WM_CHAR => Some(OSEvent {
                    ty: OSEventType::Press,
                    key: last_keydown,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: Some(char::from_u32(message.wParam.0 as u32).unwrap()),
                    delta: 0.,
                }),
                _ => None,
            };
            if let Some(event) = event {
                events.push(event);
            }

            unsafe {
                TranslateMessage(&mut message);
                DispatchMessageA(&mut message);
            }
        }

        events
    }
}

fn windows_keycode_to_oskey(param: WPARAM) -> OSKey {
    match param.0 {
        0x08 => OSKey::Keyboard(OSKeyCode::KeyBackspace),
        0x09 => OSKey::Keyboard(OSKeyCode::KeyTab),
        0x0D => OSKey::Keyboard(OSKeyCode::KeyEnter),
        0x10 => OSKey::Keyboard(OSKeyCode::KeyLeftShift),
        0x11 => OSKey::Keyboard(OSKeyCode::KeyLeftCtrl),
        0x12 => OSKey::Keyboard(OSKeyCode::KeyLeftAlt),
        0x14 => OSKey::Keyboard(OSKeyCode::KeyCapsLock),
        0x1B => OSKey::Keyboard(OSKeyCode::KeyEscape),
        0x20 => OSKey::Keyboard(OSKeyCode::KeySpace),
        0x21 => OSKey::Keyboard(OSKeyCode::KeyPageUp),
        0x22 => OSKey::Keyboard(OSKeyCode::KeyPageDown),
        0x23 => OSKey::Keyboard(OSKeyCode::KeyEnd),
        0x24 => OSKey::Keyboard(OSKeyCode::KeyHome),
        0x25 => OSKey::Keyboard(OSKeyCode::KeyLeftArrow),
        0x26 => OSKey::Keyboard(OSKeyCode::KeyUpArrow),
        0x27 => OSKey::Keyboard(OSKeyCode::KeyRightArrow),
        0x28 => OSKey::Keyboard(OSKeyCode::KeyDownArrow),
        0x2D => OSKey::Keyboard(OSKeyCode::KeyInsert),
        0x2E => OSKey::Keyboard(OSKeyCode::KeyDelete),
        0x30 => OSKey::Keyboard(OSKeyCode::Key0),
        0x31 => OSKey::Keyboard(OSKeyCode::Key1),
        0x32 => OSKey::Keyboard(OSKeyCode::Key2),
        0x33 => OSKey::Keyboard(OSKeyCode::Key3),
        0x34 => OSKey::Keyboard(OSKeyCode::Key4),
        0x35 => OSKey::Keyboard(OSKeyCode::Key5),
        0x36 => OSKey::Keyboard(OSKeyCode::Key6),
        0x37 => OSKey::Keyboard(OSKeyCode::Key7),
        0x38 => OSKey::Keyboard(OSKeyCode::Key8),
        0x39 => OSKey::Keyboard(OSKeyCode::Key9),
        0x41 => OSKey::Keyboard(OSKeyCode::KeyA),
        0x42 => OSKey::Keyboard(OSKeyCode::KeyB),
        0x43 => OSKey::Keyboard(OSKeyCode::KeyC),
        0x44 => OSKey::Keyboard(OSKeyCode::KeyD),
        0x45 => OSKey::Keyboard(OSKeyCode::KeyE),
        0x46 => OSKey::Keyboard(OSKeyCode::KeyF),
        0x47 => OSKey::Keyboard(OSKeyCode::KeyG),
        0x48 => OSKey::Keyboard(OSKeyCode::KeyH),
        0x49 => OSKey::Keyboard(OSKeyCode::KeyI),
        0x4A => OSKey::Keyboard(OSKeyCode::KeyJ),
        0x4B => OSKey::Keyboard(OSKeyCode::KeyK),
        0x4C => OSKey::Keyboard(OSKeyCode::KeyL),
        0x4D => OSKey::Keyboard(OSKeyCode::KeyM),
        0x4E => OSKey::Keyboard(OSKeyCode::KeyN),
        0x4F => OSKey::Keyboard(OSKeyCode::KeyO),
        0x50 => OSKey::Keyboard(OSKeyCode::KeyP),
        0x51 => OSKey::Keyboard(OSKeyCode::KeyQ),
        0x52 => OSKey::Keyboard(OSKeyCode::KeyR),
        0x53 => OSKey::Keyboard(OSKeyCode::KeyS),
        0x54 => OSKey::Keyboard(OSKeyCode::KeyT),
        0x55 => OSKey::Keyboard(OSKeyCode::KeyU),
        0x56 => OSKey::Keyboard(OSKeyCode::KeyV),
        0x57 => OSKey::Keyboard(OSKeyCode::KeyW),
        0x58 => OSKey::Keyboard(OSKeyCode::KeyX),
        0x59 => OSKey::Keyboard(OSKeyCode::KeyY),
        0x5A => OSKey::Keyboard(OSKeyCode::KeyZ),
        0x5B => OSKey::Keyboard(OSKeyCode::KeyLeftSuper),
        0x5C => OSKey::Keyboard(OSKeyCode::KeyRightSuper),
        0x60 => OSKey::Keyboard(OSKeyCode::KeyKeypad0),
        0x61 => OSKey::Keyboard(OSKeyCode::KeyKeypad1),
        0x62 => OSKey::Keyboard(OSKeyCode::KeyKeypad2),
        0x63 => OSKey::Keyboard(OSKeyCode::KeyKeypad3),
        0x64 => OSKey::Keyboard(OSKeyCode::KeyKeypad4),
        0x65 => OSKey::Keyboard(OSKeyCode::KeyKeypad5),
        0x66 => OSKey::Keyboard(OSKeyCode::KeyKeypad6),
        0x67 => OSKey::Keyboard(OSKeyCode::KeyKeypad7),
        0x68 => OSKey::Keyboard(OSKeyCode::KeyKeypad8),
        0x69 => OSKey::Keyboard(OSKeyCode::KeyKeypad9),
        0x6A => OSKey::Keyboard(OSKeyCode::KeyKeypadMultiply),
        0x6B => OSKey::Keyboard(OSKeyCode::KeyKeypadAdd),
        0x6D => OSKey::Keyboard(OSKeyCode::KeyKeypadSubtract),
        0x6E => OSKey::Keyboard(OSKeyCode::KeyKeypadDecimal),
        0x6F => OSKey::Keyboard(OSKeyCode::KeyKeypadDivide),
        0x70 => OSKey::Keyboard(OSKeyCode::KeyF1),
        0x71 => OSKey::Keyboard(OSKeyCode::KeyF2),
        0x72 => OSKey::Keyboard(OSKeyCode::KeyF3),
        0x73 => OSKey::Keyboard(OSKeyCode::KeyF4),
        0x74 => OSKey::Keyboard(OSKeyCode::KeyF5),
        0x75 => OSKey::Keyboard(OSKeyCode::KeyF6),
        0x76 => OSKey::Keyboard(OSKeyCode::KeyF7),
        0x77 => OSKey::Keyboard(OSKeyCode::KeyF8),
        0x78 => OSKey::Keyboard(OSKeyCode::KeyF9),
        0x79 => OSKey::Keyboard(OSKeyCode::KeyF10),
        0x7A => OSKey::Keyboard(OSKeyCode::KeyF11),
        0x7B => OSKey::Keyboard(OSKeyCode::KeyF12),
        0x7C => OSKey::Keyboard(OSKeyCode::KeyF13),
        0x7D => OSKey::Keyboard(OSKeyCode::KeyF14),
        0x7E => OSKey::Keyboard(OSKeyCode::KeyF15),
        0x7F => OSKey::Keyboard(OSKeyCode::KeyF16),
        0x80 => OSKey::Keyboard(OSKeyCode::KeyF17),
        0x81 => OSKey::Keyboard(OSKeyCode::KeyF18),
        0x82 => OSKey::Keyboard(OSKeyCode::KeyF19),
        0x83 => OSKey::Keyboard(OSKeyCode::KeyF20),
        0x90 => OSKey::Keyboard(OSKeyCode::KeyNumLock),
        0xA0 => OSKey::Keyboard(OSKeyCode::KeyLeftShift),
        0xA1 => OSKey::Keyboard(OSKeyCode::KeyRightShift),
        0xA2 => OSKey::Keyboard(OSKeyCode::KeyLeftCtrl),
        0xA3 => OSKey::Keyboard(OSKeyCode::KeyRightCtrl),
        0xA4 => OSKey::Keyboard(OSKeyCode::KeyLeftAlt),
        0xA5 => OSKey::Keyboard(OSKeyCode::KeyRightAlt),
        0xBF => OSKey::Keyboard(OSKeyCode::KeySlash),
        0xC0 => OSKey::Keyboard(OSKeyCode::KeyGraveAccent),
        _ => {
            println!("WARNING: Key not handled: {:?}!", param.0);
            OSKey::Keyboard(OSKeyCode::KeyA)
        }
    }
}

extern "system" fn wndproc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            // WM_PAINT => LRESULT(0),
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_SIZE | WM_SIZING => {
                // PostMessageW(Some(window), message, wparam, lparam);
                LRESULT(0)
            }
            _ => DefWindowProcA(window, message, wparam, lparam),
        }
    }
}

pub fn timer_init() -> f64 {
    let mut counter: i64 = 0;
    // Returns 10_000_000 on Windows 11, Amd Ryzen CPU
    unsafe { QueryPerformanceFrequency(&mut counter).unwrap() };
    counter as f64
}
pub fn timer_value() -> u64 {
    let mut ticks: i64 = 0;
    unsafe {
        QueryPerformanceCounter(&mut ticks).unwrap();
    }
    ticks as u64
}
