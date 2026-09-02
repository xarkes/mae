use std::{collections::HashMap, fmt::Write};

use crate::{
    imui::{
        Axis, Color, CrossAxisAlign, IMUI, MainAxisAlign, Padding, Point, Size, TextEditState,
        UIBoxFlags, UIBoxStyle, UISignal, UiKey,
    },
    os::{OSEvent, OSEventFlag, OSKey, OSKeyCode},
    render::{
        self, RectCoords, RenderBatch,
        software::{self, SoftwareSurface, Texture},
    },
};

#[derive(Clone, Debug)]
pub struct UiNodeSnapshot {
    pub key: UiKey,
    pub parent_key: Option<UiKey>,
    pub depth: usize,
    pub child_count: usize,
    pub label: Option<String>,
    pub text: Option<String>,
    pub bounds: RectCoords,
    pub computed_size: Size,
    pub scroll: Point,
    pub scroll_max: Point,
    pub content_size: Size,
    pub clip_rect: RectCoords,
    pub signal: UISignal,
    /// The widget's stable `###` id, when it has one. Survives display-text
    /// changes, so tests addressing a widget by id don't churn when a label is
    /// reworded or translated.
    pub key_id: Option<String>,
    pub flags: UIBoxFlags,
    pub visible: bool,
    pub focused: bool,
    pub layout_axis: Axis,
    pub padding: Padding,
    pub child_gap: f32,
    pub main_axis_align: MainAxisAlign,
    pub cross_axis_align: CrossAxisAlign,
    pub style: UIBoxStyle,
    pub hot_t: f32,
    pub active_t: f32,
    pub focus_t: f32,
    pub appear_t: f32,
    /// Effective painted alpha: this box's own `style.opacity` and `appear_t`
    /// multiplied by every ancestor's — i.e. what actually reaches the screen,
    /// not just what this box asked for.
    pub opacity: f32,
    pub text_edit: Option<TextEditState>,
}

impl UiNodeSnapshot {
    /// Match by stable `###` id or by visible text. Prefer the id in tests:
    /// `matches("###app_new_note_btn")` keeps working when the button's label
    /// changes, which display-text matching does not.
    pub fn matches(&self, id: &str) -> bool {
        self.key_id.as_deref() == Some(id)
            || self.label.as_deref() == Some(id)
            || self.text.as_deref() == Some(id)
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.bounds.x0 + self.bounds.width() / 2.0,
            self.bounds.y0 + self.bounds.height() / 2.0,
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct UiSnapshot {
    pub nodes: Vec<UiNodeSnapshot>,
}

impl UiSnapshot {
    pub fn node(&self, id: &str) -> &UiNodeSnapshot {
        self.try_node(id)
            .unwrap_or_else(|| panic!("UI node not found: {id}"))
    }

    pub fn try_node(&self, id: &str) -> Option<&UiNodeSnapshot> {
        self.nodes.iter().find(|node| node.matches(id))
    }

    pub fn debug_dump(&self) -> String {
        let mut out = String::new();
        self.write_debug_dump(&mut out)
            .expect("writing UiSnapshot debug dump to String should not fail");
        out
    }

    pub fn write_debug_dump(&self, out: &mut impl Write) -> std::fmt::Result {
        writeln!(out, "UiSnapshot nodes={}", self.nodes.len())?;
        for (idx, node) in self.nodes.iter().enumerate() {
            write_node_dump(out, idx, node)?;
        }
        Ok(())
    }
}

pub struct UiHarness {
    ui: IMUI,
    last_snapshot: UiSnapshot,
}

impl UiHarness {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            ui: IMUI::new_for_test(width, height),
            last_snapshot: UiSnapshot::default(),
        }
    }

    pub fn ui(&self) -> &IMUI {
        &self.ui
    }

    pub fn ui_mut(&mut self) -> &mut IMUI {
        &mut self.ui
    }

    pub fn frame(&mut self, build: impl FnOnce(&mut IMUI)) -> UiSnapshot {
        self.ui.begin_frame();
        build(&mut self.ui);
        self.last_snapshot = self.ui.end_test_frame();
        self.last_snapshot.clone()
    }

    pub fn push_event(&mut self, event: OSEvent) {
        self.ui.push_test_event(event);
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.push_event(OSEvent::mouse_move(Point::new(x, y)));
    }

    pub fn mouse_down(&mut self, button: OSKey, x: f32, y: f32) {
        self.push_event(OSEvent::press(button, Some(Point::new(x, y))));
    }

    pub fn mouse_up(&mut self, button: OSKey, x: f32, y: f32) {
        self.push_event(OSEvent::release(button, Some(Point::new(x, y))));
    }

    /// See [`UiDriver::drag_x`]. Queues the whole gesture as events without
    /// rendering anything, like every other `UiHarness` input method — so
    /// the caller's own frame loop decides when they're processed. That
    /// makes it a *click*, not a drag, unless frames are interleaved
    /// between the steps: see `NativeDriver`'s own `drag_x` for why, and
    /// prefer that one for drag-selection.
    pub fn drag_x(&mut self, id: &str, from_frac: f32, to_frac: f32) {
        let bounds = self.last_snapshot.node(id).bounds;
        let (width, y) = (bounds.width(), bounds.y0 + bounds.height() / 2.0);
        let (from_x, to_x) = (bounds.x0 + width * from_frac, bounds.x0 + width * to_frac);
        self.mouse_move(from_x, y);
        self.mouse_down(OSKey::LeftMouseButton, from_x, y);
        self.mouse_move(from_x + (to_x - from_x) / 2.0, y);
        self.mouse_move(to_x, y);
        self.mouse_up(OSKey::LeftMouseButton, to_x, y);
    }

    pub fn click_at(&mut self, x: f32, y: f32) {
        self.mouse_move(x, y);
        self.mouse_down(OSKey::LeftMouseButton, x, y);
        self.mouse_up(OSKey::LeftMouseButton, x, y);
    }

    pub fn click(&mut self, id: &str) {
        let center = self.last_snapshot.node(id).center();
        self.click_at(center.x(), center.y());
    }

    pub fn right_click_at(&mut self, x: f32, y: f32) {
        self.mouse_move(x, y);
        self.mouse_down(OSKey::RightMouseButton, x, y);
        self.mouse_up(OSKey::RightMouseButton, x, y);
    }

    pub fn right_click(&mut self, id: &str) {
        let center = self.last_snapshot.node(id).center();
        self.right_click_at(center.x(), center.y());
    }

    pub fn scroll_at(&mut self, x: f32, y: f32, delta: f32) {
        self.push_event(OSEvent::scroll(Point::new(x, y), delta));
    }

    pub fn scroll_at_with_flags(&mut self, x: f32, y: f32, delta: f32, flags: OSEventFlag) {
        self.push_event(OSEvent::scroll_with_flags(
            Point::new(x, y),
            delta,
            Some(flags),
        ));
    }

    pub fn scroll(&mut self, id: &str, delta: f32) {
        let center = self.last_snapshot.node(id).center();
        self.scroll_at(center.x(), center.y(), delta);
    }

    pub fn scroll_with_flags(&mut self, id: &str, delta: f32, flags: OSEventFlag) {
        let center = self.last_snapshot.node(id).center();
        self.scroll_at_with_flags(center.x(), center.y(), delta, flags);
    }

    pub fn key_press(&mut self, key: OSKeyCode) {
        self.push_event(OSEvent::press(OSKey::Keyboard(key), None));
    }

    pub fn key_press_with_flags(&mut self, key: OSKeyCode, flags: OSEventFlag) {
        self.push_event(OSEvent::press_with_flags(
            OSKey::Keyboard(key),
            None,
            Some(flags),
        ));
    }

    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.push_event(OSEvent::text(ch));
        }
    }

    pub fn snapshot(&self) -> &UiSnapshot {
        &self.last_snapshot
    }

    pub fn debug_dump(&self) -> String {
        self.last_snapshot.debug_dump()
    }
}

/// Shared action+query surface for driving a mae UI from a test scenario —
/// implemented once by [`NativeDriver`] (in-process, synchronous, no
/// browser) and once by `cdp::CdpDriver` (feature = "cdp": drives a real
/// page over the Chrome DevTools Protocol). A scenario function written
/// against `impl UiDriver` runs unchanged against either backend, so
/// coverage doesn't need to be hand-duplicated per platform.
///
/// Selection is always by `id`: the same string [`UiNodeSnapshot::matches`]
/// compares against (a box's current display text/value) — `paint_dom.rs`
/// mirrors that same string onto the DOM as the `data-mae-id` attribute, so
/// both backends select "the same way".
///
/// Each method is expected to leave the UI settled (any resulting rebuild —
/// native or the real browser's own event loop — has already happened)
/// before returning, so scenario code never needs a separate "advance a
/// frame" step.
pub trait UiDriver {
    fn click(&mut self, id: &str);
    fn right_click(&mut self, id: &str);
    /// Move the pointer onto `id` and leave it there.
    ///
    /// Hover is a first-class interaction — highlights, tooltips, and the
    /// hot/active bookkeeping behind them all hang off it — but until this
    /// existed no scenario could express it, so none of that was covered on
    /// either backend.
    fn hover(&mut self, id: &str);
    /// Press, move, and release across `id` horizontally at its vertical
    /// centre, from `from_frac` to `to_frac` of its own width (`0.0` = its
    /// left edge, `1.0` = its right edge) — i.e. a real mouse drag, the
    /// gesture that selects text. Fractions outside `0.0..=1.0` are allowed
    /// and land outside the element, which is how you select right up to a
    /// line's end without depending on exact glyph widths.
    ///
    /// Deliberately a *drag*, not a "select this range" shortcut: text
    /// selection on the DOM backend is the browser's own, built from real
    /// pointer hit-testing across the painted spans, so anything less than
    /// genuine mouse events wouldn't exercise it (see `paint_dom.rs`'s
    /// `sync_richtext_caret` and the `[contenteditable] *` style rule).
    fn drag_x(&mut self, id: &str, from_frac: f32, to_frac: f32);
    fn scroll(&mut self, id: &str, delta: f32);
    fn key_press(&mut self, key: OSKeyCode);
    fn key_press_with_flags(&mut self, key: OSKeyCode, flags: OSEventFlag);
    /// Types at the current input focus, same as [`UiHarness::type_text`].
    fn type_text(&mut self, text: &str);
    /// The element currently matching `id`'s text/value, if any.
    fn text_of(&mut self, id: &str) -> Option<String>;
    /// Let the UI advance without dispatching any input first — one frame
    /// natively, one full repaint-idle wait in a browser. For the handful of
    /// effects that land a frame or two *after* the action that requested
    /// them (a deferred focus move, a debounced save), where a scenario has
    /// to wait rather than assert immediately.
    fn settle(&mut self);
    /// The stable `###` id of whatever currently holds the keyboard focus —
    /// where typing goes — or `None` when nothing does.
    ///
    /// The two backends answer this from genuinely different places (mae's
    /// own `focus_key`; the browser's `document.activeElement`), which is
    /// exactly the point: a scenario asserting on it holds the DOM backend
    /// to actually *moving the browser's focus* when the app focuses a
    /// field, rather than only drawing a focus ring around it.
    fn focused_id(&mut self) -> Option<String>;
    /// Whether an element currently matches `id`.
    fn exists(&mut self, id: &str) -> bool {
        self.text_of(id).is_some()
    }
}

/// The `###` id of the focused node in `snapshot`, for both `UiDriver`
/// implementations backed by one — see [`UiDriver::focused_id`].
fn focused_id_in(snapshot: &UiSnapshot) -> Option<String> {
    snapshot
        .nodes
        .iter()
        .find(|node| node.focused)
        .and_then(|node| node.key_id.clone())
}

fn node_text(node: &UiNodeSnapshot) -> String {
    node.text
        .clone()
        .or_else(|| node.label.clone())
        .unwrap_or_default()
}

impl UiDriver for UiHarness {
    fn click(&mut self, id: &str) {
        UiHarness::click(self, id);
    }

    fn hover(&mut self, id: &str) {
        let center = self.snapshot().node(id).center();
        self.mouse_move(center.x(), center.y());
    }

    fn right_click(&mut self, id: &str) {
        UiHarness::right_click(self, id);
    }

    fn drag_x(&mut self, id: &str, from_frac: f32, to_frac: f32) {
        UiHarness::drag_x(self, id, from_frac, to_frac);
    }

    fn scroll(&mut self, id: &str, delta: f32) {
        UiHarness::scroll(self, id, delta);
    }

    fn key_press(&mut self, key: OSKeyCode) {
        UiHarness::key_press(self, key);
    }

    fn key_press_with_flags(&mut self, key: OSKeyCode, flags: OSEventFlag) {
        UiHarness::key_press_with_flags(self, key, flags);
    }

    fn type_text(&mut self, text: &str) {
        UiHarness::type_text(self, text);
    }

    fn text_of(&mut self, id: &str) -> Option<String> {
        self.last_snapshot.try_node(id).map(node_text)
    }

    fn settle(&mut self) {
        // `UiHarness` has no build closure of its own to re-run, so a
        // scenario driving one directly is responsible for its own frames —
        // the snapshot it already holds is as settled as this can get.
    }

    fn focused_id(&mut self) -> Option<String> {
        focused_id_in(&self.last_snapshot)
    }
}

/// A [`UiDriver`] that owns a [`UiHarness`] plus the app's own per-frame
/// `build` closure (e.g. `|ui| render(ui, &mut state)`), so every action
/// automatically re-renders afterward — the same "act, then call `.frame`"
/// step every hand-written testkit scenario already did explicitly, just no longer duplicated per call
/// site or per test file.
pub struct NativeDriver<F> {
    harness: UiHarness,
    build: F,
}

impl<F: FnMut(&mut IMUI)> NativeDriver<F> {
    pub fn new(width: f32, height: f32, mut build: F) -> Self {
        let mut harness = UiHarness::new(width, height);
        harness.frame(&mut build);
        Self { harness, build }
    }

    pub fn harness(&self) -> &UiHarness {
        &self.harness
    }

    pub fn harness_mut(&mut self) -> &mut UiHarness {
        &mut self.harness
    }

    /// Re-renders one frame without performing any input action first —
    /// e.g. to let a debounced/async effect settle.
    pub fn settle(&mut self) -> UiSnapshot {
        self.harness.frame(&mut self.build)
    }

    /// Two settle passes: the first frame's `build` reads a widget's own
    /// state *before* it processes that widget's own signal for this frame
    /// (build order, not a framework guarantee — e.g. `counter_widget`
    /// below and `src/main.rs`'s real counter demo both format their label
    /// from `*counter` ahead of the `if plus.clicked() { *counter += 1 }`
    /// check on the same line further down), so a state mutation triggered
    /// by *this* action isn't visible in that frame's own output yet — only
    /// starting from the next build. A real browser click naturally gets
    /// this for free (pointerdown/pointerup/click are separate DOM events,
    /// each forcing its own rebuild — see `CdpDriver`'s doc comment), so
    /// this mirrors that instead of leaning on every widget happening to
    /// check its own signal before formatting its own display text.
    fn settle_twice(&mut self) {
        self.settle();
        self.settle();
    }
}

impl<F: FnMut(&mut IMUI)> UiDriver for NativeDriver<F> {
    fn click(&mut self, id: &str) {
        self.harness.click(id);
        self.settle_twice();
    }

    fn hover(&mut self, id: &str) {
        let center = self.harness.snapshot().node(id).center();
        self.harness.mouse_move(center.x(), center.y());
        self.settle_twice();
    }

    fn right_click(&mut self, id: &str) {
        self.harness.right_click(id);
        self.settle_twice();
    }

    fn drag_x(&mut self, id: &str, from_frac: f32, to_frac: f32) {
        let bounds = self.harness.snapshot().node(id).bounds;
        let (width, y) = (bounds.width(), bounds.y0 + bounds.height() / 2.0);
        let (from_x, to_x) = (bounds.x0 + width * from_frac, bounds.x0 + width * to_frac);
        // A rendered frame between each step, rather than pushing the whole
        // gesture and settling once at the end: a native drag only extends a
        // selection across *frames* — the press frame sets the anchor, and
        // each later frame extends to wherever the pointer now is (see
        // `imui/tests.rs`'s own mouse-drag tests, which interleave frames the
        // same way). Pushed all at once, this would register as a plain click.
        self.harness.mouse_move(from_x, y);
        self.harness.mouse_down(OSKey::LeftMouseButton, from_x, y);
        self.settle();
        self.harness.mouse_move(from_x + (to_x - from_x) / 2.0, y);
        self.settle();
        self.harness.mouse_move(to_x, y);
        self.settle();
        self.harness.mouse_up(OSKey::LeftMouseButton, to_x, y);
        self.settle_twice();
    }

    fn scroll(&mut self, id: &str, delta: f32) {
        self.harness.scroll(id, delta);
        self.settle_twice();
    }

    fn key_press(&mut self, key: OSKeyCode) {
        self.harness.key_press(key);
        self.settle_twice();
    }

    fn key_press_with_flags(&mut self, key: OSKeyCode, flags: OSEventFlag) {
        self.harness.key_press_with_flags(key, flags);
        self.settle_twice();
    }

    fn type_text(&mut self, text: &str) {
        self.harness.type_text(text);
        self.settle_twice();
    }

    fn text_of(&mut self, id: &str) -> Option<String> {
        self.harness.snapshot().try_node(id).map(node_text)
    }

    fn settle(&mut self) {
        NativeDriver::settle(self);
    }

    fn focused_id(&mut self) -> Option<String> {
        focused_id_in(self.harness.snapshot())
    }
}

// Drives a real browser over the Chrome DevTools Protocol against a locally
// installed Chromium — the other `UiDriver` implementation, see its doc
// comment. Test/dev tooling only: never built into the app itself, and
// never reachable from a wasm32 build (a browser can't launch another
// browser).
#[cfg(all(feature = "cdp", not(target_arch = "wasm32")))]
pub mod cdp;

fn write_node_dump(out: &mut impl Write, idx: usize, node: &UiNodeSnapshot) -> std::fmt::Result {
    let indent = "  ".repeat(node.depth);
    let name = node_name(node);
    writeln!(
        out,
        "{indent}{idx:03} {name} key={:#x} parent={} children={} bounds={} size={} clip={}",
        node.key.0,
        node.parent_key
            .map(|key| format!("{:#x}", key.0))
            .unwrap_or_else(|| "-".to_string()),
        node.child_count,
        fmt_rect(node.bounds),
        fmt_size(node.computed_size),
        fmt_rect(node.clip_rect),
    )?;

    let mut flags = Vec::new();
    push_flag(&mut flags, node.visible, "visible");
    push_flag(&mut flags, node.focused, "focused");
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::MOUSE_CLICKABLE),
        "mouse",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::KEYBOARD_CLICKABLE),
        "keyboard",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::CLICK_TO_FOCUS),
        "click_focus",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::TEXT_INPUT),
        "text_input",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::SCROLL_X),
        "scroll_x",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::SCROLL_Y),
        "scroll_y",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::DRAW_BACKGROUND),
        "bg",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::DRAW_BORDER),
        "border",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::DRAW_TEXT),
        "text",
    );
    push_flag(&mut flags, node.flags.contains(UIBoxFlags::CLIP), "clip");
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::FLOATING_X),
        "float_x",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::FLOATING_Y),
        "float_y",
    );
    push_flag(
        &mut flags,
        node.flags.contains(UIBoxFlags::NO_WRAP_X),
        "no_wrap_x",
    );

    writeln!(
        out,
        "{indent}    flags=[{}] signal={:?} axis={:?} align={:?}/{:?} gap={:.1} padding={} scroll={} max={} content={}",
        flags.join(","),
        node.signal,
        node.layout_axis,
        node.main_axis_align,
        node.cross_axis_align,
        node.child_gap,
        fmt_padding(node.padding),
        fmt_point(node.scroll),
        fmt_point(node.scroll_max),
        fmt_size(node.content_size),
    )?;

    writeln!(
        out,
        "{indent}    style=font:{:.1} icon:{} margin:{:.1} border:{:.1} radius:{:.1} bg:{} text:{} border:{} anim=hot:{:.2} active:{:.2} focus:{:.2} appear:{:.2}",
        node.style.font_size,
        node.style.font_icon,
        node.style.margin,
        node.style.border_size,
        node.style.corner_radius,
        fmt_color(node.style.bg_color),
        fmt_color(node.style.text_color),
        fmt_color(node.style.border_color),
        node.hot_t,
        node.active_t,
        node.focus_t,
        node.appear_t,
    )?;

    if let Some(text_edit) = &node.text_edit {
        writeln!(
            out,
            "{indent}    text_edit=cursor:{} selection:{:?} desired_column:{:?} scroll_follow:{:?}",
            text_edit.cursor,
            text_edit.selection_range(),
            text_edit.desired_column,
            text_edit.scroll_follow_cursor,
        )?;
    }

    Ok(())
}

fn node_name(node: &UiNodeSnapshot) -> String {
    match (node.label.as_deref(), node.text.as_deref()) {
        (Some(label), Some(text)) if label == text => format!("label={}", quote(label)),
        (Some(label), Some(text)) => format!("label={} text={}", quote(label), quote(text)),
        (Some(label), None) => format!("label={}", quote(label)),
        (None, Some(text)) => format!("text={}", quote(text)),
        (None, None) => "<anonymous>".to_string(),
    }
}

fn quote(value: &str) -> String {
    format!("{value:?}")
}

fn push_flag(flags: &mut Vec<&'static str>, enabled: bool, name: &'static str) {
    if enabled {
        flags.push(name);
    }
}

fn fmt_rect(rect: RectCoords) -> String {
    format!(
        "({:.1},{:.1})-({:.1},{:.1})",
        rect.x0, rect.y0, rect.x1, rect.y1
    )
}

fn fmt_size(size: Size) -> String {
    format!("{:.1}x{:.1}", size.width, size.height)
}

fn fmt_point(point: Point) -> String {
    format!("({:.1},{:.1})", point.x(), point.y())
}

fn fmt_padding(padding: Padding) -> String {
    format!(
        "t:{:.1} r:{:.1} b:{:.1} l:{:.1}",
        padding.top, padding.right, padding.bottom, padding.left
    )
}

fn fmt_color(color: Color) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color_byte(color.r),
        color_byte(color.g),
        color_byte(color.b),
        color_byte(color.a),
    )
}

fn color_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderSnapshot {
    surface: SoftwareSurface,
}

impl RenderSnapshot {
    pub fn width(&self) -> usize {
        self.surface.width()
    }

    pub fn height(&self) -> usize {
        self.surface.height()
    }

    pub fn pixels(&self) -> &[u32] {
        self.surface.pixels()
    }

    pub fn pixel(&self, x: usize, y: usize) -> u32 {
        self.surface.pixels()[y * self.surface.width() + x]
    }
}

pub fn render_batches(width: usize, height: usize, batches: &[RenderBatch]) -> RenderSnapshot {
    render_batches_with_textures(width, height, batches, &HashMap::new())
}

pub fn render_batches_with_textures(
    width: usize,
    height: usize,
    batches: &[RenderBatch],
    textures: &HashMap<u32, Texture>,
) -> RenderSnapshot {
    let mut surface = SoftwareSurface::new(width, height);
    surface.clear(software::DEFAULT_CLEAR_COLOR);
    render::software::render_batches(&mut surface, batches, textures);
    RenderSnapshot { surface }
}
