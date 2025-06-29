use windows::{
    Win32::Foundation::*, Win32::Graphics::Gdi::*, Win32::System::LibraryLoader::GetModuleHandleA,
    Win32::System::LibraryLoader::GetProcAddress, Win32::UI::WindowsAndMessaging::*, core::*,
};

use super::OSEvent;

pub struct Window {}

impl Window {
    pub fn new(width: u32, height: u32) -> Self {
        unsafe {
            let instance = GetModuleHandleA(None).unwrap();
            debug_assert!(instance.0 != 0);

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
                instance,
                None,
            );

            SetWindowDisplayAffinity(win, WDA_MONITOR);
            Window {}
        }
    }

    pub fn get_size(&self) -> (f32, f32) {
        // TODO
        (1024., 768.)
    }

    pub fn get_events(&self) -> Vec<OSEvent> {
        let events = Vec::new();

        // TODO
        let mut message = MSG::default();
        while unsafe { PeekMessageA(&mut message, HWND(0), 0, 0, PM_REMOVE).into() } {
            if message.message == WM_QUIT {
                // running = false;
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
            WM_PAINT => {
                println!("WM_PAINT");
                //ValidateRect(window, None);

                let mut msg = String::from("ZOMG!");
                let mut ps = PAINTSTRUCT::default();
                let psp = &mut ps as *mut PAINTSTRUCT;
                let rp = &mut ps.rcPaint as *mut RECT;
                let hdc = BeginPaint(window, psp);
                let brush = CreateSolidBrush(COLORREF(0x0000F0F0));
                FillRect(hdc, &ps.rcPaint, brush);
                DrawTextA(
                    hdc,
                    msg.as_bytes_mut(),
                    rp,
                    DT_SINGLELINE | DT_CENTER | DT_VCENTER,
                );
                EndPaint(window, &ps);
                LRESULT(0)
            }
            WM_DESTROY => {
                println!("WM_DESTROY");
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_SIZE => {
                println!("WM_SIZE");
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
pub fn timer_value() -> f64 {
    1.
}
