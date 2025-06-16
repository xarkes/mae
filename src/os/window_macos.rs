extern crate objc2;

//#![deny(unsafe_op_in_unsafe_fn)]
use std::cell::OnceCell;

use objc2::{
    DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
    sel,
};
use objc2_app_kit::{
    NSAnyEventMask, NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSBackingStoreType, NSMenu, NSMenuItem, NSView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSDate, NSDefaultRunLoopMode, NSNotification, NSObject, NSObjectProtocol,
    NSPoint, NSRect, NSSize, ns_string,
};

#[derive(Debug, Default)]
struct AppDelegateIvars {
    window: OnceCell<Retained<NSWindow>>,
    view: OnceCell<Retained<NSView>>,
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
            let frame_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(300.0, 300.0));
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

            // set various window properties
            window.setTitle(ns_string!("A window"));
            window.center();
            unsafe { window.setContentMinSize(NSSize::new(300.0, 300.0)) };
            window.setDelegate(Some(ProtocolObject::from_ref(self)));

            // create menu bar and add shortcuts
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

            // create NSView and apply it to window
            let view = unsafe { NSView::initWithFrame(NSView::alloc(mtm), frame_rect) };
            window.setContentView(Some(&view));

            // show the window
            window.makeKeyAndOrderFront(None);
            window.orderFront(None);

            // store the window in the delegate
            self.ivars().window.set(window).unwrap();
            self.ivars().view.set(view).unwrap();

            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

            // activate the application, required when launching unbundled
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);

            // xarkes: stop the application such that run() is not blocking, and we can handle events on our own
            app.stop(None);
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
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppDelegateIvars::default());
        // SAFETY: The signature of `NSObject`'s `init` method is correct.
        unsafe { msg_send![super(this), init] }
    }
}

impl Window {
    pub fn new() -> Self {
        // xarkes: open the window OS side
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        let delegate = Delegate::new(mtm);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // xarkes: call run once - due to our code in "applicationDidFinishLaunching", this won't be blocking
        app.run();

        Window {
            app,
            window: delegate.ivars().window.clone(),
            view: delegate.ivars().view.clone(),
        }
    }

    pub fn get_events(&self) {
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: TODO
        unsafe {
            let event = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSAnyEventMask,
                Some(&NSDate::distantFuture()),
                NSDefaultRunLoopMode,
                true,
            );
            if let Some(ev) = event {
                println!("Got event: {:?}", ev);
                app.sendEvent(&ev);
            }
        }
    }
}
