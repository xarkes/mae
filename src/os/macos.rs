extern crate objc2;

use crate::{imui::Point, os::OSCursor};

use super::{OSEvent, OSEventFlag, OSEventType, OSKey, OSKeyCode};
use std::cell::{Cell, OnceCell};

use objc2::{
    AnyThread, ClassType, DefinedClass, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSApplicationTerminateReply, NSAutoresizingMaskOptions, NSBackingStoreType, NSCursor, NSEvent,
    NSEventMask, NSEventModifierFlags, NSEventType, NSImage, NSMenu, NSMenuItem, NSModalResponseOK,
    NSOpenPanel, NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF,
    NSTextInputClient, NSView, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSAttributedString, NSAttributedStringKey, NSData, NSDate,
    NSDefaultRunLoopMode, NSNotFound, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRange,
    NSRangePointer, NSRect, NSSize, NSString, NSUInteger, ns_string,
};
use std::cell::RefCell;

thread_local! {
    /// Set when AppKit wants to terminate (Cmd+Q menu shortcut, window close). We route
    /// this through our own event loop as a `Quit` event instead of letting AppKit
    /// hard-exit, so the application can shut down gracefully (e.g. flush unsaved state).
    static GRACEFUL_QUIT: Cell<bool> = const { Cell::new(false) };
}

fn request_graceful_quit() {
    GRACEFUL_QUIT.with(|flag| flag.set(true));
}

fn take_graceful_quit() -> bool {
    GRACEFUL_QUIT.with(|flag| flag.replace(false))
}

/// Extract a plain Rust string from an `insertText:`/`setMarkedText:` argument,
/// which AppKit passes as either an `NSString` or `NSAttributedString`.
fn extract_string(obj: &AnyObject) -> String {
    if let Some(att) = obj.downcast_ref::<NSAttributedString>() {
        return att.string().to_string();
    }
    if let Some(s) = obj.downcast_ref::<NSString>() {
        return s.to_string();
    }
    String::new()
}

struct InputViewIvars {
    /// Text committed by the IME (insertText:), drained into key events each frame.
    pending_text: RefCell<String>,
    /// The current marked (preedit / composing) string; empty when not composing.
    marked_text: RefCell<String>,
    /// Set when marked text changes so the UI can repaint the preedit.
    marked_dirty: Cell<bool>,
    /// Caret rectangle in screen coordinates, reported back to AppKit so the IME
    /// candidate window appears next to the caret.
    caret_rect: Cell<NSRect>,
}

impl Default for InputViewIvars {
    fn default() -> Self {
        Self {
            pending_text: RefCell::new(String::new()),
            marked_text: RefCell::new(String::new()),
            marked_dirty: Cell::new(false),
            caret_rect: Cell::new(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0))),
        }
    }
}

define_class!(
    // SAFETY:
    // - The superclass NSView has no subclassing requirements violated here.
    // - `InputView` does not implement `Drop`.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = InputViewIvars]
    struct InputView;

    impl InputView {
        // Must be first responder for the text input context to deliver IME events.
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }
    }

    // SAFETY: method signatures match the NSTextInputClient protocol.
    unsafe impl NSTextInputClient for InputView {
        #[unsafe(method(insertText:replacementRange:))]
        unsafe fn insert_text(&self, string: &AnyObject, _replacement: NSRange) {
            let text = extract_string(string);
            self.ivars().pending_text.borrow_mut().push_str(&text);
            // Committing ends any composition.
            self.ivars().marked_text.borrow_mut().clear();
            self.ivars().marked_dirty.set(true);
        }

        #[unsafe(method(doCommandBySelector:))]
        unsafe fn do_command_by_selector(&self, _selector: Sel) {
            // Navigation/editing commands are surfaced via key-code events instead.
        }

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        unsafe fn set_marked_text(
            &self,
            string: &AnyObject,
            _selected: NSRange,
            _replacement: NSRange,
        ) {
            *self.ivars().marked_text.borrow_mut() = extract_string(string);
            self.ivars().marked_dirty.set(true);
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            self.ivars().marked_text.borrow_mut().clear();
            self.ivars().marked_dirty.set(true);
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            NSRange::new(0, 0)
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            let len = self.ivars().marked_text.borrow().encode_utf16().count();
            if len == 0 {
                NSRange::new(NSNotFound as NSUInteger, 0)
            } else {
                NSRange::new(0, len)
            }
        }

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            !self.ivars().marked_text.borrow().is_empty()
        }

        // Returns objects as raw autoreleased pointers: objc2's `define_class!`
        // only accepts `EncodeReturn` returns (not `Retained`) for non-init methods.
        #[unsafe(method(attributedSubstringForProposedRange:actualRange:))]
        unsafe fn attributed_substring(
            &self,
            _range: NSRange,
            _actual: NSRangePointer,
        ) -> *mut NSAttributedString {
            std::ptr::null_mut()
        }

        #[unsafe(method(validAttributesForMarkedText))]
        fn valid_attributes(&self) -> *mut NSArray<NSAttributedStringKey> {
            Retained::autorelease_ptr(NSArray::new())
        }

        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        unsafe fn first_rect(&self, _range: NSRange, _actual: NSRangePointer) -> NSRect {
            self.ivars().caret_rect.get()
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index_for_point(&self, _point: NSPoint) -> NSUInteger {
            0
        }
    }
);

impl InputView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(InputViewIvars::default());
        // SAFETY: NSView's designated initializer.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

#[derive(Default)]
struct AppDelegateIvars {
    window: OnceCell<Retained<NSWindow>>,
    view: OnceCell<Retained<NSView>>,
    input_view: OnceCell<Retained<InputView>>,
    width: u32,
    height: u32,
}

pub struct Window {
    pub window: OnceCell<Retained<NSWindow>>,
    pub view: OnceCell<Retained<NSView>>,
    /// The same view as `view`, kept concretely typed for IME (NSTextInputClient) state.
    input_view: OnceCell<Retained<InputView>>,
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
        // SAFETY: TODO: XXX: Need to review all this file
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, notification: &NSNotification) {
            let mtm = self.mtm();

            let app = notification
                .object()
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
            window.setContentMinSize(NSSize::new(100.0, 100.0));
            window.setDelegate(Some(ProtocolObject::from_ref(self)));
            window.setOpaque(true);

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

            // xarkes: create our text-input-capable NSView subclass and apply it.
            let view = InputView::new(mtm, frame_rect);
            view.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            window.setContentView(Some(&view));
            // Make the view first responder so the text input context (IME) is active.
            window.makeFirstResponder(Some(&view));

            // xarkes: show the window
            window.makeKeyAndOrderFront(None);
            window.orderFront(None);

            // xarkes: store the window in the delegate. `view` is kept both as the
            // concrete InputView (for IME) and upcast to NSView (for the renderer).
            self.ivars().window.set(window).unwrap();
            self.ivars().view.set(view.clone().into_super()).unwrap();
            let _ = self.ivars().input_view.set(view);

            // xarkes: activate the application, required when launching unbundled
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);

            // xarkes: stop the application such that run() is not blocking, and we can handle events on our own
            app.stop(None);
        }

        #[unsafe(method(applicationDidChangeScreenParameters:))]
        fn did_change_screen_parameters(&self, _notification: &NSNotification) {
            // TODO(xarkes): you may want to update things when display settings are updated
        }

        #[unsafe(method(applicationDidChangeOcclusionState:))]
        fn did_change_occlusion_state(&self, _notification: &NSNotification) {
            // TODO(xarkes): you may want to stop rendering when the window is hidden
        }

        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self, _app: &NSApplication) -> NSApplicationTerminateReply {
            // Cancel AppKit's immediate termination and request a graceful quit through
            // our event loop instead, so the app's normal shutdown (flush) path runs.
            request_graceful_quit();
            NSApplicationTerminateReply::TerminateCancel
        }
    }

    // SAFETY: `NSWindowDelegate` has no safety requirements.
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowDidResize:))]
        fn did_resize(&self, _notification: &NSNotification) {
            // TODO(xarkes): we may have to implement our own resize handling due to the way MacOS handles it :') - TL;DR the sendEvent() when a mouse click is in a resize area will run its own eventloop to wait until we release the mouse button. More details here:https://github.com/rust-windowing/winit/issues/219
        }

        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            // Quit the application gracefully when the window is closed (see
            // `applicationShouldTerminate:` / `GRACEFUL_QUIT`).
            request_graceful_quit();
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
    pub fn new(width: u32, height: u32, title: &str) -> Self {
        // xarkes: open the window using cocoa API
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        let delegate = Delegate::new(mtm, width, height);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // NOTE(xarkes): due to our code in `applicationDidFinishLaunching`, run won't be blocking
        app.run();

        let win = delegate.ivars().window.clone();
        let oswindow = Window {
            // app,
            window: win.clone(),
            view: delegate.ivars().view.clone(),
            input_view: delegate.ivars().input_view.clone(),
            dpi: win.get().unwrap().backingScaleFactor() as f32,
        };
        oswindow.set_title(title);
        oswindow
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

    pub fn refresh_rate_hz(&self) -> f32 {
        unsafe {
            let screen: *mut objc2::runtime::AnyObject =
                msg_send![Retained::as_ptr(self.window.get().unwrap()), screen];
            if screen.is_null() {
                return 60.0;
            }

            let rate: isize = msg_send![screen, maximumFramesPerSecond];
            if rate > 1 { rate as f32 } else { 60.0 }
        }
    }

    /// Set the window's title bar text.
    pub fn set_title(&self, title: &str) {
        if let Some(window) = self.window.get() {
            window.setTitle(&NSString::from_str(title));
        }
    }

    /// Set the application icon (shown in the Dock and app switcher) from PNG bytes.
    pub fn set_app_icon(&self, png_bytes: &[u8]) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let data = NSData::with_bytes(png_bytes);
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            return;
        };
        // Constrain the logical size to a standard icon size. Without this, AppKit
        // processes the Dock icon at the source bitmap's native resolution, which for a
        // large asset (e.g. 1254x1254) makes setApplicationIconImage take several seconds.
        image.setSize(NSSize::new(256.0, 256.0));
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: `image` is a valid NSImage retained for the duration of the call.
        unsafe { app.setApplicationIconImage(Some(&image)) };
    }

    /// Set the mouse cursor shape
    pub fn set_cursor(&mut self, cursor: OSCursor) {
        let cursor = match cursor {
            OSCursor::Arrow => NSCursor::arrowCursor(),
            OSCursor::IBeam => NSCursor::IBeamCursor(),
            OSCursor::Hand => NSCursor::pointingHandCursor(),
            // xarkes: Older APIs, keeping now for reference, maybe useful for backporting
            // OSCursor::ResizeH => NSCursor::resizeLeftRightCursor(),
            // OSCursor::ResizeV => NSCursor::resizeUpDownCursor(),
            OSCursor::ResizeH => NSCursor::rowResizeCursor(),
            OSCursor::ResizeV => NSCursor::columnResizeCursor(),
            OSCursor::ResizeNWSE => {
                // AppKit has no public diagonal-resize cursor; use the long-
                // stable private class method, falling back to the arrow.
                let cls = NSCursor::class();
                let sel = sel!(_windowResizeNorthWestSouthEastCursor);
                let responds: bool = unsafe { msg_send![cls, respondsToSelector: sel] };
                let diagonal: Option<Retained<NSCursor>> = if responds {
                    Some(unsafe { msg_send![cls, _windowResizeNorthWestSouthEastCursor] })
                } else {
                    None
                };
                diagonal.unwrap_or_else(NSCursor::arrowCursor)
            }
        };
        cursor.set();
    }

    pub fn get_events(&self) -> Vec<OSEvent> {
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);

        let mut events = Vec::new();
        // SAFETY: TODO
        unsafe {
            loop {
                let event = app.nextEventMatchingMask_untilDate_inMode_dequeue(
                    NSEventMask::Any,
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
                            deltax: 0.,
                            deltay: 0.,
                            flags: None,
                        }),
                        NSEventType::RightMouseDragged => Some(OSEvent {
                            ty: OSEventType::MouseMove,
                            key: OSKey::RightMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            deltax: 0.,
                            deltay: 0.,
                            flags: None,
                        }),
                        NSEventType::LeftMouseDown => Some(OSEvent {
                            ty: OSEventType::Press,
                            key: OSKey::LeftMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            deltax: 0.,
                            deltay: 0.,
                            flags: None,
                        }),
                        NSEventType::LeftMouseUp => Some(OSEvent {
                            ty: OSEventType::Release,
                            key: OSKey::LeftMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            deltax: 0.,
                            deltay: 0.,
                            flags: None,
                        }),
                        NSEventType::RightMouseDown => Some(OSEvent {
                            ty: OSEventType::Press,
                            key: OSKey::RightMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            deltax: 0.,
                            deltay: 0.,
                            flags: None,
                        }),
                        NSEventType::RightMouseUp => Some(OSEvent {
                            ty: OSEventType::Release,
                            key: OSKey::RightMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            deltax: 0.,
                            deltay: 0.,
                            flags: None,
                        }),
                        NSEventType::KeyDown => self.handle_keydown_ime(&ev),
                        NSEventType::LeftMouseDragged => Some(OSEvent {
                            ty: OSEventType::MouseMove,
                            key: OSKey::LeftMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            deltax: 0.,
                            deltay: 0.,
                            flags: None,
                        }),
                        NSEventType::ScrollWheel => Some(OSEvent {
                            ty: OSEventType::Scroll,
                            key: OSKey::LeftMouseButton,
                            pos: Some(self.translate_loc(ev.locationInWindow())),
                            chars: None,
                            deltax: ev.deltaX() as f32,
                            deltay: ev.deltaY() as f32,
                            flags: macos_keyflag_to_osflag(ev.modifierFlags()),
                        }),
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
                    // Key events are handled by our own UI rather than the AppKit
                    // responder chain (which would beep on unhandled keys), so they are
                    // not forwarded via sendEvent. But menu key equivalents (e.g. Cmd+Q)
                    // are dispatched by AppKit, so offer key-downs to the main menu first;
                    // if it consumes the shortcut, don't also surface it to the app.
                    let mut consumed_by_menu = false;
                    if ev.r#type() == NSEventType::KeyDown
                        && let Some(menu) = app.mainMenu()
                    {
                        consumed_by_menu = menu.performKeyEquivalent(&ev);
                    }

                    if ev.r#type() != NSEventType::KeyDown && ev.r#type() != NSEventType::KeyUp {
                        app.sendEvent(&ev);
                    }
                    if !consumed_by_menu && let Some(new_ev) = new_ev {
                        events.push(new_ev);
                    }
                } else {
                    break;
                }
            }
        }
        // Drain IME-committed text (insertText:) into per-char key events, reusing the
        // existing `chars` insertion path. A preedit (marked text) change with no commit
        // still needs a repaint so the composing text is drawn.
        if let Some(view) = self.input_view.get() {
            let committed = std::mem::take(&mut *view.ivars().pending_text.borrow_mut());
            for ch in committed.chars() {
                events.push(OSEvent {
                    ty: OSEventType::Press,
                    key: OSKey::Keyboard(OSKeyCode::KeySpace),
                    pos: None,
                    chars: Some(ch),
                    deltax: 0.,
                    deltay: 0.,
                    flags: None,
                });
            }
            if view.ivars().marked_dirty.replace(false) {
                events.push(OSEvent {
                    ty: OSEventType::Repaint,
                    key: OSKey::LeftMouseButton,
                    pos: None,
                    chars: None,
                    deltax: 0.,
                    deltay: 0.,
                    flags: None,
                });
            }
        }

        // A pending AppKit termination (Cmd+Q / window close) surfaces as a Quit event so
        // the event loop can break and run the application's shutdown path.
        if take_graceful_quit() {
            events.push(OSEvent::quit());
        }
        events
    }

    /// Route a key-down through the text input context (IME). Committed text and
    /// preedit are captured on the view's ivars; returns a key-code event only when
    /// the IME consumed the key as neither text nor composition (i.e. navigation /
    /// shortcut), so the app's own handling still works.
    fn handle_keydown_ime(&self, ev: &NSEvent) -> Option<OSEvent> {
        let view = self.input_view.get();
        let snapshot = |v: &InputView| {
            (
                !v.ivars().marked_text.borrow().is_empty(),
                v.ivars().pending_text.borrow().len(),
            )
        };
        let (was_composing, pending_before) = view.map_or((false, 0), |v| snapshot(v));
        if let Some(v) = view
            && let Some(ctx) = v.inputContext()
        {
            ctx.handleEvent(ev);
        }
        let (now_composing, pending_after) = view.map_or((false, 0), |v| snapshot(v));

        if !was_composing && !now_composing && pending_after == pending_before {
            Some(OSEvent {
                ty: OSEventType::Press,
                key: macos_keycode_to_oskey(ev.keyCode()),
                pos: None,
                chars: None,
                deltax: 0.,
                deltay: 0.,
                flags: macos_keyflag_to_osflag(ev.modifierFlags()),
            })
        } else {
            None
        }
    }

    /// The current IME preedit (composing) string, if any. Rendered inline by the
    /// focused text editor; not part of the committed buffer.
    pub fn ime_preedit(&self) -> Option<String> {
        let text = self.input_view.get()?.ivars().marked_text.borrow();
        if text.is_empty() {
            None
        } else {
            Some(text.clone())
        }
    }

    /// Report the focused caret's rectangle in screen coordinates so the IME places
    /// its candidate window next to the caret.
    pub fn set_ime_caret_rect(&self, x: f32, y: f32, width: f32, height: f32) {
        if let Some(view) = self.input_view.get() {
            view.ivars().caret_rect.set(NSRect::new(
                NSPoint::new(x as f64, y as f64),
                NSSize::new(width as f64, height as f64),
            ));
        }
    }
}

unsafe extern "C" {
    fn clock_gettime_nsec_np(t: u64) -> u64;
}
pub fn timer_init() -> f64 {
    1_000_000_000.
}
pub fn timer_value() -> u64 {
    let _clock_monotonic_raw = 4;
    unsafe { clock_gettime_nsec_np(_clock_monotonic_raw) }
}

/// Write `text` to the system clipboard (the general `NSPasteboard`), replacing any
/// previous contents.
pub fn clipboard_set(text: &str) {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: `NSPasteboardTypeString` is a framework-provided constant string.
    let string_type = unsafe { NSPasteboardTypeString };
    pasteboard.setString_forType(&NSString::from_str(text), string_type);
}

/// Read the system clipboard's plain-text contents, if it currently holds a string.
pub fn clipboard_get() -> Option<String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    // SAFETY: `NSPasteboardTypeString` is a framework-provided constant string.
    let string_type = unsafe { NSPasteboardTypeString };
    pasteboard.stringForType(string_type).map(|s| s.to_string())
}

/// Read an image off the clipboard as encoded bytes: PNG if present, otherwise
/// TIFF (macOS screenshots / "Copy Image" usually land as TIFF). The caller
/// decodes via the `image` crate, which detects the format from the bytes.
pub fn clipboard_get_image() -> Option<Vec<u8>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    // SAFETY: both are framework-provided constant pasteboard-type strings.
    let png_type = unsafe { NSPasteboardTypePNG };
    if let Some(data) = pasteboard.dataForType(png_type) {
        return Some(data.to_vec());
    }
    let tiff_type = unsafe { NSPasteboardTypeTIFF };
    pasteboard.dataForType(tiff_type).map(|data| data.to_vec())
}

/// Show a native open panel to pick a single image file. Returns its path, or
/// `None` if the user cancelled.
pub fn open_image_file_dialog() -> Option<std::path::PathBuf> {
    let mtm = MainThreadMarker::new()?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    // The file extension is validated by the caller after selection (kept simple
    // to avoid the UTType allowed-content-types dance).
    let response = panel.runModal();
    if response != NSModalResponseOK {
        return None;
    }
    let url = panel.URL()?;
    let path = url.path()?;
    Some(std::path::PathBuf::from(path.to_string()))
}

fn macos_keyflag_to_osflag(flag: NSEventModifierFlags) -> Option<OSEventFlag> {
    let mut out = 0i32;
    if flag.contains(NSEventModifierFlags::Control) {
        out |= OSEventFlag::Control as i32;
    }
    if flag.contains(NSEventModifierFlags::Shift) {
        out |= OSEventFlag::Shift as i32;
    }
    if flag.contains(NSEventModifierFlags::Option) {
        out |= OSEventFlag::Alt as i32;
    }
    if flag.contains(NSEventModifierFlags::Command) {
        out |= OSEventFlag::Super as i32;
    }
    OSEventFlag::try_from(out).ok()
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
