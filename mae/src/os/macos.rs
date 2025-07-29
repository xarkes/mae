extern crate objc2;

use crate::imui::Point;

use super::{OSEvent, OSEventType, OSKey, OSKeyCode};
use std::cell::OnceCell;

use objc2::{
    DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
    sel,
};
use objc2_app_kit::{
    NSAnyEventMask, NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSAutoresizingMaskOptions, NSBackingStoreType, NSEventSubtype, NSEventType, NSMenu, NSMenuItem,
    NSView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSDate, NSDefaultRunLoopMode, NSNotification, NSObject, NSObjectProtocol,
    NSPoint, NSRect, NSSize, ns_string,
};

#[derive(Debug, Default)]
struct AppDelegateIvars {
    window: OnceCell<Retained<NSWindow>>,
    view: OnceCell<Retained<NSView>>,
    width: u32,
    height: u32,
}

pub struct Window {
    pub window: OnceCell<Retained<NSWindow>>,
    pub view: OnceCell<Retained<NSView>>,
    pub dpi: f32,
}

define_class!(
    // SAFETY:
    // - The superclass NSObject does not have any subclassing requirements.
    // - `Delegate` does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppDelegateIvars]
    struct Delegate;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for Delegate {}

    // SAFETY: `NSApplicationDelegate` has no safety requirements.
    unsafe impl NSApplicationDelegate for Delegate {
        // SAFETY: The signature is correct.
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, notification: &NSNotification) {
            let mtm = self.mtm();

            let app = unsafe { notification.object() }
                .unwrap()
                .downcast::<NSApplication>()
                .unwrap();

            // SAFETY: We disable releasing when closed below.
            let frame_rect = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(self.ivars().width as f64, self.ivars().height as f64),
            );
            let window = unsafe {
                NSWindow::initWithContentRect_styleMask_backing_defer(
                    NSWindow::alloc(mtm),
                    frame_rect,
                    NSWindowStyleMask::Titled
                        | NSWindowStyleMask::Closable
                        | NSWindowStyleMask::Miniaturizable
                        | NSWindowStyleMask::Resizable,
                    NSBackingStoreType::Buffered,
                    false,
                )
            };
            // SAFETY: Disable auto-release when closing windows.
            // This is required when creating `NSWindow` outside a window
            // controller.
            unsafe { window.setReleasedWhenClosed(false) };

            // xarkes: set window properties
            window.setTitle(ns_string!("A window"));
            window.center();
            unsafe { window.setContentMinSize(NSSize::new(100.0, 100.0)) };
            window.setDelegate(Some(ProtocolObject::from_ref(self)));

            // xarkes: create menu bar and add cmd+Q shortcut
            let menubar = NSMenu::new(mtm);
            let app_menu_item = NSMenuItem::new(mtm);
            menubar.addItem(&app_menu_item);
            app.setMainMenu(Some(&menubar));
            let app_menu = NSMenu::new(mtm);
            let quit_title = ns_string!("Quit application");
            let quit_menu_item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    quit_title,
                    Some(sel!(terminate:)),
                    ns_string!("q"),
                )
            };
            app_menu.addItem(&quit_menu_item);
            app_menu_item.setSubmenu(Some(&app_menu));

            // xarkes: create NSView and apply it to window
            let view = unsafe { NSView::initWithFrame(NSView::alloc(mtm), frame_rect) };
            unsafe {
                view.setAutoresizingMask(
                    NSAutoresizingMaskOptions::ViewWidthSizable
                        | NSAutoresizingMaskOptions::ViewHeightSizable,
                );
            }
            window.setContentView(Some(&view));

            // xarkes: show the window
            window.makeKeyAndOrderFront(None);
            window.orderFront(None);

            // xarkes: store the window in the delegate
            self.ivars().window.set(window).unwrap();
            self.ivars().view.set(view).unwrap();

            // xarkes: activate the application, required when launching unbundled
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);

            // xarkes: stop the application such that run() is not blocking, and we can handle events on our own
            app.stop(None);
        }

        #[unsafe(method(windowDidResize:))]
        fn did_resize(&self, _notification: &NSNotification) {
            // TODO(xarkes): we may have to implement our own resize handling due to the way MacOS handles it :') - TL;DR the sendEvent() when a mouse click is in a resize area will run its own eventloop to wait until we release the mouse button. More details here:https://github.com/rust-windowing/winit/issues/219
        }

        #[unsafe(method(applicationDidChangeScreenParameters:))]
        fn did_change_screen_parameters(&self, _notification: &NSNotification) {
            // TODO(xarkes): you may want to update things when display settings are updated
        }

        #[unsafe(method(applicationDidChangeOcclusionState:))]
        fn did_change_occlusion_state(&self, _notification: &NSNotification) {
            // TODO(xarkes): you may want to stop rendering when the window is hidden
        }
    }

    // SAFETY: `NSWindowDelegate` has no safety requirements.
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            // Quit the application when the window is closed.
            unsafe { NSApplication::sharedApplication(self.mtm()).terminate(None) };
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker, width: u32, height: u32) -> Retained<Self> {
        let mut vars = AppDelegateIvars::default();
        vars.width = width;
        vars.height = height;
        let this = Self::alloc(mtm).set_ivars(vars);
        // SAFETY: The signature of `NSObject`'s `init` method is correct.
        unsafe { msg_send![super(this), init] }
    }
}

impl Window {
    pub fn new(width: u32, height: u32) -> Self {
        // xarkes: open the window using cocoa API
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        let delegate = Delegate::new(mtm, width, height);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // NOTE(xarkes): due to our code in `applicationDidFinishLaunching`, run won't be blocking
        app.run();

        let win = delegate.ivars().window.clone();
        Window {
            // app,
            window: win.clone(),
            view: delegate.ivars().view.clone(),
            dpi: win.get().unwrap().backingScaleFactor() as f32,
        }
    }

    /// Translate event location to screen coords
    fn translate_loc(&self, point: NSPoint) -> Point {
        Point::new(point.x as f32, self.get_size().1 - point.y as f32)
    }

    pub fn get_size(&self) -> (f32, f32) {
        let rect = self.view.get().unwrap().frame();
        (rect.size.width as f32, rect.size.height as f32)
    }

    pub fn get_render_size(&mut self) -> (f32, f32) {
        let (w, h) = self.get_size();
        // TODO(xarkes): compute dpi only on screen change
        self.dpi = self.window.get().unwrap().backingScaleFactor() as f32;
        (w * self.dpi, h * self.dpi)
    }

    pub fn get_events(&self) -> Vec<OSEvent> {
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);

        let mut events = Vec::new();
        // SAFETY: TODO
        unsafe {
            loop {
                let event = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                    NSAnyEventMask,
                    Some(&NSDate::distantPast()),
                    NSDefaultRunLoopMode,
                    true,
                );
                if let Some(ev) = event {
                    // xarkes: translate the OS event into a more generic event
                    let new_ev = match ev.r#type() {
                        NSEventType::MouseMoved => Some(OSEvent {
                            ty: OSEventType::MouseMove,
                            key: OSKey::LeftMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            delta: 0.,
                        }),
                        NSEventType::RightMouseDragged => Some(OSEvent {
                            ty: OSEventType::MouseMove,
                            key: OSKey::RightMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            delta: 0.,
                        }),
                        NSEventType::LeftMouseDown => Some(OSEvent {
                            ty: OSEventType::Press,
                            key: OSKey::LeftMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            delta: 0.,
                        }),
                        NSEventType::LeftMouseUp => Some(OSEvent {
                            ty: OSEventType::Release,
                            key: OSKey::LeftMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            delta: 0.,
                        }),
                        NSEventType::RightMouseDown => Some(OSEvent {
                            ty: OSEventType::Press,
                            key: OSKey::RightMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            delta: 0.,
                        }),
                        NSEventType::RightMouseUp => Some(OSEvent {
                            ty: OSEventType::Release,
                            key: OSKey::RightMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            delta: 0.,
                        }),
                        NSEventType::KeyDown => Some(OSEvent {
                            ty: OSEventType::Press,
                            key: macos_keycode_to_oskey(ev.keyCode()),
                            pos: None,
                            chars: ev.characters().unwrap().to_string().chars().nth(0),
                            delta: 0.,
                        }),
                        NSEventType::LeftMouseDragged => Some(OSEvent {
                            ty: OSEventType::MouseMove,
                            key: OSKey::LeftMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            delta: 0.,
                        }),
                        // XXX(xarkes): I think this sucks to use key (LMB/RMB) to differentiate scroll axis
                        // Good enough for now, will likely have to improve after mobile support
                        NSEventType::ScrollWheel => match ev.deltaX() != 0. {
                            true => Some(OSEvent {
                                ty: OSEventType::Scroll,
                                key: OSKey::LeftMouseButton,
                                pos: Some(self.translate_loc(ev.locationInWindow())),
                                chars: None,
                                delta: ev.deltaX() as f32,
                            }),
                            false => Some(OSEvent {
                                ty: OSEventType::Scroll,
                                key: OSKey::RightMouseButton,
                                pos: Some(self.translate_loc(ev.locationInWindow())),
                                chars: None,
                                delta: ev.deltaY() as f32,
                            }),
                        },
                        // NSEventType::FlagsChanged => Some(OSEvent {

                        // }),
                        // NSEventType::AppKitDefined => {
                        //     println!("Unhandled event: {:?}", ev.subtype());
                        //     None
                        // }
                        _ => None,
                    };
                    // if new_ev.is_none() {
                    //     println!("Unhandled event: {:?}", ev);
                    // }
                    // xarkes: send the event to the NSApplication
                    if ev.r#type() != NSEventType::KeyDown && ev.r#type() != NSEventType::KeyUp {
                        app.sendEvent(&ev);
                    }
                    if let Some(new_ev) = new_ev {
                        events.push(new_ev);
                    }
                } else {
                    break;
                }
            }
        }
        events
    }
}

#[repr(C)]
struct MachTimeBaseInfoT {
    denom: u32,
    numer: u32,
}
unsafe extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(t: *const std::ffi::c_void);
}
pub fn timer_init() -> f64 {
    let info = MachTimeBaseInfoT { denom: 0, numer: 0 };
    unsafe { mach_timebase_info(std::ptr::from_ref(&info) as *const _) };
    info.denom as f64 * 1e9 / info.numer as f64
}
pub fn timer_value() -> u64 {
    unsafe { mach_absolute_time() }
}

fn macos_keycode_to_oskey(keycode: u16) -> OSKey {
    // TODO: Support keys with non latin layouts?
    match keycode {
        0x00 => OSKey::Keyboard(OSKeyCode::KeyA),
        0x01 => OSKey::Keyboard(OSKeyCode::KeyS),
        0x02 => OSKey::Keyboard(OSKeyCode::KeyD),
        0x03 => OSKey::Keyboard(OSKeyCode::KeyF),
        0x04 => OSKey::Keyboard(OSKeyCode::KeyH),
        0x05 => OSKey::Keyboard(OSKeyCode::KeyG),
        0x06 => OSKey::Keyboard(OSKeyCode::KeyZ),
        0x07 => OSKey::Keyboard(OSKeyCode::KeyX),
        0x08 => OSKey::Keyboard(OSKeyCode::KeyC),
        0x09 => OSKey::Keyboard(OSKeyCode::KeyV),
        0x0B => OSKey::Keyboard(OSKeyCode::KeyB),
        0x0C => OSKey::Keyboard(OSKeyCode::KeyQ),
        0x0D => OSKey::Keyboard(OSKeyCode::KeyW),
        0x0E => OSKey::Keyboard(OSKeyCode::KeyE),
        0x0F => OSKey::Keyboard(OSKeyCode::KeyR),
        0x10 => OSKey::Keyboard(OSKeyCode::KeyY),
        0x11 => OSKey::Keyboard(OSKeyCode::KeyT),
        0x12 => OSKey::Keyboard(OSKeyCode::Key1),
        0x13 => OSKey::Keyboard(OSKeyCode::Key2),
        0x14 => OSKey::Keyboard(OSKeyCode::Key3),
        0x15 => OSKey::Keyboard(OSKeyCode::Key4),
        0x16 => OSKey::Keyboard(OSKeyCode::Key6),
        0x17 => OSKey::Keyboard(OSKeyCode::Key5),
        0x18 => OSKey::Keyboard(OSKeyCode::KeyEqual),
        0x19 => OSKey::Keyboard(OSKeyCode::Key9),
        0x1A => OSKey::Keyboard(OSKeyCode::Key7),
        0x1B => OSKey::Keyboard(OSKeyCode::KeyMinus),
        0x1C => OSKey::Keyboard(OSKeyCode::Key8),
        0x1D => OSKey::Keyboard(OSKeyCode::Key0),
        0x1E => OSKey::Keyboard(OSKeyCode::KeyRightBracket),
        0x1F => OSKey::Keyboard(OSKeyCode::KeyO),
        0x20 => OSKey::Keyboard(OSKeyCode::KeyU),
        0x21 => OSKey::Keyboard(OSKeyCode::KeyLeftBracket),
        0x22 => OSKey::Keyboard(OSKeyCode::KeyI),
        0x23 => OSKey::Keyboard(OSKeyCode::KeyP),
        0x25 => OSKey::Keyboard(OSKeyCode::KeyL),
        0x26 => OSKey::Keyboard(OSKeyCode::KeyJ),
        0x27 => OSKey::Keyboard(OSKeyCode::KeyApostrophe),
        0x28 => OSKey::Keyboard(OSKeyCode::KeyK),
        0x29 => OSKey::Keyboard(OSKeyCode::KeySemicolon),
        0x2A => OSKey::Keyboard(OSKeyCode::KeyBackslash),
        0x2B => OSKey::Keyboard(OSKeyCode::KeyComma),
        0x2C => OSKey::Keyboard(OSKeyCode::KeySlash),
        0x2D => OSKey::Keyboard(OSKeyCode::KeyN),
        0x2E => OSKey::Keyboard(OSKeyCode::KeyM),
        0x2F => OSKey::Keyboard(OSKeyCode::KeyPeriod),
        0x32 => OSKey::Keyboard(OSKeyCode::KeyGraveAccent),
        0x41 => OSKey::Keyboard(OSKeyCode::KeyKeypadDecimal),
        0x43 => OSKey::Keyboard(OSKeyCode::KeyKeypadMultiply),
        0x45 => OSKey::Keyboard(OSKeyCode::KeyKeypadAdd),
        // 0x47 => OSKey::Keyboard(OSKeyCode::KeyKeypadClear),
        0x4B => OSKey::Keyboard(OSKeyCode::KeyKeypadDivide),
        0x4C => OSKey::Keyboard(OSKeyCode::KeyKeypadEnter),
        0x4E => OSKey::Keyboard(OSKeyCode::KeyKeypadSubtract),
        0x51 => OSKey::Keyboard(OSKeyCode::KeyKeypadEqual),
        0x52 => OSKey::Keyboard(OSKeyCode::KeyKeypad0),
        0x53 => OSKey::Keyboard(OSKeyCode::KeyKeypad1),
        0x54 => OSKey::Keyboard(OSKeyCode::KeyKeypad2),
        0x55 => OSKey::Keyboard(OSKeyCode::KeyKeypad3),
        0x56 => OSKey::Keyboard(OSKeyCode::KeyKeypad4),
        0x57 => OSKey::Keyboard(OSKeyCode::KeyKeypad5),
        0x58 => OSKey::Keyboard(OSKeyCode::KeyKeypad6),
        0x59 => OSKey::Keyboard(OSKeyCode::KeyKeypad7),
        0x5B => OSKey::Keyboard(OSKeyCode::KeyKeypad8),
        0x5C => OSKey::Keyboard(OSKeyCode::KeyKeypad9),
        0x24 => OSKey::Keyboard(OSKeyCode::KeyEnter),
        0x30 => OSKey::Keyboard(OSKeyCode::KeyTab),
        0x31 => OSKey::Keyboard(OSKeyCode::KeySpace),
        0x33 => OSKey::Keyboard(OSKeyCode::KeyBackspace),
        0x35 => OSKey::Keyboard(OSKeyCode::KeyEscape),
        // 0x37 => OSKey::Keyboard(OSKeyCode::KeyCommand),
        0x38 => OSKey::Keyboard(OSKeyCode::KeyLeftShift),
        0x39 => OSKey::Keyboard(OSKeyCode::KeyCapsLock),
        // 0x3A => OSKey::Keyboard(OSKeyCode::KeyOption),
        // 0x3B => OSKey::Keyboard(OSKeyCode::KeyControl),
        0x3B => OSKey::Keyboard(OSKeyCode::KeyLeftAlt),
        0x3C => OSKey::Keyboard(OSKeyCode::KeyRightShift),
        // 0x3D => OSKey::Keyboard(OSKeyCode::KeyRightOption),
        // 0x3E => OSKey::Keyboard(OSKeyCode::KeyRightControl),
        0x3E => OSKey::Keyboard(OSKeyCode::KeyRightAlt),
        // 0x3F => OSKey::Keyboard(OSKeyCode::KeyFunction),
        0x40 => OSKey::Keyboard(OSKeyCode::KeyF17),
        // 0x48 => OSKey::Keyboard(OSKeyCode::KeyVolumeUp),
        // 0x49 => OSKey::Keyboard(OSKeyCode::KeyVolumeDown),
        // 0x4A => OSKey::Keyboard(OSKeyCode::KeyMute),
        0x4F => OSKey::Keyboard(OSKeyCode::KeyF18),
        0x50 => OSKey::Keyboard(OSKeyCode::KeyF19),
        0x5A => OSKey::Keyboard(OSKeyCode::KeyF20),
        0x60 => OSKey::Keyboard(OSKeyCode::KeyF5),
        0x61 => OSKey::Keyboard(OSKeyCode::KeyF6),
        0x62 => OSKey::Keyboard(OSKeyCode::KeyF7),
        0x63 => OSKey::Keyboard(OSKeyCode::KeyF3),
        0x64 => OSKey::Keyboard(OSKeyCode::KeyF8),
        0x65 => OSKey::Keyboard(OSKeyCode::KeyF9),
        0x67 => OSKey::Keyboard(OSKeyCode::KeyF11),
        0x69 => OSKey::Keyboard(OSKeyCode::KeyF13),
        0x6A => OSKey::Keyboard(OSKeyCode::KeyF16),
        0x6B => OSKey::Keyboard(OSKeyCode::KeyF14),
        0x6D => OSKey::Keyboard(OSKeyCode::KeyF10),
        0x6E => OSKey::Keyboard(OSKeyCode::KeyMenu),
        0x6F => OSKey::Keyboard(OSKeyCode::KeyF12),
        0x71 => OSKey::Keyboard(OSKeyCode::KeyF15),
        // 0x72 => OSKey::Keyboard(OSKeyCode::KeyHelp),
        0x73 => OSKey::Keyboard(OSKeyCode::KeyHome),
        0x74 => OSKey::Keyboard(OSKeyCode::KeyPageUp),
        0x75 => OSKey::Keyboard(OSKeyCode::KeyDelete),
        0x76 => OSKey::Keyboard(OSKeyCode::KeyF4),
        0x77 => OSKey::Keyboard(OSKeyCode::KeyEnd),
        0x78 => OSKey::Keyboard(OSKeyCode::KeyF2),
        0x79 => OSKey::Keyboard(OSKeyCode::KeyPageDown),
        0x7A => OSKey::Keyboard(OSKeyCode::KeyF1),
        0x7B => OSKey::Keyboard(OSKeyCode::KeyLeftArrow),
        0x7C => OSKey::Keyboard(OSKeyCode::KeyRightArrow),
        0x7D => OSKey::Keyboard(OSKeyCode::KeyDownArrow),
        0x7E => OSKey::Keyboard(OSKeyCode::KeyUpArrow),
        _ => {
            println!("Warning: keyboard key not handled: {:?}", keycode);
            OSKey::Keyboard(OSKeyCode::KeyA)
        }
    }
}
