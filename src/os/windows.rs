use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{
            DataExchange::*,
            LibraryLoader::GetModuleHandleA,
            Memory::*,
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
        },
        UI::{
            Input::{
                Ime::*,
                KeyboardAndMouse::{
                    GetKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_PROCESSKEY, VK_RWIN, VK_SHIFT,
                },
            },
            WindowsAndMessaging::*,
        },
    },
    core::*,
};

use std::cell::Cell;

use crate::{
    imui::Point,
    os::{OSEventFlag, OSEventType, OSKey, OSKeyCode},
};

use super::{OSCursor, OSEvent};

pub struct Window {
    viewport: RECT,
    pub handle: HWND,
    pub dpi: f32,
    current_cursor: OSCursor,
    /// Current IME composition (preedit) string, drawn inline by the focused editor.
    /// Empty / `None` when not composing. Updated from `WM_IME_COMPOSITION`.
    ime_preedit: Option<String>,
    /// Caret position (client coords) reported by the focused editor; the IME
    /// composition + candidate windows are pinned here. `None` until first set.
    ime_caret: Cell<Option<POINT>>,
}

impl Window {
    /// The current IME preedit (composing) string, if any. Rendered inline by the
    /// focused text editor; not part of the committed buffer.
    pub fn ime_preedit(&self) -> Option<String> {
        self.ime_preedit.clone()
    }

    /// Report the focused caret's rectangle (client-area coordinates) so the IME
    /// places its composition / candidate windows next to the caret.
    pub fn set_ime_caret_rect(&self, x: f32, y: f32, _width: f32, height: f32) {
        // IMM positions windows at a single point; use the caret's bottom-left.
        let pt = POINT {
            x: x as i32,
            y: (y + height) as i32,
        };
        self.ime_caret.set(Some(pt));
        self.apply_ime_caret();
    }

    /// Pin the IME composition + candidate windows to the stored caret point.
    fn apply_ime_caret(&self) {
        let Some(pt) = self.ime_caret.get() else {
            return;
        };
        unsafe {
            let himc = ImmGetContext(self.handle);
            if himc.0.is_null() {
                return;
            }
            let comp = COMPOSITIONFORM {
                dwStyle: CFS_POINT,
                ptCurrentPos: pt,
                rcArea: RECT::default(),
            };
            let _ = ImmSetCompositionWindow(himc, &comp);
            let cand = CANDIDATEFORM {
                dwIndex: 0,
                dwStyle: CFS_CANDIDATEPOS,
                ptCurrentPos: pt,
                rcArea: RECT::default(),
            };
            let _ = ImmSetCandidateWindow(himc, &cand);
            let _ = ImmReleaseContext(self.handle, himc);
        }
    }

    /// Read the IME composition string of the given kind (`GCS_COMPSTR` for the
    /// preedit, `GCS_RESULTSTR` for the committed text). Returns `None` when empty.
    fn ime_composition_string(&self, kind: IME_COMPOSITION_STRING) -> Option<String> {
        unsafe {
            let himc = ImmGetContext(self.handle);
            if himc.0.is_null() {
                return None;
            }
            // First call (null buffer) returns the byte length of the UTF-16 string.
            let byte_len = ImmGetCompositionStringW(himc, kind, None, 0);
            let result = if byte_len > 0 {
                let mut buf = vec![0u8; byte_len as usize];
                ImmGetCompositionStringW(
                    himc,
                    kind,
                    Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
                    byte_len as u32,
                );
                let units: Vec<u16> = buf
                    .chunks_exact(2)
                    .map(|c| u16::from_ne_bytes([c[0], c[1]]))
                    .collect();
                Some(String::from_utf16_lossy(&units))
            } else {
                None
            };
            let _ = ImmReleaseContext(self.handle, himc);
            result
        }
    }

    /// Finalize any in-progress IME composition, committing it where it was being
    /// typed. The OS only ends a composition on focus/caret changes it can observe
    /// (e.g. clicking in Notepad); it never sees our *in-app* caret moves, so a
    /// click that repositions the editor caret would otherwise leave the preedit
    /// active and trailing the new caret. We commit the current composition string
    /// as ordinary text events (so it lands at the old caret, before the click is
    /// applied) and tell the IME to drop it so it isn't committed a second time.
    fn finish_composition(&mut self, events: &mut Vec<OSEvent>) {
        if self.ime_preedit.is_none() {
            return;
        }
        if let Some(committed) = self.ime_composition_string(GCS_COMPSTR) {
            for ch in committed.chars() {
                events.push(OSEvent::text(ch));
            }
        }
        unsafe {
            let himc = ImmGetContext(self.handle);
            if !himc.0.is_null() {
                // CPS_CANCEL clears the composition without emitting a result string,
                // so the text we just committed manually isn't duplicated.
                let _ = ImmNotifyIME(himc, NI_COMPOSITIONSTR, CPS_CANCEL, 0);
                let _ = ImmReleaseContext(self.handle, himc);
            }
        }
        self.ime_preedit = None;
        events.push(ime_repaint());
    }

    pub fn new(width: u32, height: u32, title: &str) -> Self {
        unsafe {
            let instance = GetModuleHandleA(None).unwrap();

            let window_class = w!("window");

            let wc = WNDCLASSW {
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hInstance: instance.into(),
                lpszClassName: window_class,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),

                ..Default::default()
            };

            let atom = RegisterClassW(&wc);
            debug_assert!(atom != 0);

            // Keep the wide title alive in an owned local for the duration of the
            // call. Passing `&HSTRING` lets the bindings build the PCWSTR without us
            // handing out a raw pointer into a temporary.
            let title = HSTRING::from(title);
            let win = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                window_class,
                &title,
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
                current_cursor: OSCursor::Arrow,
                ime_preedit: None,
                ime_caret: Cell::new(None),
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

    pub fn refresh_rate_hz(&self) -> f32 {
        unsafe {
            let hdc = GetDC(Some(self.handle));
            if hdc.is_invalid() {
                return 60.0;
            }

            let rate = GetDeviceCaps(Some(hdc), VREFRESH);
            ReleaseDC(Some(self.handle), hdc);
            if rate > 1 { rate as f32 } else { 60.0 }
        }
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

        // xarkes: TODO: fix a bug where "hold" is not reset when cursor goes outside the window
        // most likely because the buttonup event wont be forwarded to the window
        let mut message = MSG::default();
        loop {
            unsafe {
                if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    break;
                }
            };

            // IME composition messages are handled and *consumed* here (no
            // DispatchMessage), so the OS doesn't draw its own inline composition or
            // double-commit the text. We render the preedit inline ourselves and emit
            // committed text as ordinary char events, mirroring the macOS path.
            match message.message {
                WM_IME_STARTCOMPOSITION => {
                    self.ime_preedit = Some(String::new());
                    self.apply_ime_caret();
                    continue;
                }
                WM_IME_COMPOSITION => {
                    let lparam = message.lParam.0 as u32;
                    // A completed conversion: commit the result string as text events.
                    if lparam & GCS_RESULTSTR.0 != 0
                        && let Some(committed) = self.ime_composition_string(GCS_RESULTSTR)
                    {
                        for ch in committed.chars() {
                            events.push(OSEvent::text(ch));
                        }
                        self.ime_preedit = None;
                    }
                    // The in-progress composition: update the inline preedit.
                    if lparam & GCS_COMPSTR.0 != 0 {
                        let preedit = self.ime_composition_string(GCS_COMPSTR);
                        self.ime_preedit = preedit.filter(|s| !s.is_empty());
                        // Keep the candidate window pinned to the caret as it moves.
                        self.apply_ime_caret();
                    }
                    events.push(ime_repaint());
                    continue;
                }
                WM_IME_ENDCOMPOSITION => {
                    self.ime_preedit = None;
                    events.push(ime_repaint());
                    continue;
                }
                // A mouse press (which moves the editor caret) or losing focus must
                // finalize composition first, so the composed text commits where it
                // was typed rather than trailing the caret. We don't `continue`: the
                // button-press event itself is still emitted by the match below.
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_KILLFOCUS => {
                    self.finish_composition(&mut events);
                }
                _ => {}
            }

            let event = match message.message {
                WM_MOUSEMOVE => Some(OSEvent {
                    ty: OSEventType::MouseMove,
                    key: OSKey::LeftMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    deltax: 0.,
                    deltay: 0.,
                    flags: None,
                }),
                WM_LBUTTONDOWN => Some(OSEvent {
                    ty: OSEventType::Press,
                    key: OSKey::LeftMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    deltax: 0.,
                    deltay: 0.,
                    flags: None,
                }),
                WM_LBUTTONUP => Some(OSEvent {
                    ty: OSEventType::Release,
                    key: OSKey::LeftMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    deltax: 0.,
                    deltay: 0.,
                    flags: None,
                }),
                WM_RBUTTONDOWN => Some(OSEvent {
                    ty: OSEventType::Press,
                    key: OSKey::RightMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    deltax: 0.,
                    deltay: 0.,
                    flags: None,
                }),
                WM_RBUTTONUP => Some(OSEvent {
                    ty: OSEventType::Release,
                    key: OSKey::RightMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    deltax: 0.,
                    deltay: 0.,
                    flags: None,
                }),
                WM_MOUSEWHEEL => Some(OSEvent {
                    ty: OSEventType::Scroll,
                    key: OSKey::LeftMouseButton,
                    pos: Some(self.translate_loc(message.pt)),
                    chars: None,
                    deltax: 0.,
                    deltay: (message.wParam.0 >> 16) as i16 as f32 / 60.,
                    flags: current_modifier_flags(),
                }),
                WM_KEYDOWN if message.wParam.0 == VK_PROCESSKEY.0 as usize => {
                    // The IME is consuming this key for composition. Let it through to
                    // DefWindowProc (below) so the IME posts its WM_IME_* messages; emit
                    // no key event of our own. TranslateMessage cooperates with the IME
                    // here rather than producing a plain WM_CHAR.
                    unsafe {
                        let _ = TranslateMessage(&message);
                    }
                    None
                }
                WM_KEYDOWN => {
                    // A key press surfaces as a single Press event carrying both the
                    // key code (for navigation/shortcuts) and the typed character (for
                    // text input), matching the other platforms.
                    //
                    // Translating the key-down posts its WM_CHAR (if the key produces
                    // text) which we consume right here. Keys without a character —
                    // arrows, function keys, Home/End, … — produce no WM_CHAR and would
                    // otherwise be dropped entirely; we still emit a Press for them.
                    let key = windows_keycode_to_oskey(message.wParam);
                    let chars = unsafe {
                        TranslateMessage(&message);
                        let mut char_msg = MSG::default();
                        if PeekMessageW(
                            &mut char_msg,
                            Some(self.handle),
                            WM_CHAR,
                            WM_CHAR,
                            PM_REMOVE,
                        )
                        .as_bool()
                        {
                            char::from_u32(char_msg.wParam.0 as u32)
                        } else {
                            None
                        }
                    };
                    Some(OSEvent {
                        ty: OSEventType::Press,
                        key,
                        pos: None,
                        chars,
                        deltax: 0.,
                        deltay: 0.,
                        flags: current_modifier_flags(),
                    })
                }
                _ => None,
            };
            if let Some(event) = event {
                events.push(event);
            }

            // NOTE: WM_KEYDOWN is translated inline above, so we must not translate
            // again here or we would post a duplicate WM_CHAR. The cursor is kept in
            // sync via the window class cursor (see `set_cursor`), so WM_SETCURSOR can
            // fall through to DefWindowProc.
            //
            // Use the wide dispatch to match our Unicode window class, so messages
            // carrying text (e.g. WM_SETTEXT from SetWindowTextW) keep their wide
            // strings instead of being reinterpreted as ANSI.
            unsafe {
                DispatchMessageW(&message);
            }
        }

        events
    }

    pub fn set_title(&self, title: &str) {
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            SetWindowTextW(self.handle, PCWSTR(title_w.as_ptr()));
        }
    }

    pub fn set_app_icon(&self, _png_bytes: &[u8]) {
        // !TODO
    }

    pub fn set_cursor(&mut self, cursor: OSCursor) {
        if self.current_cursor == cursor {
            return;
        }
        self.current_cursor = cursor;
        let cur_kind = match cursor {
            OSCursor::Arrow => IDC_ARROW,
            OSCursor::IBeam => IDC_IBEAM,
            OSCursor::Hand => IDC_HAND,
            OSCursor::ResizeH => IDC_SIZEWE,
            OSCursor::ResizeV => IDC_SIZENS,
            OSCursor::ResizeNWSE => IDC_SIZENWSE,
        };
        unsafe {
            let hcursor = LoadCursorW(None, cur_kind).unwrap();
            // Update the window class cursor as well: the default WM_SETCURSOR
            // handling re-applies the class cursor on every mouse move over the
            // client area, so without this our SetCursor would be reset right away.
            SetClassLongPtrW(self.handle, GCLP_HCURSOR, hcursor.0 as isize);
            SetCursor(Some(hcursor));
        }
    }
}

/// A bare repaint event, emitted when the IME preedit changes without committing
/// text so the focused editor redraws the composing string this frame.
fn ime_repaint() -> OSEvent {
    OSEvent {
        ty: OSEventType::Repaint,
        key: OSKey::LeftMouseButton,
        pos: None,
        chars: None,
        deltax: 0.,
        deltay: 0.,
        flags: None,
    }
}

/// Read the keyboard modifier keys that are currently held, mapped to an
/// [`OSEventFlag`] (matching how the other platforms tag key events).
fn current_modifier_flags() -> Option<OSEventFlag> {
    let mut out = 0i32;
    unsafe {
        // GetKeyState's high bit is set while the key is down.
        if GetKeyState(VK_CONTROL.0 as i32) < 0 {
            out |= OSEventFlag::Control as i32;
        }
        if GetKeyState(VK_SHIFT.0 as i32) < 0 {
            out |= OSEventFlag::Shift as i32;
        }
        if GetKeyState(VK_MENU.0 as i32) < 0 {
            out |= OSEventFlag::Alt as i32;
        }
        if GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0 {
            out |= OSEventFlag::Super as i32;
        }
    }
    OSEventFlag::try_from(out).ok()
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
            // The window class is registered as Unicode (RegisterClassW), and the
            // title is set with SetWindowTextW. WM_SETTEXT is handled by the default
            // window proc, so it must be the wide one — DefWindowProcA would read the
            // wide title as ANSI and keep only its first character.
            _ => DefWindowProcW(window, message, wparam, lparam),
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

const CF_UNICODETEXT: u32 = 13;

pub fn clipboard_set(text: &str) {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytelen = utf16.len() * 2;
    unsafe {
        match OpenClipboard(None) {
            Err(_) => {
                return;
            }
            _ => {}
        }
        match EmptyClipboard() {
            Err(_) => {
                return;
            }
            _ => {}
        }

        let hmem = GlobalAlloc(GMEM_MOVEABLE, bytelen).unwrap();
        if hmem.0.is_null() {
            return;
        }

        let dst = GlobalLock(hmem) as *mut u16;
        if dst.is_null() {
            let _ = GlobalFree(Some(hmem));
            return;
        }
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), dst, utf16.len());
        let _ = GlobalUnlock(hmem);

        if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hmem.0)))
            .unwrap()
            .0
            .is_null()
        {
            // API failed and we still own the pointer, so we must free it.
            // Otherwise, system takes ownership
            let _ = GlobalFree(Some(hmem));
            return;
        }
        let _ = CloseClipboard();
    }
}

pub fn clipboard_get() -> Option<String> {
    unsafe {
        match OpenClipboard(None) {
            Err(_) => {
                return None;
            }
            _ => {}
        }

        let h = GetClipboardData(CF_UNICODETEXT).unwrap();
        let out = match h.is_invalid() {
            true => None,
            false => {
                let ptr = GlobalLock(HGLOBAL(h.0)) as *const u16;

                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }

                let slice = std::slice::from_raw_parts(ptr, len);
                let _ = GlobalUnlock(HGLOBAL(h.0));

                Some(String::from_utf16_lossy(slice))
            }
        };
        let _ = CloseClipboard();

        return out;
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
