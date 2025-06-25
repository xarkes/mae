extern crate objc2;

//#![deny(unsafe_op_in_unsafe_fn)]
use std::cell::OnceCell;

use objc2::{
    DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
    sel,
};
use objc2_app_kit::{
    NSAnyEventMask, NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSAutoresizingMaskOptions, NSBackingStoreType, NSEventType, NSMenu, NSMenuItem, NSView,
    NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSDate, NSDefaultRunLoopMode, NSNotification, NSObject, NSObjectProtocol,
    NSPoint, NSRect, NSSize, ns_string,
};

#[derive(Debug, Default)]
struct AppDelegateIvars {
    window: OnceCell<Retained<NSWindow>>,
    view: OnceCell<Retained<NSView>>,
    width: u16,
    height: u16,
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
            // TODO(xarkes): Should we send an event to notify the renderer of some update?
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
    fn new(mtm: MainThreadMarker, width: u16, height: u16) -> Retained<Self> {
        let mut vars = AppDelegateIvars::default();
        vars.width = width;
        vars.height = height;
        let this = Self::alloc(mtm).set_ivars(vars);
        // SAFETY: The signature of `NSObject`'s `init` method is correct.
        unsafe { msg_send![super(this), init] }
    }
}

impl Window {
    pub fn new(width: u16, height: u16) -> Self {
        // xarkes: open the window using cocoa API
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        let delegate = Delegate::new(mtm, width, height);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // NOTE(xarkes): due to our code in `applicationDidFinishLaunching`, run won't be blocking
        app.run();

        Window {
            app,
            window: delegate.ivars().window.clone(),
            view: delegate.ivars().view.clone(),
        }
    }

    pub fn get_size(&self) -> (f32, f32) {
        let rect = self.view.get().unwrap().frame();
        (rect.size.width as f32, rect.size.height as f32)
    }

    pub fn get_events(&self) -> Vec<WindowEvent> {
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
                    // xarkes: send the event to the NSApplication
                    app.sendEvent(&ev);
                    // xarkes: translate the OS event into a more generic event
                    let mut new_ev = WindowEvent {
                        ty: WindowEventType::Unknown,
                        data0: 0.0,
                        data1: 0.0,
                    };
                    match ev.r#type() {
                        NSEventType::MouseMoved => {
                            new_ev.ty = WindowEventType::MouseMove;
                            let point = ev.locationInWindow();
                            new_ev.data0 = point.x as f32;
                            new_ev.data1 = self.get_size().1 - point.y as f32;
                        }
                        _ => {}
                    }
                    events.push(new_ev);
                } else {
                    break;
                }
            }
        }
        events
    }
}

// XXX(xarkes): Not window related, but lazy to add another file
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
