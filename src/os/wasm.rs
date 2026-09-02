//! Browser windowing/event backend for the DOM render path (`feature = "dom"`,
//! `target_arch = "wasm32"`). Unlike the native platforms, there is no GPU
//! context here: `Window` exists purely to own the container element, bridge
//! browser input into the shared `OSEvent` stream, and track viewport size —
//! the same narrow contract every other `os/*.rs` implements (see
//! `src/imui/lifecycle.rs`'s `os_window`/`os_window_mut` helpers).

use super::{OSCursor, OSEvent, OSEventFlag, OSEventType, OSKey, OSKeyCode, Point};
use crate::imui::RepaintWaker;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Element, EventTarget, HtmlElement, KeyboardEvent, PointerEvent, WheelEvent};

pub struct Window {
    container: HtmlElement,
    size: (f32, f32),
    pub dpi: f32,
    events: Rc<RefCell<VecDeque<OSEvent>>>,
    // Kept alive for the container's lifetime; dropping these would detach the
    // listeners.
    _on_pointer_down: Closure<dyn FnMut(PointerEvent)>,
    _on_pointer_up: Closure<dyn FnMut(PointerEvent)>,
    _on_pointer_cancel: Closure<dyn FnMut(PointerEvent)>,
    _on_pointer_move: Closure<dyn FnMut(PointerEvent)>,
    _on_wheel: Closure<dyn FnMut(WheelEvent)>,
    _on_key_down: Closure<dyn FnMut(KeyboardEvent)>,
    _on_key_up: Closure<dyn FnMut(KeyboardEvent)>,
    _on_resize: Closure<dyn FnMut(js_sys::Array)>,
    _resize_observer: web_sys::ResizeObserver,
    /// `None` where `window.visualViewport` is unavailable — see
    /// `visual_viewport_height`.
    _on_visual_viewport: Option<Closure<dyn FnMut(web_sys::Event)>>,
}

fn container_pos(container: &Element, client_x: f64, client_y: f64) -> Point {
    let rect = container.get_bounding_client_rect();
    Point::new(
        (client_x - rect.left()) as f32,
        (client_y - rect.top()) as f32,
    )
}

/// How far a finger must travel before its press is reinterpreted as a
/// scroll rather than a tap. Without a threshold the wobble in an ordinary
/// tap would scroll the list out from under whatever was being tapped.
const TOUCH_SLOP: f32 = 8.0;

/// Pixels of content per unit of `OSEvent::scroll` delta —
/// `apply_scroll_signal` (`imui/widgets.rs`) multiplies by exactly this, so
/// dividing by it here makes the content track the finger 1:1, which is the
/// only ratio a drag-to-scroll gesture can have.
const SCROLL_PX_PER_UNIT: f32 = 16.0;

/// A one-finger drag on a touch screen, tracked across the pointer listeners.
///
/// mae scrolls by transforming a wrapper inside a clipped box
/// (`imui/paint_dom.rs`'s `ensure_scroll_wrapper`) rather than by giving the
/// browser a real scroller to drive, and scroll input was wheel-only — so
/// before this, nothing mae drew could be scrolled by finger at all. Rather
/// than add a second scrolling mechanism, a qualifying drag is turned into
/// the `OSEvent::scroll` stream the wheel already produces, and the whole
/// existing path (`imui/scroll.rs`'s `absorb_pending_scroll_for_box`, which
/// picks the scrollable box under the pointer and chains to its ancestors)
/// handles it unchanged.
#[derive(Default, Clone, Copy)]
struct TouchPan {
    /// Where the finger went down, in container coordinates. `None` when no
    /// touch is down — a mouse never sets this, so a mouse drag keeps
    /// selecting text and dragging splitters exactly as before.
    origin: Option<Point>,
    /// The previous position, for per-move deltas.
    last: Point,
    /// Past `TOUCH_SLOP`: this gesture is a scroll now, and the press it
    /// began life as has already been cancelled.
    scrolling: bool,
}

impl TouchPan {
    /// End the gesture, returning what it had become. Called from both
    /// `pointerup` and `pointercancel`, which are the only two ways a touch
    /// can finish.
    fn take_ending(&mut self) -> TouchPan {
        std::mem::take(self)
    }
}

/// Advance a touch drag, queueing scroll events for it. Returns `true` when
/// the move was consumed as a scroll and must not also be reported as a
/// pointer move — a drag cannot be a scroll *and* a text selection.
///
/// See [`TouchPan`] for why scrolling is expressed as `OSEvent::scroll` at
/// all rather than as its own mechanism.
fn touch_pan_move(
    pan: &Rc<RefCell<TouchPan>>,
    events: &Rc<RefCell<VecDeque<OSEvent>>>,
    pos: Point,
) -> bool {
    let mut pan = pan.borrow_mut();
    let Some(origin) = pan.origin else {
        return false; // a mouse, or no button down: nothing to reinterpret
    };
    if !pan.scrolling {
        let (dx, dy) = (pos.x() - origin.x(), pos.y() - origin.y());
        if (dx * dx + dy * dy).sqrt() < TOUCH_SLOP {
            pan.last = pos;
            return false;
        }
        pan.scrolling = true;
        // Cancel the press this gesture began as, *outside* every box: a
        // release within bounds is how `signal_from_key_and_flags` recognises
        // a click, so releasing at the origin would activate whatever the
        // finger came down on. Far outside, it only clears the active key —
        // which is the point, since an in-progress drag (a scrollbar thumb, a
        // text selection) must not keep tracking a finger that is now
        // scrolling.
        events.borrow_mut().push_back(OSEvent::release(
            OSKey::LeftMouseButton,
            Some(Point::new(-10000.0, -10000.0)),
        ));
    }
    // Content follows the finger: dragging down reveals what is above, which
    // is a *decrease* in scroll offset, and `apply_scroll_signal` subtracts.
    let delta = (pos.y() - pan.last.y()) / SCROLL_PX_PER_UNIT;
    pan.last = pos;
    if delta != 0.0 {
        events.borrow_mut().push_back(OSEvent::scroll(pos, delta));
    }
    true
}

fn pointer_button(e: &PointerEvent) -> Option<OSKey> {
    match e.button() {
        0 => Some(OSKey::LeftMouseButton),
        2 => Some(OSKey::RightMouseButton),
        _ => None,
    }
}

fn web_modifiers(shift: bool, ctrl: bool, alt: bool, meta: bool) -> Option<OSEventFlag> {
    let base = match (ctrl, alt, shift) {
        (true, true, true) => Some(OSEventFlag::ControlAltShift),
        (true, true, false) => Some(OSEventFlag::ControlAlt),
        (true, false, true) => Some(OSEventFlag::ControlShift),
        (true, false, false) => Some(OSEventFlag::Control),
        (false, true, true) => Some(OSEventFlag::AltShift),
        (false, true, false) => Some(OSEventFlag::Alt),
        (false, false, true) => Some(OSEventFlag::Shift),
        (false, false, false) => None,
    };
    if !meta {
        return base;
    }
    match base {
        Some(flags) => Some(flags.with(OSEventFlag::Super)),
        None => Some(OSEventFlag::Super),
    }
}

/// Physical-key `KeyboardEvent.code()` string to `OSKeyCode`. Not exhaustive —
/// covers the same practical breadth as `os/linux.rs`'s keysym table.
fn web_code_to_oskeycode(code: &str) -> Option<OSKeyCode> {
    use OSKeyCode::*;
    Some(match code {
        "KeyA" => KeyA,
        "KeyB" => KeyB,
        "KeyC" => KeyC,
        "KeyD" => KeyD,
        "KeyE" => KeyE,
        "KeyF" => KeyF,
        "KeyG" => KeyG,
        "KeyH" => KeyH,
        "KeyI" => KeyI,
        "KeyJ" => KeyJ,
        "KeyK" => KeyK,
        "KeyL" => KeyL,
        "KeyM" => KeyM,
        "KeyN" => KeyN,
        "KeyO" => KeyO,
        "KeyP" => KeyP,
        "KeyQ" => KeyQ,
        "KeyR" => KeyR,
        "KeyS" => KeyS,
        "KeyT" => KeyT,
        "KeyU" => KeyU,
        "KeyV" => KeyV,
        "KeyW" => KeyW,
        "KeyX" => KeyX,
        "KeyY" => KeyY,
        "KeyZ" => KeyZ,
        "Digit0" => Key0,
        "Digit1" => Key1,
        "Digit2" => Key2,
        "Digit3" => Key3,
        "Digit4" => Key4,
        "Digit5" => Key5,
        "Digit6" => Key6,
        "Digit7" => Key7,
        "Digit8" => Key8,
        "Digit9" => Key9,
        "Minus" => KeyMinus,
        "Equal" => KeyEqual,
        "BracketLeft" => KeyLeftBracket,
        "BracketRight" => KeyRightBracket,
        "Semicolon" => KeySemicolon,
        "Quote" => KeyApostrophe,
        "Backquote" => KeyGraveAccent,
        "Comma" => KeyComma,
        "Period" => KeyPeriod,
        "Slash" => KeySlash,
        "Backslash" => KeyBackslash,
        "Enter" | "NumpadEnter" => KeyEnter,
        "Tab" => KeyTab,
        "Space" => KeySpace,
        "Backspace" => KeyBackspace,
        "Escape" => KeyEscape,
        "CapsLock" => KeyCapsLock,
        "ControlLeft" => KeyLeftCtrl,
        "ControlRight" => KeyRightCtrl,
        "ShiftLeft" => KeyLeftShift,
        "ShiftRight" => KeyRightShift,
        "AltLeft" => KeyLeftAlt,
        "AltRight" => KeyRightAlt,
        "MetaLeft" => KeyLeftSuper,
        "MetaRight" => KeyRightSuper,
        "ContextMenu" => KeyMenu,
        "Insert" => KeyInsert,
        "Home" => KeyHome,
        "PageUp" => KeyPageUp,
        "Delete" => KeyDelete,
        "End" => KeyEnd,
        "PageDown" => KeyPageDown,
        "ArrowLeft" => KeyLeftArrow,
        "ArrowRight" => KeyRightArrow,
        "ArrowDown" => KeyDownArrow,
        "ArrowUp" => KeyUpArrow,
        "F1" => KeyF1,
        "F2" => KeyF2,
        "F3" => KeyF3,
        "F4" => KeyF4,
        "F5" => KeyF5,
        "F6" => KeyF6,
        "F7" => KeyF7,
        "F8" => KeyF8,
        "F9" => KeyF9,
        "F10" => KeyF10,
        "F11" => KeyF11,
        "F12" => KeyF12,
        _ => return None,
    })
}

impl Window {
    /// Mounts into the DOM element with id `container_id`, which must already
    /// exist (see `www/index.html`). Sets it up as the positioning root for
    /// the DOM reconciler (`imui/paint_dom.rs`) and wires browser input into
    /// the shared `OSEvent` queue that `get_events` drains.
    ///
    /// `waker.schedule_tick()` is called from every listener below after it
    /// pushes an event — `run_dom` (`imui/lifecycle.rs`) only keeps a
    /// `requestAnimationFrame` tick scheduled while there's a known reason to
    /// (see its doc comment), so without this, input arriving while idle
    /// would sit in the queue unnoticed until something else happened to
    /// wake a tick. `schedule_tick` (not `wake`) deliberately leaves whether
    /// that tick actually rebuilds to `run_dom`'s own `has_actionable_event`
    /// check — e.g. a pure mousemove with no button held shouldn't force a
    /// rebuild (hover feedback is CSS-driven), only `wake`ing would.
    pub fn new_in_container(container_id: &str, waker: RepaintWaker) -> Self {
        let window = web_sys::window().expect("no global `window`");
        let document = window.document().expect("no `document`");
        let container: HtmlElement = document
            .get_element_by_id(container_id)
            .unwrap_or_else(|| panic!("container element #{container_id} not found"))
            .dyn_into()
            .expect("container must be an HtmlElement");

        let style = container.style();
        let _ = style.set_property("position", "relative");
        let _ = style.set_property("overflow", "hidden");
        // `pinch-zoom`, not `none`. `none` hands mae every touch gesture,
        // which sounds right for an app that draws its own everything — but
        // mae has no pinch gesture to hand them to, so all it achieved was
        // taking zoom away: a phone user could neither zoom the browser's way
        // nor the app's. Naming `pinch-zoom` gives the browser exactly the
        // two-finger gesture (and double-tap) while single-finger events
        // still arrive here as pointer events for the app to interpret.
        let _ = style.set_property("touch-action", "pinch-zoom");

        let rect = container.get_bounding_client_rect();
        let size = (rect.width().max(1.0) as f32, rect.height().max(1.0) as f32);

        let events: Rc<RefCell<VecDeque<OSEvent>>> = Rc::new(RefCell::new(VecDeque::new()));
        let touch_pan: Rc<RefCell<TouchPan>> = Rc::new(RefCell::new(TouchPan::default()));
        let events_elem: Element = container.clone().into();
        // Pointer/wheel events are dispatched by position and so belong on the
        // container; keyboard events are dispatched by focus and do not — see
        // the keydown listener below.
        let key_target: EventTarget = document.clone().into();

        let on_pointer_down = {
            let events = events.clone();
            let container = events_elem.clone();
            let waker = waker.clone();
            let touch_pan = touch_pan.clone();
            Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| {
                // mae's input model is one pointer (see `IMUI::mouse`), so a
                // second finger must not be reported as another press. The
                // browser marks exactly one contact primary; the rest belong
                // to gestures it is handling itself, and forwarding them
                // turned every pinch into a jittery drag.
                if !e.is_primary() {
                    return;
                }
                let Some(key) = pointer_button(&e) else {
                    return;
                };
                let pos = container_pos(&container, e.client_x() as f64, e.client_y() as f64);
                // The press is still reported: this may yet turn out to be a
                // tap. `pointermove` decides (see `TouchPan`).
                if e.pointer_type() == "touch" {
                    *touch_pan.borrow_mut() = TouchPan {
                        origin: Some(pos),
                        last: pos,
                        scrolling: false,
                    };
                }
                events
                    .borrow_mut()
                    .push_back(OSEvent::press(key, Some(pos)));
                waker.schedule_tick();
            })
        };
        container
            .add_event_listener_with_callback(
                "pointerdown",
                on_pointer_down.as_ref().unchecked_ref(),
            )
            .expect("add pointerdown listener");

        let on_pointer_up = {
            let events = events.clone();
            let container = events_elem.clone();
            let waker = waker.clone();
            let touch_pan = touch_pan.clone();
            Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| {
                if !e.is_primary() {
                    return;
                }
                let Some(key) = pointer_button(&e) else {
                    return;
                };
                let pos = container_pos(&container, e.client_x() as f64, e.client_y() as f64);
                // A gesture that became a scroll already had its press
                // cancelled; reporting a release here too could land as a
                // click on whatever the finger started on.
                if touch_pan.borrow_mut().take_ending().scrolling {
                    waker.schedule_tick();
                    return;
                }
                events
                    .borrow_mut()
                    .push_back(OSEvent::release(key, Some(pos)));
                waker.schedule_tick();
            })
        };
        container
            .add_event_listener_with_callback("pointerup", on_pointer_up.as_ref().unchecked_ref())
            .expect("add pointerup listener");

        // `pointercancel` fires when the *browser* takes a gesture over —
        // most often a touch that turns out to be a pinch or a browser-level
        // pan. No `pointerup` follows it, so without this the press that
        // started the gesture is never released: mae keeps `active_key` set
        // and treats every later move as a continuing drag.
        let on_pointer_cancel = {
            let events = events.clone();
            let container = events_elem.clone();
            let waker = waker.clone();
            let touch_pan = touch_pan.clone();
            Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| {
                if !e.is_primary() {
                    return;
                }
                touch_pan.borrow_mut().take_ending();
                let pos = container_pos(&container, e.client_x() as f64, e.client_y() as f64);
                // Released as the left button whatever the event says: a
                // cancelled pointer reports `button: -1`, which
                // `pointer_button` maps to `None`, and the press being
                // cancelled is the one mae is holding.
                events
                    .borrow_mut()
                    .push_back(OSEvent::release(OSKey::LeftMouseButton, Some(pos)));
                waker.schedule_tick();
            })
        };
        container
            .add_event_listener_with_callback(
                "pointercancel",
                on_pointer_cancel.as_ref().unchecked_ref(),
            )
            .expect("add pointercancel listener");

        let on_pointer_move = {
            let events = events.clone();
            let container = events_elem.clone();
            let waker = waker.clone();
            let touch_pan = touch_pan.clone();
            Closure::<dyn FnMut(PointerEvent)>::new(move |e: PointerEvent| {
                if !e.is_primary() {
                    return;
                }
                let pos = container_pos(&container, e.client_x() as f64, e.client_y() as f64);
                if touch_pan_move(&touch_pan, &events, pos) {
                    waker.schedule_tick();
                    return;
                }
                events.borrow_mut().push_back(OSEvent::mouse_move(pos));
                // Browsers already throttle pointermove dispatch to roughly
                // the display refresh rate, so this is bounded to "one tick
                // per frame while the pointer is actually moving" — not a
                // return to the old always-scheduled 60Hz idle poll. Needed
                // unconditionally (not just while a button is down) so
                // `self.mouse` stays live for hover-adjacent geometry checks
                // (e.g. scrollbar hit-testing) even outside a drag.
                waker.schedule_tick();
            })
        };
        container
            .add_event_listener_with_callback(
                "pointermove",
                on_pointer_move.as_ref().unchecked_ref(),
            )
            .expect("add pointermove listener");

        let on_wheel = {
            let events = events.clone();
            let container = events_elem.clone();
            let waker = waker.clone();
            Closure::<dyn FnMut(WheelEvent)>::new(move |e: WheelEvent| {
                // Ctrl+wheel is the desktop browser's zoom gesture, not a
                // scroll. Preventing it unconditionally meant the app could
                // not be zoomed with a mouse either — the same hole
                // `touch-action` left on phones.
                if e.ctrl_key() {
                    return;
                }
                e.prevent_default();
                let pos = container_pos(&container, e.client_x() as f64, e.client_y() as f64);
                // Browser deltaY is positive scrolling down; native OSEvent::scroll
                // treats a positive delta as scrolling up (see os/linux.rs).
                let delta = (-(e.delta_y() as f32) / 40.0).clamp(-10.0, 10.0);
                let flags = web_modifiers(e.shift_key(), e.ctrl_key(), e.alt_key(), e.meta_key());
                events
                    .borrow_mut()
                    .push_back(OSEvent::scroll_with_flags(pos, delta, flags));
                waker.schedule_tick();
            })
        };
        container
            .add_event_listener_with_callback("wheel", on_wheel.as_ref().unchecked_ref())
            .expect("add wheel listener");

        let on_key_down = {
            let events = events.clone();
            let waker = waker.clone();
            Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
                // Hosted <input>/<textarea>/contenteditable elements
                // (imui/paint_dom.rs) own their own keyboard/IME handling
                // directly; don't also replay their keys as synthetic
                // OSEvents. See the predicate for the two keys that are
                // deliberately let through anyway.
                if key_owned_by_hosted_editor(&e) {
                    return;
                }
                let Some(code) = web_code_to_oskeycode(&e.code()) else {
                    return;
                };
                let flags = web_modifiers(e.shift_key(), e.ctrl_key(), e.alt_key(), e.meta_key());
                let key = e.key();
                // A Cmd/Ctrl chord is a *shortcut*, not typing: `e.key()` for
                // Cmd+F is still "F", and passing that on as a character
                // would type an F into whatever field has focus alongside
                // running the shortcut. Native platforms never deliver a
                // character for one of these (macOS's text input client
                // simply issues no `insertText`), so this matches them.
                let chars = (!primary_modifier_held(&e) && key.chars().count() == 1)
                    .then(|| key.chars().next())
                    .flatten();
                let mut ev = OSEvent::press_with_flags(OSKey::Keyboard(code), None, flags);
                ev.chars = chars;
                events.borrow_mut().push_back(ev);
                waker.schedule_tick();
            })
        };
        // On `document`, not the container. Keyboard events go to whatever is
        // *focused*, and nothing in `#mae-root` is focused on a freshly
        // loaded page — the container is a plain `<div>` with no `tabindex`,
        // so focus sits on `<body>` and a keydown dispatched there never
        // bubbles *into* the container at all. Listening on the container
        // therefore meant every global shortcut (Ctrl+F, Escape, …) silently
        // did nothing until the user's first click, and did nothing again
        // any time a click landed somewhere unfocusable. Hosted
        // `<input>`/`<textarea>`/`contenteditable` elements are still
        // excluded by target, in `key_owned_by_hosted_editor`, which is
        // independent of where the listener sits.
        key_target
            .add_event_listener_with_callback("keydown", on_key_down.as_ref().unchecked_ref())
            .expect("add keydown listener");

        let on_key_up = {
            let events = events.clone();
            let waker = waker.clone();
            Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
                if key_owned_by_hosted_editor(&e) {
                    return;
                }
                let Some(code) = web_code_to_oskeycode(&e.code()) else {
                    return;
                };
                events
                    .borrow_mut()
                    .push_back(OSEvent::release(OSKey::Keyboard(code), None));
                waker.schedule_tick();
            })
        };
        key_target
            .add_event_listener_with_callback("keyup", on_key_up.as_ref().unchecked_ref())
            .expect("add keyup listener");

        let size_cell: Rc<RefCell<(f32, f32)>> = Rc::new(RefCell::new(size));
        let on_resize = {
            let events = events.clone();
            let container = events_elem.clone();
            let size_cell = size_cell.clone();
            let waker = waker.clone();
            Closure::<dyn FnMut(js_sys::Array)>::new(move |_entries: js_sys::Array| {
                let rect = container.get_bounding_client_rect();
                let w = rect.width().max(1.0) as f32;
                let h = rect.height().max(1.0) as f32;
                let mut cur = size_cell.borrow_mut();
                if (*cur).0 != w || (*cur).1 != h {
                    *cur = (w, h);
                    events.borrow_mut().push_back(OSEvent {
                        ty: OSEventType::Resize,
                        key: OSKey::LeftMouseButton,
                        pos: None,
                        chars: None,
                        deltax: w,
                        deltay: h,
                        flags: None,
                    });
                    waker.schedule_tick();
                }
            })
        };
        let resize_observer = web_sys::ResizeObserver::new(on_resize.as_ref().unchecked_ref())
            .expect("create ResizeObserver");
        resize_observer.observe(&events_elem);

        // The `ResizeObserver` above watches the container's *layout* box,
        // which the on-screen keyboard does not touch on iOS — see
        // `visual_viewport_height`. This is the wake-up for that case; the
        // size itself is read back out by `get_size`, which `run_dom` polls
        // every tick.
        let on_visual_viewport = window.visual_viewport().map(|viewport| {
            let waker = waker.clone();
            let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                waker.schedule_tick();
            });
            let _ = viewport
                .add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref());
            closure
        });

        Window {
            container,
            size,
            dpi: 1.0,
            events,
            _on_pointer_down: on_pointer_down,
            _on_pointer_up: on_pointer_up,
            _on_pointer_cancel: on_pointer_cancel,
            _on_pointer_move: on_pointer_move,
            _on_wheel: on_wheel,
            _on_key_down: on_key_down,
            _on_key_up: on_key_up,
            _on_resize: on_resize,
            _resize_observer: resize_observer,
            _on_visual_viewport: on_visual_viewport,
        }
    }

    pub fn container_element(&self) -> &HtmlElement {
        &self.container
    }

    /// Composition (IME) is owned directly by hosted `<input>`/`<textarea>`
    /// elements in the DOM paint path (`imui/paint_dom.rs`), not replayed
    /// through synthetic key events, so there is no canvas-style preedit
    /// overlay to report here.
    pub fn ime_preedit(&self) -> Option<String> {
        None
    }

    pub fn set_ime_caret_rect(&self, _x: f32, _y: f32, _width: f32, _height: f32) {}

    pub fn get_size(&self) -> (f32, f32) {
        let rect = self.container.get_bounding_client_rect();
        // The smaller of the container's box and the visual viewport, so the
        // on-screen keyboard takes height away from the app rather than
        // covering it — see `visual_viewport_height`.
        let height = match visual_viewport_height() {
            Some(visual) if visual < rect.height() => visual,
            _ => rect.height(),
        };
        (rect.width().max(1.0) as f32, height.max(1.0) as f32)
    }

    pub fn get_render_size(&self) -> (f32, f32) {
        self.get_size()
    }

    pub fn refresh_rate_hz(&self) -> f32 {
        60.0
    }

    pub fn set_title(&self, title: &str) {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                document.set_title(title);
            }
        }
    }

    /// Favicon/app-icon control is out of scope for this slice.
    pub fn set_app_icon(&self, _png_bytes: &[u8]) {}

    /// A no-op on this backend: unlike native windows, there's no single
    /// container-wide cursor to drive from Rust's `resolve_cursor()` here.
    /// `paint_dom.rs` renders `MOUSE_CLICKABLE` boxes as real `<button>`
    /// elements and hosts `LINE_EDIT`/`MULTILINE` as real `<input>`/
    /// `<textarea>` — the browser already shows the right cursor
    /// (pointer / text) for those natively, driven by a static stylesheet
    /// rule (`button.mae-btn` in `paint_dom.rs::DomReconciler::new`), not a
    /// per-frame `cursor` CSS field written here.
    ///
    /// Known gap: `OSCursor::ResizeH/V/NWSE` (drag handles, e.g. an image
    /// resize grip) have no DOM equivalent yet, since that interaction
    /// isn't implemented in the DOM backend at all currently.
    pub fn set_cursor(&mut self, _cursor: OSCursor) {}

    pub fn get_events(&mut self) -> Vec<OSEvent> {
        self.size = self.get_size();
        self.events.borrow_mut().drain(..).collect()
    }
}

/// True if a keyboard event should be left entirely to the hosted text
/// element it landed on — a real `<input>`/`<textarea>` created for a
/// `LINE_EDIT`/plain `MULTILINE` box, or a `RICH_TEXT_HOST`'s `<div
/// contenteditable>` (`paint_dom.rs`) — rather than also being replayed
/// into mae as a synthetic `OSEvent`. `isContentEditable` (not a tag name
/// check) catches the third case: it reflects the element *or an ancestor*
/// having `contenteditable="true"`, so this is also correct if the event's
/// target ends up being a descendant node (a span/text node inside the
/// host) rather than the host element itself.
///
/// Missing the `RICH_TEXT_HOST` case here was a real, user-reported bug
/// (confirmed with this check forced off): every real keystroke landed
/// twice — once through `beforeinput` (`attach_richtext_listeners`, the
/// actual owner), and once more through this bridge replaying it as a
/// synthetic `OSEvent` that native's own key handling then *also* applied
/// to the box (`self.focus_key` now correctly tracks it — see `pending_
/// focus` — so the native path was no longer a silent no-op the way it
/// used to be before that got fixed).
///
/// The exceptions are keys a hosted element does *nothing* with, and which
/// therefore cannot be double-applied — but which mae itself needs, so
/// swallowing them here loses them outright:
///
/// * **Escape**, in all three hosts. It is the app-wide dismiss key
///   (`UISignal`-independent, routed by the app itself), and it edits
///   nothing anywhere.
/// * **Enter, in a single-line `<input>` only.** This is what raises
///   `UISignal::committed()` — "the user accepted this value" — for an
///   inline rename or a name-then-move-on flow. A `<textarea>` and a
///   rich-text host both *do* act on Enter (a newline, an `insertParagraph`
///   `beforeinput`), so it stays swallowed there or the break would land
///   twice.
/// * **Cmd/Ctrl chords**, minus the five the browser itself implements for a
///   text field — see the check below for both halves of that.
fn key_owned_by_hosted_editor(e: &KeyboardEvent) -> bool {
    let Some(el) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
        return false;
    };
    let tag = el.tag_name();
    let single_line_input = tag.eq_ignore_ascii_case("input");
    let hosted = single_line_input
        || tag.eq_ignore_ascii_case("textarea")
        || el.unchecked_ref::<HtmlElement>().is_content_editable();
    if !hosted {
        return false;
    }
    // A Cmd/Ctrl chord is an application shortcut, and a hosted field does
    // nothing with it — except for the handful the *browser* implements for
    // a text field itself, which have to stay the browser's alone or they
    // would be applied twice (once natively, once by mae's own handling of
    // the replayed event — see `text_edit.rs`'s `primary` block, which
    // implements exactly this set). Without this, opening the search palette
    // and then pressing Cmd+Shift+F to widen the search did nothing at all:
    // the palette's own input had focus by then, so its chord was swallowed
    // here before the app ever saw it.
    if primary_modifier_held(e) {
        return matches!(
            e.key().to_ascii_lowercase().as_str(),
            "a" | "c" | "v" | "x" | "z"
        );
    }
    match e.key().as_str() {
        "Escape" => false,
        "Enter" => !single_line_input,
        _ => true,
    }
}

/// Is the platform's primary shortcut modifier held — ⌘ on Apple platforms,
/// Ctrl everywhere else? The browser-side counterpart to
/// `OSEventFlag::command()`, which this target resolves at runtime for the
/// same reason (see `is_apple_platform`).
fn primary_modifier_held(e: &KeyboardEvent) -> bool {
    if is_apple_platform() {
        e.meta_key()
    } else {
        e.ctrl_key()
    }
}

/// The visual viewport's height, when it should be treated as the app's
/// height — `None` when it should not.
///
/// iOS Safari does not shrink the *layout* viewport for the on-screen
/// keyboard (Chrome does, given `interactive-widget=resizes-content` in the
/// page's viewport meta — see `www/index.html`), so the container's box stays
/// full height and the bottom of the app, including whatever field was just
/// focused to summon the keyboard, ends up behind it. The visual viewport
/// *is* shrunk, so reporting that height is what lifts the app back above it.
///
/// Skipped while pinch-zoomed (`scale > 1`), which shrinks the visual
/// viewport just the same: re-laying-out the app under the user's fingers is
/// exactly what handing zoom to the browser is meant to avoid.
fn visual_viewport_height() -> Option<f64> {
    let viewport = web_sys::window()?.visual_viewport()?;
    (viewport.scale() <= 1.01).then(|| viewport.height())
}

/// Is the page open on an Apple platform (macOS, iOS, iPadOS)?
///
/// The one thing the web build cannot learn from `target_os`, which is
/// `"unknown"` on `wasm32-unknown-unknown` — see `OSEventFlag::command`.
/// Asked once and cached: the answer cannot change while the page is open,
/// and it is read on the shortcut path of every key event.
///
/// `navigator.platform` is formally deprecated, but it is the only reading
/// that is present in every browser this ships to; the modern
/// `navigator.userAgentData.platform` is Chromium-only, and matching on the
/// user-agent string is worse than either. `"MacIntel"`, `"iPhone"` and
/// `"iPad"` are the values that matter (an Apple Silicon Mac still reports
/// `"MacIntel"`, and iPadOS reports `"MacIntel"` too — both want ⌘).
pub fn is_apple_platform() -> bool {
    thread_local! {
        static IS_APPLE: std::cell::OnceCell<bool> = const { std::cell::OnceCell::new() };
    }
    IS_APPLE.with(|cell| {
        *cell.get_or_init(|| {
            let platform = web_sys::window()
                .map(|w| w.navigator().platform().unwrap_or_default())
                .unwrap_or_default();
            platform.starts_with("Mac")
                || platform.starts_with("iPhone")
                || platform.starts_with("iPad")
        })
    })
}

pub fn timer_init() -> f64 {
    // microseconds per second; timer_value() below returns microseconds.
    1_000_000.0
}

pub fn timer_value() -> u64 {
    let ms = web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0);
    (ms * 1000.0) as u64
}

/// The browser Clipboard API is async (returns a `Promise`), which doesn't fit
/// this synchronous contract; not wired up in this slice.
pub fn clipboard_set(_text: &str) {}

pub fn clipboard_get() -> Option<String> {
    None
}

pub fn clipboard_get_image() -> Option<Vec<u8>> {
    None
}

pub fn open_image_file_dialog() -> Option<std::path::PathBuf> {
    None
}
