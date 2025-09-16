use windows::{
    Win32::Foundation::*, Win32::Graphics::Gdi::*, Win32::System::LibraryLoader::GetModuleHandleA,
    Win32::System::LibraryLoader::GetProcAddress, Win32::UI::WindowsAndMessaging::*, core::*,
};

use crate::{
    imui::Point,
    os::{OSEventType, OSKey},
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
        loop {
            unsafe {
                if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    break;
                }
            };
            if message.message == WM_PAINT {
                continue;
            }
            println!("Event: {:?}", message);
            match message.message {
                WM_MOUSEMOVE => events.push(OSEvent {
                    ty: OSEventType::MouseMove,
                    key: OSKey::LeftMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    delta: 0.,
                }),
                // WM_SIZING | WM_SIZE => {
                //     println!("Resizing ev");
                //     self.width = (message.lParam.0 as u32) as f32;
                //     self.height = ((message.lParam.0 as u64) >> 32) as f32;
                //     unsafe {
                //         DefWindowProcW(self.handle, message.message, message.wParam, message.lParam)
                //     };
                // }
                _ => {}
            }

            unsafe {
                TranslateMessage(&mut message);
                DispatchMessageA(&mut message);
            }
        }

        events
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
    // TODO(xarkes)
    1.
}
pub fn timer_value() -> u64 {
    1
}
