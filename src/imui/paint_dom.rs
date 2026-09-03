//! DOM render backend: reconciles the box tree into real DOM elements instead
//! of GL draw calls (see `render/mod.rs`'s `RenderBackend`, which this
//! deliberately bypasses — glyph rasterization there happens *after* the
//! semantic box-tree walk, one step too late for real `<div>`/`<input>`
//! output). Text-editing boxes (`LINE_EDIT`/`MULTILINE`) get a real
//! `<input>`/`<textarea>` so the browser owns IME composition, selection, and
//! spellcheck directly instead of a synthetic canvas caret.
//!
//! Only reachable with `feature = "dom"` (wasm32 web target); native builds
//! never construct a `DomReconciler` (`IMUI::dom` stays `None`), so this
//! module adds no per-frame cost to the GPU paint path in `paint.rs`.

use super::input::DomPointerState;
use super::*;
use rustc_hash::FxHasher;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hasher;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    Blob, BlobPropertyBag, Document, Element, HtmlElement, HtmlImageElement, HtmlInputElement,
    HtmlTextAreaElement, InputEvent, KeyboardEvent, Node, PointerEvent, Range, StaticRange, Text,
    Url,
};

/// A text-editing box's pending edit, as last reported by its hosted DOM
/// element, applied at the start of the box's next `line_edit`/`textarea`
/// call (see `IMUI::apply_pending_dom_edit`, called from `text_edit.rs`).
pub(super) enum PendingDomEdit {
    /// A plain `<input>`/`<textarea>` (`LINE_EDIT`, or `MULTILINE` without
    /// `RICH_TEXT_HOST`): the hosted element's whole `.value` mirrors the raw
    /// buffer 1:1, so every edit is reported as a full replace — simple, and
    /// cheap enough since these buffers are typically small.
    Replace { value: String, cursor: usize },
    /// A rich-text host (`MULTILINE` + `RICH_TEXT_HOST`): the DOM shows
    /// *rendered* markdown, not raw text, so there's no single `.value` to
    /// diff — instead `attach_richtext_listeners` computes the edit directly
    /// from the intercepted `beforeinput` event (its target range, resolved
    /// to raw offsets via `data-raw-start`/`data-raw-end` — see
    /// `resolve_raw_offset`) before the browser ever mutates the DOM.
    Range {
        raw_start: usize,
        raw_end: usize,
        replacement: String,
        cursor: usize,
    },
    /// Ctrl+Z / Ctrl+Shift+Z in a rich-text host. The browser has no history
    /// of its own to undo here — `attach_richtext_listeners` prevents every
    /// edit it would otherwise make, so the DOM is entirely Rust's doing —
    /// which is why the intent is forwarded to the editor's own undo stack
    /// instead (see `IMUI::apply_pending_dom_edit`).
    History { redo: bool },
}

/// One entry of `DomReconciler::richtext_log` — a rich-text host child's raw
/// range and which kind of DOM anchor it is, for `sync_richtext_caret` to
/// resolve a raw cursor offset back to a `(Node, intra-offset)` browser
/// selection target.
#[derive(Clone, Copy)]
enum RichTextAnchorKind {
    /// A span: its element has exactly one child text node, so the caret can
    /// land at any intra-text offset within it.
    Text,
    /// An image line or an empty-line spacer: nothing to land a caret
    /// *inside*, so the caret goes just before or just after this element
    /// instead (see `sync_richtext_caret`).
    Atomic,
}

/// Native press/release/click state for one `MOUSE_CLICKABLE` box since it
/// was last taken — see `DomReconciler::dom_pointer_edges` and
/// `IMUI::take_dom_pointer_edge`. Bools rather than an enum since a box
/// could in principle see more than one of these in a single un-consumed
/// frame (e.g. press then release before the next rebuild). Right-button
/// state is just `right_clicked` (from the native `contextmenu` event,
/// which already only fires for a real right-click): mae has no
/// right-button-drag concept, so there's no need to mirror `left_pressed`'s
/// press/active-key exclusivity dance for it.
#[derive(Default, Clone, Copy)]
pub(super) struct DomPointerEdge {
    pub left_pressed: bool,
    pub left_released: bool,
    pub left_clicked: bool,
    pub right_clicked: bool,
}

enum DomNode {
    Div(HtmlElement),
    Input(HtmlInputElement),
    TextArea(HtmlTextAreaElement),
    Img(HtmlImageElement),
    /// A `RICH_TEXT_HOST` box's `<div contenteditable="true">` — distinct
    /// from `Div` (a plain, non-editable `<div>`, used for everything else,
    /// e.g. a rich-text host's own row/span children) so `paint_richtext_
    /// host` can tell whether a `DomKey` reused across frames still names
    /// the right *kind* of node, exactly like `paint_text_input`'s own
    /// `Input`/`TextArea` check — this matters here specifically because
    /// toggling `MarkdownMode::Source`/`Rendered` swaps a `MULTILINE` box
    /// between a plain `TextArea` and this, at the same `DomKey`.
    RichText(HtmlElement),
}

impl DomNode {
    fn as_html_element(&self) -> &HtmlElement {
        match self {
            DomNode::Div(e) => e,
            DomNode::Input(e) => e.unchecked_ref(),
            DomNode::TextArea(e) => e.unchecked_ref(),
            DomNode::Img(e) => e.unchecked_ref(),
            DomNode::RichText(e) => e,
        }
    }
}

/// The UTF-16 code-unit offset of char index `chars` in `value` — the unit
/// every DOM text API (`set_selection_range`, a `Range`'s offsets,
/// `Selection`) counts in, while every offset on the Rust side is a char
/// index. Saturates at the end of the string.
fn utf16_offset(value: &str, chars: usize) -> u32 {
    value
        .chars()
        .take(chars)
        .map(|c| c.len_utf16() as u32)
        .sum()
}

/// The inverse of [`utf16_offset`]: the char index `units` UTF-16 code units
/// into `value`.
///
/// Every offset a browser hands back — `selectionStart`, a `Range`'s offsets,
/// a `Selection`'s anchor — counts UTF-16 code units, and every offset on the
/// Rust side is a char index. The two agree only while the text stays inside
/// the BMP, so anything past it (every emoji, which is a surrogate *pair*)
/// shifted the caret by one char per emoji before it. Worse, a UTF-16 offset
/// used to slice the UTF-8 `String` directly lands mid-character for any
/// non-ASCII text at all and panics — which in a wasm build kills the frame,
/// so typing an accent or an emoji stopped text input dead.
///
/// An offset that falls *inside* a surrogate pair (which a browser should
/// never produce) rounds up to the end of that char rather than splitting it.
fn char_offset_from_utf16(value: &str, units: usize) -> usize {
    let mut seen = 0usize;
    for (index, c) in value.chars().enumerate() {
        if seen >= units {
            return index;
        }
        seen += c.len_utf16();
    }
    value.chars().count()
}

/// Minimal RGBA -> PNG encoder (uncompressed "stored" zlib block — no need
/// for real compression, this only ever runs once per image at DOM-node
/// creation time, not per frame). Same technique as `render/png.rs`'s
/// `png_capture`-gated encoder, duplicated here rather than lifting that
/// feature gate, since the two have no other reason to depend on each other.
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    fn write_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
        buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
        buf.extend_from_slice(chunk_type);
        buf.extend_from_slice(data);
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in chunk_type.iter().chain(data) {
            let mut v = (crc ^ byte as u32) & 0xFF;
            for _ in 0..8 {
                v = if v & 1 != 0 {
                    (v >> 1) ^ 0xEDB8_8320
                } else {
                    v >> 1
                };
            }
            crc = (crc >> 8) ^ v;
        }
        buf.extend_from_slice(&(crc ^ 0xFFFF_FFFF).to_be_bytes());
    }
    fn zlib_store(data: &[u8]) -> Vec<u8> {
        let mut buf = vec![0x78, 0x01];
        const MAX_BLOCK: usize = 65535;
        let mut offset = 0;
        loop {
            let end = (offset + MAX_BLOCK).min(data.len());
            let block = &data[offset..end];
            let is_last = end >= data.len();
            let len = block.len() as u16;
            buf.push(if is_last { 0x01 } else { 0x00 });
            buf.extend_from_slice(&len.to_le_bytes());
            buf.extend_from_slice(&(!len).to_le_bytes());
            buf.extend_from_slice(block);
            if is_last {
                break;
            }
            offset = end;
        }
        const M: u32 = 65521;
        let (mut s1, mut s2) = (1u32, 0u32);
        for &b in data {
            s1 = (s1 + b as u32) % M;
            s2 = (s2 + s1) % M;
        }
        buf.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
        buf
    }

    let (width, height) = (width as usize, height as usize);
    let mut buf = Vec::new();
    buf.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&(width as u32).to_be_bytes());
    ihdr[4..8].copy_from_slice(&(height as u32).to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 6; // color type: RGBA
    write_chunk(&mut buf, b"IHDR", &ihdr);
    let mut raw = Vec::with_capacity(height * (1 + width * 4));
    for row in 0..height {
        raw.push(0); // filter: None
        raw.extend_from_slice(&rgba[row * width * 4..(row + 1) * width * 4]);
    }
    write_chunk(&mut buf, b"IDAT", &zlib_store(&raw));
    write_chunk(&mut buf, b"IEND", &[]);
    buf
}

/// CSS `padding` in (top, right, bottom, left) px, folding in the box's
/// `style.margin` the same way `paint.rs`'s `content_left`/`content_y0`
/// inset text by `padding.X + style.margin` — see `draw_ui_root_skipping_clipped`.
type Inset = (f32, f32, f32, f32);

#[derive(Clone)]
struct PaintSnapshot {
    rect: RectCoords,
    /// This box's *own* alpha multiplier. Unlike the native path, which folds
    /// ancestors in via `box_opacity`, CSS `opacity` already composes down the
    /// element tree — so each node carries only its own factor.
    opacity: f32,
    bg: Option<Color>,
    border: Option<(Color, f32)>,
    corner_radius: f32,
    inset: Inset,
    /// (hover_bg, active_bg) for a DRAW_HOT_EFFECTS box — see `paint_div`'s
    /// `hot` parameter.
    hot: Option<(Color, Color)>,
    text: Option<String>,
    /// `Some` for a normal-flow box (flex layout, sized but not positioned by
    /// Rust); `None` for a floating/absolute box (`Self::position`, sized
    /// *and* positioned by Rust) or a node with no layout intent of its own
    /// (hosted input, image, scrollbar thumb).
    flow: Option<FlowLayout>,
    /// What was last written by `apply_flow_size`. Compared instead of the
    /// rect for a normal-flow box: a box that grows or takes a percentage
    /// emits the same CSS whatever the solve made of it that frame, and a
    /// box that changed which of the two it is emits different CSS at an
    /// unchanged size.
    flow_size: Option<FlowSize>,
    /// Tracked separately from `style_differs`'s other fields so a theme
    /// switch that only changes text color (bg/border/etc. of a plain label
    /// often stay identical, e.g. transparent-on-transparent) still triggers
    /// a DOM write — otherwise the label keeps the old theme's stale color
    /// indefinitely, since nothing else about it changed to trip the diff.
    text_color: Color,
    /// Mirrors the box's `display_string` — the same current display text
    /// `testkit`'s `UiNodeSnapshot::matches` selects nodes by (see
    /// `src/testkit.rs`) — as the `data-mae-id` attribute (see `apply_id`),
    /// so a CDP-driven driver can select elements the exact same way
    /// testkit does instead of falling back to fragile `textContent`
    /// search.
    id: Option<String>,
    /// Mirrors `UIBox::key_id` as the `data-mae-key` attribute — the stable
    /// `###` half of the widget label, so a CDP driver can address a widget by
    /// id the same way `UiNodeSnapshot::matches` does natively.
    key_id: Option<String>,
    /// Mirrors `UIBox::richtext_span` — the `data-raw-start`/`data-raw-end`
    /// attributes stamped for a rich-text host's row/span/image children
    /// (see `DomReconciler::set_richtext_span`). `None` for every other node.
    richtext_span: Option<(usize, usize)>,
}

impl PaintSnapshot {
    fn blank() -> Self {
        PaintSnapshot {
            rect: RectCoords {
                x0: f32::NAN,
                y0: f32::NAN,
                x1: f32::NAN,
                y1: f32::NAN,
            },
            // NaN so the first `style_differs` always reports a change and the
            // real value gets written, matching the other blank sentinels.
            opacity: f32::NAN,
            bg: None,
            border: None,
            corner_radius: -1.0,
            inset: (f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            hot: None,
            text: None,
            flow: None,
            flow_size: None,
            text_color: Color {
                r: -1.0,
                g: -1.0,
                b: -1.0,
                a: -1.0,
            },
            id: None,
            key_id: None,
            richtext_span: None,
        }
    }

    /// For a floating/absolute node, all four coordinates matter (Rust sets
    /// both position and size). For a normal-flow node, position is CSS's to
    /// decide — only a size change needs a DOM write.
    fn geometry_differs(&self, other: &RectCoords, floating: bool, size: FlowSize) -> bool {
        if floating {
            self.rect.x0 != other.x0
                || self.rect.y0 != other.y0
                || self.rect.x1 != other.x1
                || self.rect.y1 != other.y1
        } else {
            // Not the rect: what `apply_flow_size` would emit. A `Grow`/`Pct`
            // box's CSS is the same whatever this frame's solve made of it,
            // and a box that swapped one for the other needs rewriting even
            // at an identical size.
            self.flow_size != Some(size)
        }
    }

    fn style_differs(
        &self,
        bg: &Option<Color>,
        border: &Option<(Color, f32)>,
        radius: f32,
        inset: Inset,
        hot: &Option<(Color, Color)>,
        flow: &Option<FlowLayout>,
        text_color: Color,
        opacity: f32,
    ) -> bool {
        self.opacity != opacity
            || !opt_color_eq(&self.bg, bg)
            || !opt_border_eq(&self.border, border)
            || self.corner_radius != radius
            || self.inset != inset
            || !opt_color_pair_eq(&self.hot, hot)
            || !opt_flow_eq(&self.flow, flow)
            || !color_eq(self.text_color, text_color)
    }

    fn text_differs(&self, text: &Option<String>) -> bool {
        self.text.as_deref() != text.as_deref()
    }

    fn key_id_differs(&self, key_id: Option<&str>) -> bool {
        self.key_id.as_deref() != key_id
    }

    fn id_differs(&self, id: Option<&str>) -> bool {
        self.id.as_deref() != id
    }
}

/// Writes (or clears) `data-mae-id` on `el`. Called only when
/// `PaintSnapshot::id_differs` — an empty/absent id removes the attribute
/// rather than leaving a stale one from a reused `DomKey`.
fn apply_id(el: &Element, id: Option<&str>) {
    apply_attr(el, "data-mae-id", id);
}

fn apply_key_id(el: &Element, key_id: Option<&str>) {
    apply_attr(el, "data-mae-key", key_id);
}

fn apply_attr(el: &Element, name: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            let _ = el.set_attribute(name, value);
        }
        None => {
            let _ = el.remove_attribute(name);
        }
    }
}

/// Resolve a `beforeinput` target range's endpoint `(node, offset)` to a raw
/// buffer char offset, using the `data-raw-start`/`data-raw-end` attributes
/// `set_richtext_span`/`paint_richtext_host` stamped on our own host/row/
/// span/image elements. Always well-defined because a rich-text host's DOM
/// is never mutated by the browser — `attach_richtext_listeners` prevents
/// every `beforeinput` that isn't a composition event, so what's here is
/// always exactly what we last painted.
fn resolve_raw_offset(node: &Node, offset: u32) -> Option<usize> {
    if let Some(text) = node.dyn_ref::<Text>() {
        // A span's element has exactly one child: the text node
        // `paint_div`'s `set_text_content` gives it. `offset` is a UTF-16
        // code-unit count into that span's own text, and `data-raw-start` a
        // char index into the raw buffer, so the offset is converted rather
        // than added as-is — see `char_offset_from_utf16`.
        let parent = text.parent_element()?;
        let raw_start: usize = parent.get_attribute("data-raw-start")?.parse().ok()?;
        let within = char_offset_from_utf16(&text.data(), offset as usize);
        return Some(raw_start + within);
    }
    let el = node.dyn_ref::<Element>()?;
    // Not a text node: `offset` is a child index — land on that child's own
    // start (recursing; a plain row's own children are exactly the text
    // nodes handled above, so this only ever recurses one level deeper in
    // practice), or this container's own end when `offset` is past the last
    // child (e.g. a click after the last span on a line, or into the host
    // itself past its last row).
    if let Some(child) = el.child_nodes().item(offset) {
        return resolve_raw_offset(&child, 0);
    }
    if let Some(raw_end) = el
        .get_attribute("data-raw-end")
        .and_then(|s| s.parse().ok())
    {
        return Some(raw_end);
    }
    el.get_attribute("data-raw-start")?.parse().ok()
}

/// The inverse of [`resolve_raw_offset`]: find the DOM caret position for
/// raw buffer offset `raw`, by walking `host`'s own `data-raw-start`/
/// `data-raw-end` stamped descendants. Purely DOM-driven (no access to the
/// Rust-side anchor log), so it can run inside an event listener.
///
/// Each raw offset at an intra-row span boundary has *two* valid DOM
/// positions — end of the left span, start of the right one — so this
/// deliberately commits to one (the first span that strictly contains
/// `raw`, i.e. document order, preferring `raw < end`) to keep caret
/// motion single-valued; see the arrow-key handler in
/// `attach_richtext_listeners` for why that matters.
fn dom_caret_target_for_raw(host: &Element, raw: usize) -> Option<(Node, u32)> {
    let all = host.query_selector_all("[data-raw-start]").ok()?;
    let mut inside: Option<(Node, u32)> = None;
    let mut at_end: Option<(Node, u32)> = None;
    let mut atomic: Option<(Node, u32)> = None;
    for i in 0..all.length() {
        let Some(node) = all.item(i) else { continue };
        let Some(el) = node.dyn_ref::<Element>() else {
            continue;
        };
        let (Some(start), Some(end)) = (
            el.get_attribute("data-raw-start")
                .and_then(|s| s.parse::<usize>().ok()),
            el.get_attribute("data-raw-end")
                .and_then(|s| s.parse::<usize>().ok()),
        ) else {
            continue;
        };
        if raw < start || raw > end {
            continue;
        }
        match el.first_child() {
            // A leaf span: its sole child is the text node `paint_div`'s
            // `set_text_content` gave it. Rows (whose children are span
            // elements) and the host itself fall through to `_` below.
            Some(child) if child.node_type() == Node::TEXT_NODE => {
                // `raw`/`start` are char indices into the buffer; a DOM
                // caret offset counts UTF-16 code units into this span's own
                // text, so the distance between them has to be converted —
                // see `utf16_offset`.
                let offset = utf16_offset(&child.text_content().unwrap_or_default(), raw - start);
                if raw < end {
                    inside.get_or_insert((child, offset));
                } else {
                    at_end.get_or_insert((child, offset));
                }
            }
            // An empty line's zero-width spacer (see `emit_layout_line`):
            // no text to land in, so the caret goes to the element itself.
            None if start == end => {
                atomic.get_or_insert((el.clone().unchecked_into::<Node>(), 0));
            }
            _ => {}
        }
    }
    inside.or(at_end).or(atomic)
}

/// The `(raw_start, raw_end)` of each of a rich-text host's visual lines, in
/// document order — its direct children, one per line emitted by
/// `emit_layout_line` (a wrapped line is its own row, which is exactly what
/// vertical caret movement should step through). Rows never overlap: the
/// newline between two of them belongs to neither.
fn row_ranges(host: &Element) -> Vec<(usize, usize)> {
    let rows = host.child_nodes();
    let mut out = Vec::with_capacity(rows.length() as usize);
    for i in 0..rows.length() {
        let Some(row) = rows.item(i).and_then(|n| n.dyn_into::<Element>().ok()) else {
            continue;
        };
        let (Some(start), Some(end)) = (
            row.get_attribute("data-raw-start")
                .and_then(|s: String| s.parse::<usize>().ok()),
            row.get_attribute("data-raw-end")
                .and_then(|s: String| s.parse::<usize>().ok()),
        ) else {
            continue;
        };
        out.push((start, end));
    }
    out
}

fn flow_eq(a: FlowLayout, b: FlowLayout) -> bool {
    a.axis == b.axis
        && a.main_align == b.main_align
        && a.cross_align == b.cross_align
        && a.gap == b.gap
        && a.clip == b.clip
}
fn opt_flow_eq(a: &Option<FlowLayout>, b: &Option<FlowLayout>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => flow_eq(*a, *b),
        (None, None) => true,
        _ => false,
    }
}

fn color_eq(a: Color, b: Color) -> bool {
    a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a
}
fn opt_color_eq(a: &Option<Color>, b: &Option<Color>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => color_eq(*a, *b),
        (None, None) => true,
        _ => false,
    }
}
fn opt_border_eq(a: &Option<(Color, f32)>, b: &Option<(Color, f32)>) -> bool {
    match (a, b) {
        (Some((ac, aw)), Some((bc, bw))) => color_eq(*ac, *bc) && aw == bw,
        (None, None) => true,
        _ => false,
    }
}
fn opt_color_pair_eq(a: &Option<(Color, Color)>, b: &Option<(Color, Color)>) -> bool {
    match (a, b) {
        (Some((a0, a1)), Some((b0, b1))) => color_eq(*a0, *b0) && color_eq(*a1, *b1),
        (None, None) => true,
        _ => false,
    }
}

fn css_color(c: Color) -> String {
    format!(
        "rgba({}, {}, {}, {})",
        (c.r * 255.0).round(),
        (c.g * 255.0).round(),
        (c.b * 255.0).round(),
        c.a
    )
}

/// `style.font_icon` selects the Material icon glyph font instead of the
/// regular text font — same distinction `render/font_cache.rs` makes via
/// `FontTag::Icon` on native. See `www/index.html`'s two `@font-face` rules.
fn font_family_css(font_icon: bool) -> &'static str {
    if font_icon {
        "'Mae Icons'"
    } else {
        "'Mae Sans', sans-serif"
    }
}

fn flex_direction_css(axis: Axis) -> &'static str {
    match axis {
        Axis::X => "row",
        Axis::Y => "column",
    }
}

fn main_axis_align_css(a: MainAxisAlign) -> &'static str {
    match a {
        MainAxisAlign::Start => "flex-start",
        MainAxisAlign::Center => "center",
        MainAxisAlign::End => "flex-end",
        MainAxisAlign::SpaceBetween => "space-between",
        MainAxisAlign::SpaceAround => "space-around",
        MainAxisAlign::SpaceEvenly => "space-evenly",
    }
}

fn cross_axis_align_css(a: CrossAxisAlign) -> &'static str {
    match a {
        CrossAxisAlign::Start => "flex-start",
        CrossAxisAlign::Center => "center",
        CrossAxisAlign::End => "flex-end",
        CrossAxisAlign::Stretch => "stretch",
    }
}

/// A normal-flow box's layout intent, read once in `walk_dom` from fields
/// `paint_div` previously never saw — bundled to keep its signature sane.
#[derive(Clone, Copy)]
struct FlowLayout {
    axis: Axis,
    main_align: MainAxisAlign,
    cross_align: CrossAxisAlign,
    gap: f32,
    clip: bool,
}

/// How one axis of a normal-flow box is written to CSS.
///
/// Rust solves the whole layout either way — every rect a hit test, a
/// scrollbar or an anchored popover reads still comes from that solve. What
/// this decides is only what the *element* is told, and the difference shows
/// up whenever the browser reflows without asking Rust first: an on-screen
/// keyboard, a rotation, a window resize, a font finishing loading. A box
/// pinned to solved pixels keeps last frame's size until mae wakes up,
/// re-solves and rewrites it; one that declared `Fill` or a percentage is
/// re-laid-out by the browser on the spot, for nothing.
#[derive(Clone, Copy, PartialEq, Debug)]
enum CssLen {
    /// Rust's solved pixels — for `UISize::Pixels`, and for `ChildrenSum`
    /// (whose children may themselves be text).
    Px(f32),
    /// `UISize::TextContent`: hug the text. Carries mae's own measurement,
    /// which is what a plain `<div>` gets and what a hosted field falls back
    /// to — but a hosted field prefers `field-sizing: content` where the
    /// browser has it (see the `.mae-fit` rule), because mae shapes text with
    /// harfrust and the browser with its own engine, and the two do not agree
    /// to the pixel. They disagree by a lot on a touch device, where the
    /// field is rendered at the 16px floor that stops iOS zooming the page
    /// while mae measured the size the app asked for.
    FitText(f32),
    /// `UISize::ParentPct` — CSS resolves a percentage against the parent's
    /// content box, which is what mae's `apply_downward_size` does too.
    Pct(f32),
    /// `UISize::Fill` along the parent's *main* axis: an equal share of what
    /// is left after the fixed children and the gaps, which is exactly
    /// `flex: 1 1 0` (and exactly what `distribute_fill_children` computes).
    Grow,
    /// `UISize::Fill` across it, where mae treats Fill as the parent's whole
    /// content box — `align-self: stretch`, overriding whatever
    /// `align-items` the parent set.
    Stretch,
}

/// Both axes of a normal-flow box, in CSS terms.
#[derive(Clone, Copy, PartialEq, Debug)]
struct FlowSize {
    width: CssLen,
    height: CssLen,
    /// mae's `min_size`, which `enforce_constraints` applies after the solve.
    /// Always written out — a flex item's CSS default is `min-width: auto`,
    /// which refuses to shrink below its own content, and mae has no such
    /// rule: a box with no minimum of its own gets an explicit zero so a long
    /// word in a `Fill` box makes it scroll or clip, as it does on native,
    /// instead of pushing its siblings off the edge.
    min: (f32, f32),
}

/// Identity used for DOM-node reconciliation — deliberately *not* `UiKey`.
/// `UiKey` is zero for every anonymous box (`label()`, unnamed `row`/`column`
/// — see `alloc_box` in `widgets.rs`: it's only derived from an explicit
/// `##id` string, since native rendering has no need to track identity for
/// non-interactive boxes across frames). Most real UI content is exactly
/// that anonymous majority, so a DOM node's key is instead synthesized from
/// its position in the tree (`walk_dom`'s `path`, folding in each box's
/// stable `UiKey` when it has one, and its sibling index otherwise) — stable
/// across frames as long as the tree shape doesn't change above it.
type DomKey = u64;

struct DomEntry {
    node: DomNode,
    snapshot: PaintSnapshot,
    seen_this_frame: bool,
}

pub struct DomReconciler {
    document: Document,
    container: Element,
    nodes: HashMap<DomKey, DomEntry>,
    // A *queue* per key, not a single slot: the browser can fire several
    // `beforeinput`s (hence several of these) before the next `requestAnimat
    // ionFrame` tick ever consumes any of them — e.g. fast typing, or (found
    // this way) this exact scenario, `Input.dispatchKeyEvent`-driven test
    // characters sent back-to-back. A single `HashMap<UiKey, PendingDomEdit>`
    // slot here (this used to be one) makes every edit but the *last* in
    // such a burst silently vanish — not merely superseded: for `Range`,
    // unlike `Replace`, each one is a delta relative to a specific prior
    // state, not a full snapshot, so there's no way to recover a dropped
    // one's content from the one that overwrote it. `apply_pending_dom_edit`
    // applies every queued edit in order.
    pending_edits: Rc<RefCell<HashMap<UiKey, Vec<PendingDomEdit>>>>,
    // Keeps every hosted `<input>`/`<textarea>`'s and every clickable box's
    // event-listener closures alive for as long as its DOM node exists.
    dom_listeners: HashMap<DomKey, Vec<Closure<dyn FnMut(web_sys::Event)>>>,
    // blob: URLs handed to <img> elements (see `paint_image`) — revoked when
    // the element is removed, or they'd leak for the page's lifetime.
    image_urls: HashMap<DomKey, String>,
    // Wakes `run_dom`'s tick so a hosted element's edit gets consumed on the
    // next frame — see the comment on `IMUI::new_dom`.
    waker: RepaintWaker,
    // The last node placed under each mount point so far this frame — see
    // `reappend_if_needed`. Cleared at the start of every frame.
    last_appended_in_frame: HashMap<Option<DomKey>, Node>,
    // Native per-element hover state for `MOUSE_CLICKABLE` boxes — see
    // `attach_interactive_listeners` and `imui/input.rs`'s `#[cfg(feature =
    // "dom")]` branch of `signal_from_key_and_flags`. Level-triggered
    // (`pointerenter` inserts, `pointerleave` removes), unlike the
    // edge-triggered press/click state below, since app code can poll
    // `.hovering()` on any frame, not just the transition frame.
    dom_hover: Rc<RefCell<HashSet<UiKey>>>,
    // Native per-element press/release/click state, edge-triggered and
    // consumed ("take") once per frame by `signal_from_key_and_flags`,
    // exactly like `pending_edits` above.
    dom_pointer_edges: Rc<RefCell<HashMap<UiKey, DomPointerEdge>>>,
    /// `UiKey` of every node carrying interaction listeners.
    ///
    /// `dom_hover` and `dom_pointer_edges` are keyed by `UiKey` while the node
    /// table is keyed by `DomKey`, so without this mapping a removed node's
    /// pointer state could never be cleaned up — see `remove`.
    interactive_keys: HashMap<DomKey, UiKey>,
    /// `UiKey`s whose node was removed this frame, drained by `draw_ui_dom`.
    removed_interactive: Vec<UiKey>,
    // Bytes of an image pasted into a rich-text host, read asynchronously
    // off the clipboard by `attach_richtext_listeners`'s `paste` handler and
    // collected by `IMUI::take_pasted_image` — the same API native fills in
    // from `os::clipboard_get_image`, so the host app's handling is shared.
    pending_pasted_image: Rc<RefCell<Option<Vec<u8>>>>,
    // `Some` only while `walk_dom` is inside a `RICH_TEXT_HOST`'s own
    // subtree — populated by `set_richtext_span`'s stamping as each row/
    // span/image child is painted, consumed by `sync_richtext_caret` right
    // after that host's children finish. `None` the rest of the time (most
    // boxes in the tree, which have nothing to do with rich text).
    richtext_log: Option<Vec<(RichTextAnchorKind, usize, usize, DomKey)>>,
    // The raw cursor offset last used to position the browser's own
    // selection for a given rich-text host, so `sync_richtext_caret` only
    // touches `Selection` (and risks interrupting an in-progress native
    // mouse-drag-select) when the cursor genuinely moved since last synced.
    // Shared (not a plain `HashMap`) because `attach_richtext_listeners`'s
    // `selectionchange` listener also reads it — see that listener's own
    // comment for why: `set_base_and_extent` itself fires a real
    // `selectionchange`, and without this check that *echo* gets fed back
    // through `pending_selection` as if the user had moved the caret there
    // themselves, fighting the very edit that just happened (this was the
    // actual cause of a "second character typed lands at the start of the
    // buffer" bug: edit N's cursor got immediately clobbered by edit
    // N-1's now-stale echo, landing one edit behind on every keystroke).
    // Stored as `(anchor, focus)` so a real selection range counts as a
    // distinct state from a collapsed caret at either of its ends —
    // otherwise dragging a selection whose focus happens to sit where the
    // caret already was would look "unchanged" and never be restored.
    richtext_synced_cursor: Rc<RefCell<HashMap<UiKey, (usize, usize)>>>,
    // A rich-text host's live caret position, as last reported by the
    // browser's own `selectionchange` — see `attach_richtext_listeners`.
    // Consumed once per frame by `IMUI::apply_pending_dom_selection`
    // (`textarea_impl`, before the native pixel-based click-to-cursor path
    // runs — see that path's own doc comment for why it's skipped here):
    // Rust's own glyph-advance math (`cum_x`, built from its own text
    // shaping) has no reason to line up pixel-for-pixel with the *browser's*
    // text layout, so deriving the caret from a click's pixel position would
    // just fight the browser's own — already correct — placement every
    // repaint (this was the actual cause of a "types before the existing
    // text" bug: every rebuild after an edit clobbered the browser's real
    // caret with a wrong Rust-computed one).
    // `(anchor, focus)`, both raw buffer offsets — equal for a plain caret,
    // different for a real selection (drag, Shift+arrow, select-all).
    pending_selection: Rc<RefCell<HashMap<UiKey, (usize, usize)>>>,
    // `selectionchange` fires on `document`, not an element — unlike every
    // other listener here (stored in `dom_listeners`, dropped for free when
    // that element goes away), this one must be explicitly unregistered from
    // `document` in `remove()`, or the browser would still hold a reference
    // to it after the underlying `Closure` is dropped (a wasm-bindgen panic
    // the next time `selectionchange` fires).
    richtext_selection_listeners: HashMap<DomKey, Closure<dyn FnMut(web_sys::Event)>>,
    // A text-editing box's native `focus`/`blur` state since it was last
    // consumed (`true` = gained focus, `false` = lost it) — see `IMUI::
    // apply_pending_dom_focus`. `signal_from_key_and_flags`'s usual
    // `click_to_focus`-driven path (`apply_click_to_focus`, keyed off a
    // box's own `pressed`/`clicked` signal) never fires for a `MOUSE_
    // CLICKABLE` + `TEXT_INPUT` box on this backend: `dom_pointer_state`
    // returns `Some` (all-`false`) for *any* `MOUSE_CLICKABLE` box once a
    // `DomReconciler` exists, whether or not `attach_interactive_listeners`
    // ever actually tracked it — which text-input boxes never do (only
    // `attach_input_listeners`/`attach_richtext_listeners`, which have no
    // press/click tracking of their own) — so that `Some` permanently
    // short-circuits `signal_from_key_and_flags` past its geometric
    // fallback, and `self.focus_key` never gets set for one at all, no
    // matter how much the browser's own DOM disagrees. Found via `apply_
    // click_to_focus`-instrumented investigation of the actual root cause
    // behind `richtext_synced_cursor`'s doc comment ("second character…
    // lands at the start"): `sync_richtext_caret` was silently never
    // running at all (its `self.focus_key == Some(ui_key)` guard was
    // always false), so nothing ever corrected the caret the browser
    // itself defaults a removed-node's selection to after a rebuild.
    pending_focus: Rc<RefCell<HashMap<UiKey, bool>>>,
    /// Where the browser has scrolled each native scroller since mae last
    /// looked, keyed by the box's own `UiKey` — which is always a real one:
    /// only a box with an explicit `##id` survives from frame to frame
    /// (`alloc_box`), so an anonymous box could never have kept a scroll
    /// offset in the first place.
    ///
    /// The browser owns scrolling on this backend; this is the one wire back,
    /// so mae's `scroll`/`scroll_target` keep describing where the content
    /// actually is (`IMUI::adopt_dom_scrolls`, run before the build so the
    /// frame lays out against it).
    pending_scrolls: Rc<RefCell<HashMap<UiKey, (f32, f32)>>>,
    /// Which scrollers already have their `scroll` listener attached.
    scroll_listeners: HashMap<DomKey, Closure<dyn FnMut(web_sys::Event)>>,
    // The `UiKey` whose hosted element this reconciler last pushed the
    // browser's own focus onto — see `sync_hosted_focus`. Only an element
    // *this* drove focus into is ever blurred by it, so a focus the browser
    // itself owns (an element outside `#mae-root`, or one Rust never
    // rendered) is never stolen away.
    driven_focus: Option<UiKey>,
    // The `(anchor, cursor)` last pushed onto a plain `<input>`/`<textarea>`
    // via `set_selection_range`, per box. Same role `richtext_synced_cursor`
    // plays for a rich-text host: Rust only ever knows a plain field's caret
    // from the last `input` event (`attach_input_listeners`'s `read_back`) —
    // arrow keys, Home/End and drag-selection inside one are the browser's
    // own and never reported — so pushing the Rust-side caret unconditionally
    // every frame would wipe out a selection the user made with the mouse.
    // Pushing only on an actual *change* means a Rust-driven move (`IMUI::
    // set_textarea_cursor`, e.g. "select the placeholder name so typing
    // replaces it") lands, while a browser-owned caret is left alone.
    synced_input_caret: HashMap<UiKey, (usize, usize)>,
    /// Hash of everything a collaborator-caret overlay is built from, per
    /// overlay — see `paint_remote_carets`, which rebuilds only when it
    /// changes. Dropped with the overlay's node in `remove`.
    caret_signatures: HashMap<DomKey, u64>,
}

impl DomReconciler {
    pub fn new(container: &HtmlElement, waker: RepaintWaker) -> Self {
        let document = container
            .owner_document()
            .expect("container has no owner document");
        // Hover/active feedback for DRAW_HOT_EFFECTS boxes (buttons, nav items) is
        // delegated to the browser's own `:hover`/`:active` — see `paint_div`, which
        // sets `--mae-hover-bg`/`--mae-active-bg` per box and adds this class. That's
        // what lets `run_dom`'s tick (lifecycle.rs) skip rebuilding on plain mouse
        // movement: the visual feedback no longer depends on a Rust rebuild at all.
        //
        // The first two rules extend that from hover to *every* eased visual
        // change. `animate_visual_state`/`animate_scroll_offsets` (paint.rs,
        // scroll.rs) interpolate a box's colours, focus ring, appearance and
        // scroll offset a step per frame, and ask for another frame for as
        // long as anything is still moving — so on this backend a popover
        // fading in or a scroll settling used to rebuild and re-diff the whole
        // app at 60fps for its whole duration. They now write the *target*
        // straight out (`IMUI::css_drives_animation`) and these two rules do
        // the easing instead: a transition for colour changes, and a one-shot
        // keyframe for a floating pane appearing (`paint_div` adds
        // `.mae-appear` when it creates one — deliberately opacity-only, the
        // same thing native's `appear_t` does, since a `transform` here would
        // become the containing block for the pane's own positioned children
        // mid-animation). Both run off the main thread, and the loop can go
        // idle the frame after the state changed. Opacity is left out of the
        // transition on purpose: an app driving its own fade (enkr's view
        // transition) writes a new value every frame, and easing an already
        // eased value just adds lag.
        //
        // The `[contenteditable="true"] *` rule re-enables hit-testing and
        // text selection inside a `RICH_TEXT_HOST`. `paint_div` sets
        // `pointer-events: none` on every non-clickable box so clicks fall
        // through to whatever is actually interactive underneath — correct
        // everywhere else, but a rich-text host's rows and spans *are* the
        // text, and with them untargetable the browser cannot hit-test a
        // drag across them: selection came back empty even mid-drag
        // (confirmed against real Chromium — a plain click still placed a
        // caret, since that resolves against the focused host itself, which
        // is why this went unnoticed). `!important` because `paint_div`
        // writes `pointer-events` as an *inline* style, which a plain rule
        // could never override — same reason the `.mae-hot` rules above
        // need it.
        //
        // The last three rules are for touch. The grey flash a browser paints
        // over a tapped element is its own affordance, and duplicates (badly)
        // the `.mae-hot:active` one above. And the container's
        // `touch-action: pinch-zoom` (`os/wasm.rs`) inherits, which would
        // stop a hosted `<textarea>` scrolling under a finger — hosted text
        // is the one place the browser, not mae, owns panning, so it gets
        // `auto` back; `overscroll-behavior: contain` then keeps a scroll
        // past its end from dragging the page behind it.
        //
        // The last rule stops the *page* from zooming when a field is
        // focused. iOS Safari zooms in on any focused text field whose font
        // is under 16px — mae's default text is 14 — and then leaves the
        // whole layout scaled and panned sideways, so tapping a note to type
        // in it threw the app off screen. The threshold is what the browser
        // reads, so the fix is to meet it rather than to fight the zoom
        // afterwards (`maximum-scale=1` in the viewport would also take the
        // *user's* pinch away on Android, which is the opposite of what is
        // wanted). Fields carry their requested size as `--mae-font-size`,
        // so this only ever raises a size below the threshold and leaves a
        // larger one alone; on a mouse-driven page it does not apply at all.
        //
        // Which is exactly why the rule after it exists. A field that asked
        // to hug its text (`UISize::TextContent` — the note title) is sized
        // from mae's own measurement of that text, and mae measured it at the
        // size the app asked for, not the floored one: on a phone the box
        // came out ~12px too narrow for the text actually drawn in it, and
        // the end of the title was clipped. `field-sizing: content` hands
        // that measurement to the browser, which is the only party that knows
        // what it will really draw. Behind `@supports` because the fallback —
        // `width: auto` on an `<input>` — is a default ~20-character box,
        // much worse than mae's estimate; without support the inline pixels
        // `apply_flow_size` writes stand, as they did before.
        let style_el = document
            .create_element("style")
            .expect("create style element");
        let _ = container.class_list().add_1("mae-scope");
        style_el.set_text_content(Some(
            ".mae-scope * { transition: background-color 120ms ease, border-color 120ms ease; }\
             @keyframes mae-appear { from { opacity: 0; } }\
             .mae-appear { animation: mae-appear 120ms ease-out; }\
             .mae-hot:hover { background: var(--mae-hover-bg) !important; }\
             .mae-hot:active { background: var(--mae-active-bg) !important; }\
             button.mae-btn { appearance: none; -webkit-appearance: none; margin: 0; \
             font: inherit; color: inherit; text-align: inherit; cursor: pointer; outline: none; }\
             [contenteditable=\"true\"] * { pointer-events: auto !important; \
             user-select: text !important; -webkit-user-select: text !important; }\
             * { -webkit-tap-highlight-color: transparent; }\
             input, textarea, [contenteditable=\"true\"] { touch-action: auto; \
             overscroll-behavior: contain; }\
             @media (pointer: coarse) { input, textarea, [contenteditable=\"true\"] { \
             font-size: max(16px, var(--mae-font-size, 16px)) !important; } }\
             @supports (field-sizing: content) { .mae-scope input.mae-fit, \
             .mae-scope textarea.mae-fit { field-sizing: content; width: auto !important; \
             min-width: 2ch; } }",
        ));
        let _ = container.append_child(&style_el);
        DomReconciler {
            document,
            container: container.clone().into(),
            nodes: HashMap::new(),
            pending_edits: Rc::new(RefCell::new(HashMap::new())),
            dom_listeners: HashMap::new(),
            image_urls: HashMap::new(),
            waker,
            last_appended_in_frame: HashMap::new(),
            dom_hover: Rc::new(RefCell::new(HashSet::new())),
            dom_pointer_edges: Rc::new(RefCell::new(HashMap::new())),
            interactive_keys: HashMap::new(),
            removed_interactive: Vec::new(),
            pending_pasted_image: Rc::new(RefCell::new(None)),
            richtext_log: None,
            richtext_synced_cursor: Rc::new(RefCell::new(HashMap::new())),
            pending_selection: Rc::new(RefCell::new(HashMap::new())),
            richtext_selection_listeners: HashMap::new(),
            pending_focus: Rc::new(RefCell::new(HashMap::new())),
            pending_scrolls: Rc::new(RefCell::new(HashMap::new())),
            scroll_listeners: HashMap::new(),
            driven_focus: None,
            synced_input_caret: HashMap::new(),
            caret_signatures: HashMap::new(),
        }
    }

    fn begin_frame(&mut self) {
        self.last_appended_in_frame.clear();
        for entry in self.nodes.values_mut() {
            entry.seen_this_frame = false;
        }
    }

    /// Detach and forget DOM nodes for boxes that weren't painted this frame
    /// (removed/hidden/fully-clipped boxes) — mirrors native's implicit
    /// box-pool reuse (`IMUI::prune_boxes`).
    fn end_frame(&mut self) {
        let stale: Vec<DomKey> = self
            .nodes
            .iter()
            .filter(|(_, e)| !e.seen_this_frame)
            .map(|(k, _)| *k)
            .collect();
        for key in stale {
            self.remove(key);
        }
    }

    /// Stamp `data-raw-start`/`data-raw-end` on the element for `key` (a
    /// rich-text host's row/span/image child — see `UIBox::richtext_span`),
    /// only touching the DOM when the value actually changed (this runs for
    /// every such node on every frame, most of which touch none of them).
    fn set_richtext_span(&mut self, key: DomKey, span: (usize, usize)) {
        let Some(entry) = self.nodes.get_mut(&key) else {
            return;
        };
        if entry.snapshot.richtext_span == Some(span) {
            return;
        }
        let el = entry.node.as_html_element().clone();
        let _ = el.set_attribute("data-raw-start", &span.0.to_string());
        let _ = el.set_attribute("data-raw-end", &span.1.to_string());
        entry.snapshot.richtext_span = Some(span);
    }

    fn element_for(&self, key: DomKey) -> Option<HtmlElement> {
        self.nodes
            .get(&key)
            .map(|e| e.node.as_html_element().clone())
    }

    /// Restore the browser's own selection to `(anchor, focus)` (raw buffer
    /// offsets, equal for a plain caret), using `log` — the row/span/image
    /// ranges `set_richtext_span` collected while painting this host's
    /// children this frame. A no-op if neither end moved since the last
    /// time this host was synced (see `richtext_synced_cursor`'s doc
    /// comment) — otherwise every repaint of a focused rich-text host would
    /// reset the browser's own selection, which would make even the
    /// browser's native click-drag-to-select impossible to complete (each
    /// in-progress frame would snap it back).
    ///
    /// The anchor matters as much as the focus: syncing only a collapsed
    /// caret here is what previously made selecting text impossible on this
    /// backend at all. A drag fires `selectionchange` continuously, each one
    /// waking a rebuild, and every one of those rebuilds collapsed the
    /// half-made selection back to its focus point — so the selection could
    /// never grow, mid-drag or after.
    fn sync_richtext_caret(
        &mut self,
        ui_key: UiKey,
        log: &[(RichTextAnchorKind, usize, usize, DomKey)],
        anchor: usize,
        focus: usize,
    ) {
        if self.richtext_synced_cursor.borrow().get(&ui_key) == Some(&(anchor, focus)) {
            return;
        }
        self.richtext_synced_cursor
            .borrow_mut()
            .insert(ui_key, (anchor, focus));
        let Some((focus_node, focus_offset)) = self.resolve_caret_target(log, focus) else {
            return;
        };
        let (anchor_node, anchor_offset) = if anchor == focus {
            (focus_node.clone(), focus_offset)
        } else {
            match self.resolve_caret_target(log, anchor) {
                Some(target) => target,
                None => (focus_node.clone(), focus_offset),
            }
        };
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(Some(selection)) = window.get_selection() else {
            return;
        };
        let _ =
            selection.set_base_and_extent(&anchor_node, anchor_offset, &focus_node, focus_offset);
    }

    /// Find the best `log` entry for `cursor` (containing it, preferring a
    /// `Text` anchor when more than one does — an image/spacer's `Atomic`
    /// range only wins when `cursor` is nowhere inside any span, e.g. an
    /// empty line — then falling back to whichever entry is numerically
    /// closest, for a `cursor` past the end of the last line), and resolve
    /// it to a concrete `(Node, offset)`.
    fn resolve_caret_target(
        &self,
        log: &[(RichTextAnchorKind, usize, usize, DomKey)],
        cursor: usize,
    ) -> Option<(Node, u32)> {
        let mut best: Option<(RichTextAnchorKind, usize, DomKey, i64)> = None;
        for &(kind, start, end, key) in log {
            let contains = cursor >= start && cursor <= end;
            let dist = if cursor < start {
                (start - cursor) as i64
            } else if cursor > end {
                (cursor - end) as i64
            } else {
                0
            };
            let text_bonus = matches!(kind, RichTextAnchorKind::Text) as i64;
            let score = (contains as i64) * 1_000_000 + text_bonus * 1000 - dist;
            let better = match best {
                None => true,
                Some((_, _, _, best_score)) => score > best_score,
            };
            if better {
                best = Some((kind, start, key, score));
            }
        }
        let (kind, start, key, _) = best?;
        let el = self.element_for(key)?;
        match kind {
            RichTextAnchorKind::Text => {
                let text_node = el.first_child()?;
                let text = text_node.text_content().unwrap_or_default();
                let len = text.encode_utf16().count() as u32;
                // `cursor`/`start` are char indices; a DOM caret offset counts
                // UTF-16 code units — see `utf16_offset`.
                let intra = utf16_offset(&text, cursor.saturating_sub(start));
                Some((text_node, intra.min(len)))
            }
            RichTextAnchorKind::Atomic => {
                if cursor <= start {
                    Some((el.unchecked_into::<Node>(), 0))
                } else {
                    // These atomics (an image line, an empty-line spacer)
                    // are always their row's only child (see `emit_layout_
                    // line`) — landing right after them in their parent is
                    // always child index 1.
                    let parent = el.parent_node()?;
                    Some((parent, 1))
                }
            }
        }
    }

    fn remove(&mut self, key: DomKey) {
        if let Some(entry) = self.nodes.remove(&key) {
            // `Element::remove()` (the `ChildNode` mixin), not
            // `self.container.remove_child(...)`: nodes now nest under
            // their real parent (see `mount_element`), not always the flat
            // root, so we can't assume `self.container` is the actual
            // current parent to call `removeChild` on.
            entry.node.as_html_element().remove();
        }
        self.dom_listeners.remove(&key);
        self.scroll_listeners.remove(&key);
        self.caret_signatures.remove(&key);
        // Pointer state has to go with the node. `pointerleave` never fires for
        // an element removed from under the cursor, so a stale `dom_hover`
        // entry would keep reporting hover — and because `UiKey` is a hash of
        // the widget id, the *next* box built with that id inherits it, which
        // is how a tooltip ends up sitting on screen with the pointer nowhere
        // near it. Un-consumed edges are worse than cosmetic: a leftover
        // `left_pressed` re-arms the exclusive active key when the id comes
        // back, swallowing the next genuine click.
        if let Some(ui_key) = self.interactive_keys.remove(&key) {
            self.dom_hover.borrow_mut().remove(&ui_key);
            self.dom_pointer_edges.borrow_mut().remove(&ui_key);
            self.removed_interactive.push(ui_key);
        }
        if let Some(url) = self.image_urls.remove(&key) {
            let _ = Url::revoke_object_url(&url);
        }
        // See `richtext_selection_listeners`'s doc comment: this one needs
        // explicit `removeEventListener`, not just dropping.
        if let Some(listener) = self.richtext_selection_listeners.remove(&key) {
            let _ = self.document.remove_event_listener_with_callback(
                "selectionchange",
                listener.as_ref().unchecked_ref(),
            );
        }
    }

    /// Where a newly-created node should be appended: `Some(k)` means "as a
    /// real DOM child of the node painted for box `k`" (normal flow, or a
    /// scrollable box's content wrapper — see `ensure_scroll_wrapper`);
    /// `None` means the flat root (`#mae-root`), for floating panes/tooltips
    /// (`FLOATING_X`/`FLOATING_Y` — genuinely outside normal flow, positioned
    /// by an explicit `fixed_position` — see `walk_dom`) and the
    /// scrollbar-thumb overlay (`paint_scrollbar_thumb`, mounted as a child
    /// of its own scroll container).
    fn mount_element(&self, mount_point: Option<DomKey>) -> Element {
        mount_point
            .and_then(|k| self.nodes.get(&k))
            .map(|e| e.node.as_html_element().clone().unchecked_into::<Element>())
            .unwrap_or_else(|| self.container.clone())
    }

    /// Place `el` as `parent`'s child immediately after whatever was last
    /// placed under this same `mount_point` this frame (or as the first
    /// child, if `el` is the first thing painted there) — but only moves it
    /// if it isn't already there.
    ///
    /// Every `paint_*` call re-places its node each frame so a *reused*
    /// node's DOM position stays correct when sibling order changes (see
    /// `paint_div`'s comment) — but naively calling `appendChild`
    /// unconditionally for every child, in order, also gets the final order
    /// right, except it does so by detaching and reattaching *every*
    /// sibling every single frame (appending an already-last child is a
    /// no-op move, but appending any *earlier* sibling first displaces it,
    /// so each one keeps needing to move again next). Detaching a
    /// currently-focused hosted `<input>`/`<textarea>` (or any ancestor of
    /// one) blurs it — that made typing into a hosted field impossible, since
    /// focus was stolen back within one frame of every click. Tracking the
    /// correct target position directly (`last_appended_in_frame`, cleared
    /// each `begin_frame`) and using `insertBefore` only when a node is
    /// actually out of place reproduces the same end result while touching
    /// the DOM only for nodes that actually moved.
    fn reappend_if_needed(
        last_appended: &mut HashMap<Option<DomKey>, Node>,
        mount_point: Option<DomKey>,
        parent: &Element,
        el: &HtmlElement,
    ) {
        let node: &Node = el.unchecked_ref();
        let prev = last_appended.get(&mount_point).cloned();
        let reference = match &prev {
            Some(p) => p.next_sibling(),
            None => parent.first_child(),
        };
        let already_correct = match &reference {
            Some(r) => r.is_same_node(Some(node)),
            None => {
                node.parent_node()
                    .is_some_and(|p| p.is_same_node(Some(parent.unchecked_ref())))
                    && node.next_sibling().is_none()
            }
        };
        if !already_correct {
            let _ = parent.insert_before(node, reference.as_ref());
        }
        last_appended.insert(mount_point, node.clone());
    }

    /// Floating/absolute geometry: Rust sets both position and size — used
    /// for `FLOATING_X`/`FLOATING_Y` boxes and the scrollbar-thumb overlay,
    /// exactly as before this change.
    fn position(el: &HtmlElement, rect: &RectCoords) {
        let style = el.style();
        let _ = style.set_property("position", "absolute");
        let _ = style.set_property("left", &format!("{}px", rect.x0));
        let _ = style.set_property("top", &format!("{}px", rect.y0));
        let _ = style.set_property("width", &format!("{}px", (rect.x1 - rect.x0).max(0.0)));
        let _ = style.set_property("height", &format!("{}px", (rect.y1 - rect.y0).max(0.0)));
    }

    /// Normal-flow *size*: CSS flexbox decides position (see
    /// `apply_flex_container`), and this decides what the element is told
    /// about its own size — a declared `Fill`/percentage where the box has
    /// one, Rust's solved pixels otherwise. See [`CssLen`] for why the
    /// distinction matters, and `flow_size_for` for which boxes get which.
    ///
    /// Rust's solve stays authoritative for every rect anything *reads*
    /// (hit tests, scrollbar math, anchored popovers): the two agree in the
    /// steady state, because CSS is being handed the same declarations mae
    /// resolved. They diverge only while the browser has reflowed and mae
    /// has not caught up yet — and there the browser is the one that is
    /// right, which is the entire point.
    fn apply_flow_size(el: &HtmlElement, size: FlowSize) {
        let style = el.style();
        let _ = style.set_property("position", "static");
        // `flex` is the main axis only, so a box that grows along it is
        // still free to be a percentage or a fixed size across it.
        let grows = size.width == CssLen::Grow || size.height == CssLen::Grow;
        let _ = style.set_property("flex", if grows { "1 1 0" } else { "0 0 auto" });
        if size.width == CssLen::Stretch || size.height == CssLen::Stretch {
            let _ = style.set_property("align-self", "stretch");
        } else {
            let _ = style.remove_property("align-self");
        }
        let _ = style.set_property("min-width", &format!("{}px", size.min.0));
        let _ = style.set_property("min-height", &format!("{}px", size.min.1));
        for (property, len) in [("width", size.width), ("height", size.height)] {
            match len {
                // `FitText` writes the measurement too: it is what a plain
                // element gets, and the fallback for a hosted field in a
                // browser without `field-sizing`.
                CssLen::Px(px) | CssLen::FitText(px) => {
                    let _ = style.set_property(property, &format!("{px}px"));
                }
                CssLen::Pct(pct) => {
                    let _ = style.set_property(property, &format!("{pct}%"));
                }
                // Both are the absence of a size: `flex-basis: 0` owns the
                // main axis, `align-self: stretch` the cross one, and either
                // would be overridden by an explicit length here.
                CssLen::Grow | CssLen::Stretch => {
                    let _ = style.set_property(property, "auto");
                }
            }
        }
    }

    /// Makes `el` a flex container for its children, matching mae's own
    /// row/column model directly: `flex-direction`/`justify-content`/
    /// `align-items`/`gap` from `child_layout_axis`/`main_axis_align`/
    /// `cross_axis_align`/`child_gap`. Applied to a box's own element
    /// normally, or to a scrollable box's content wrapper instead (see
    /// `ensure_scroll_wrapper`) — a scrollable box's own element only needs
    /// to size + clip, not also act as a flex container for a single
    /// wrapper child.
    fn apply_flex_container(el: &HtmlElement, flow: FlowLayout) {
        let style = el.style();
        let _ = style.set_property("display", "flex");
        let _ = style.set_property("flex-direction", flex_direction_css(flow.axis));
        let _ = style.set_property("justify-content", main_axis_align_css(flow.main_align));
        let _ = style.set_property("align-items", cross_axis_align_css(flow.cross_align));
        let _ = style.set_property("gap", &format!("{}px", flow.gap));
        let _ = style.set_property("overflow", if flow.clip { "hidden" } else { "visible" });
    }

    /// A scrollable box (`SCROLL_X`/`SCROLL_Y`) delegates its children to an
    /// inner wrapper instead of hosting them directly: the box itself only
    /// sizes + clips (`overflow: hidden`, set by the caller), while the
    /// wrapper is the real flex container, shifted by `-scroll` via
    /// `transform` — this reproduces what `layout.rs` already does for
    /// native (subtracting `scroll` from each child's computed position;
    /// see `layout.rs`'s `pos -= self.boxes[parent].scroll.x/y`), just
    /// expressed as one CSS transform instead of per-child math, since real
    /// DOM nesting means CSS flow now computes each child's position itself.
    /// Returns the wrapper's `DomKey` for children to mount under.
    #[allow(clippy::too_many_arguments)]
    fn ensure_scroll_wrapper(
        &mut self,
        box_key: DomKey,
        box_el: &HtmlElement,
        ui_key: UiKey,
        flow: FlowLayout,
        scroll: Point,
        scrolls_x: bool,
        scrolls_y: bool,
    ) -> DomKey {
        // The outer box is the scroller (see `paint_div`), so that is what
        // reports where the browser put it.
        self.attach_scroll_listener(box_key, ui_key, box_el);
        // `scroll` here is mae's *target* (see `walk_dom`). Writing it only
        // when the element is not already there is what keeps this from
        // fighting the user: a scroll they performed arrives through the
        // listener as both `scroll` and `scroll_target`, leaving nothing to
        // push, while a programmatic jump (`scroll_to_y`) is a target the
        // element has never been at.
        if scrolls_x && (box_el.scroll_left() as f32 - scroll.x()).abs() >= 1.0 {
            box_el.set_scroll_left(scroll.x() as i32);
        }
        if scrolls_y && (box_el.scroll_top() as f32 - scroll.y()).abs() >= 1.0 {
            box_el.set_scroll_top(scroll.y() as i32);
        }
        let wrapper_key = box_key.wrapping_add(0xFFFF_FFFF_FFFF_FFF3);
        if !matches!(self.nodes.get(&wrapper_key), Some(e) if matches!(e.node, DomNode::Div(_))) {
            self.remove(wrapper_key);
            let el: HtmlElement = self
                .document
                .create_element("div")
                .expect("create div")
                .dyn_into()
                .expect("div is an HtmlElement");
            let _ = box_el.append_child(&el);
            self.nodes.insert(
                wrapper_key,
                DomEntry {
                    node: DomNode::Div(el),
                    snapshot: PaintSnapshot::blank(),
                    seen_this_frame: false,
                },
            );
        }
        let entry = self.nodes.get_mut(&wrapper_key).expect("just inserted");
        entry.seen_this_frame = true;
        let DomNode::Div(el) = &entry.node else {
            unreachable!()
        };
        if !opt_flow_eq(&entry.snapshot.flow, &Some(flow)) {
            // The wrapper is what actually holds the (possibly oversized)
            // content and gets shifted via `transform` below — clipping is
            // the *outer* box's job alone (`overflow: hidden`, set by the
            // caller). Reusing `flow.clip` here too would give the wrapper
            // its own `overflow: hidden` at its own (unscrolled) size, which
            // silently truncates anything past one wrapper-width/height's
            // worth of content no matter how far the transform scrolls it.
            Self::apply_flex_container(
                el,
                FlowLayout {
                    clip: false,
                    ..flow
                },
            );
            let _ = el.style().set_property("flex", "0 0 auto");
            // Only the cross axis (the one *not* scrolled) should track the
            // outer box's size; the scrolled axis must shrink-wrap to its
            // content instead of filling the outer box's (viewport) size.
            // `height: auto` already shrink-wraps for a normal block box, but
            // `width: auto` does the opposite — it fills the containing
            // block — so the scrolled-X case needs an explicit
            // `max-content`, or the wrapper (and everything past one
            // viewport-width of children) never grows wide enough to hold
            // its content, no matter how far the transform scrolls it.
            let style = el.style();
            if scrolls_y && !scrolls_x {
                let _ = style.set_property("width", "100%");
                let _ = style.remove_property("height");
            } else if scrolls_x && !scrolls_y {
                let _ = style.set_property("height", "100%");
                let _ = style.set_property("width", "max-content");
            } else {
                let _ = style.remove_property("width");
                let _ = style.remove_property("height");
            }
        }
        // No transform: the outer box is a real scroller now (`paint_div`),
        // so the offset lives in its `scrollLeft`/`scrollTop` and the browser
        // is what moves the content. This wrapper is left purely as the
        // shrink-wrapping flex container that gives the scroller something
        // taller (or wider) than itself to scroll.
        let _ = el.style().remove_property("transform");
        entry.snapshot = PaintSnapshot {
            flow: Some(flow),
            ..PaintSnapshot::blank()
        };
        wrapper_key
    }

    fn apply_paint_style(
        el: &HtmlElement,
        bg: &Option<Color>,
        border: &Option<(Color, f32)>,
        corner_radius: f32,
        inset: Inset,
        opacity: f32,
    ) {
        let style = el.style();
        let _ = style.set_property("opacity", &format!("{opacity}"));
        let _ = style.set_property(
            "background",
            &bg.map(css_color)
                .unwrap_or_else(|| "transparent".to_string()),
        );
        match border {
            Some((color, width)) => {
                let _ = style.set_property(
                    "border",
                    &format!("{}px solid {}", width, css_color(*color)),
                );
            }
            None => {
                let _ = style.set_property("border", "none");
            }
        }
        let _ = style.set_property("border-radius", &format!("{corner_radius}px"));
        let (top, right, bottom, left) = inset;
        let _ = style.set_property("padding", &format!("{top}px {right}px {bottom}px {left}px"));
    }

    /// The font size of a *hosted* field (`<input>`, `<textarea>`,
    /// `contenteditable`), written so the touch rule in the injected
    /// stylesheet can raise it to the 16px iOS needs to leave the page alone.
    ///
    /// The size goes in as `--mae-font-size` as well as `font-size`, because
    /// an inline declaration beats any plain rule: the media query reads the
    /// custom property back out and takes `max(16px, it)`, so a field larger
    /// than the threshold keeps its own size and a mouse-driven page is
    /// untouched. Nothing else on the page needs this — only a focused field
    /// triggers the zoom.
    fn apply_hosted_font_size(style: &web_sys::CssStyleDeclaration, font_size: f32) {
        let px = format!("{font_size}px");
        let _ = style.set_property("--mae-font-size", &px);
        let _ = style.set_property("font-size", &px);
    }

    /// `padding.X + style.margin` — the same inset `paint.rs` uses for text
    /// (`content_left`/`content_y0` in `draw_ui_root_skipping_clipped`).
    fn inset_for(padding: Padding, margin: f32) -> Inset {
        (
            padding.top + margin,
            padding.right + margin,
            padding.bottom + margin,
            padding.left + margin,
        )
    }

    /// Paint a plain (non-text-editing) box as a real DOM node, nested under
    /// `mount_point` (its parent's node, or the flat root for a floating box
    /// — see `mount_element`). Normal-flow boxes are laid out by CSS flexbox
    /// (`flow`); `floating` boxes keep `position: absolute` at their
    /// `fixed_position`, same as before this change — that's their actual
    /// designed semantic (explicitly outside flow), not a workaround.
    /// `scroll` is `Some(offset)` for a `SCROLL_X`/`SCROLL_Y` box, which
    /// delegates its children to an inner wrapper instead of hosting them
    /// directly (see `ensure_scroll_wrapper`).
    ///
    /// `hot` is `Some((hover_bg, active_bg))` for a DRAW_HOT_EFFECTS box:
    /// those colors are handed to CSS (`--mae-hover-bg`/`--mae-active-bg` +
    /// the `.mae-hot` rule installed in `new`) so the browser drives
    /// hover/press feedback on its own, with `pointer-events` enabled only
    /// for these boxes so the browser can hit-test them at all.
    ///
    /// `clickable` (a box's `MOUSE_CLICKABLE` flag) picks the emitted tag:
    /// a real `<button type="button">` instead of a `<div>`. That gets the
    /// pointer cursor, keyboard focusability, and correct semantics for
    /// free from the browser — see the static `button.mae-btn` rule
    /// installed in `new`, which resets the UA button chrome and supplies
    /// `cursor: pointer` once, rather than this file writing a per-frame
    /// `cursor` CSS field the way `os/wasm.rs::set_cursor` used to.
    ///
    /// Returns the `DomKey` this box's children should mount under.
    #[allow(clippy::too_many_arguments)]
    fn paint_div(
        &mut self,
        key: DomKey,
        mount_point: Option<DomKey>,
        ui_key: UiKey,
        rect: RectCoords,
        flow_size: FlowSize,
        bg: Option<Color>,
        border: Option<(Color, f32)>,
        corner_radius: f32,
        text: Option<String>,
        style: &UIBoxStyle,
        padding: Padding,
        hot: Option<(Color, Color)>,
        floating: bool,
        flow: FlowLayout,
        scroll: Option<Point>,
        scrolls_x: bool,
        scrolls_y: bool,
        clickable: bool,
        id: Option<&str>,
        key_id: Option<&str>,
    ) -> DomKey {
        let inset = Self::inset_for(padding, style.margin);
        if !matches!(self.nodes.get(&key), Some(e) if matches!(e.node, DomNode::Div(_))) {
            self.remove(key);
            let tag = if clickable { "button" } else { "div" };
            let el: HtmlElement = self
                .document
                .create_element(tag)
                .expect("create element")
                .dyn_into()
                .expect("element is an HtmlElement");
            if clickable {
                let _ = el.set_attribute("type", "button");
                let _ = el.class_list().add_1("mae-btn");
                self.attach_interactive_listeners(key, ui_key, &el);
            }
            // A pane that has just come into existence fades in, the way
            // native's `appear_t` does — a one-shot CSS animation, so it costs
            // no frames at all. Only on creation: a *reused* node (a menu that
            // was already on screen last frame) must not re-run it.
            if floating {
                let _ = el.class_list().add_1("mae-appear");
            }
            self.nodes.insert(
                key,
                DomEntry {
                    node: DomNode::Div(el),
                    snapshot: PaintSnapshot::blank(),
                    seen_this_frame: false,
                },
            );
        }
        let entry = self.nodes.get_mut(&key).expect("just inserted");
        entry.seen_this_frame = true;
        let DomNode::Div(el) = &entry.node else {
            unreachable!()
        };
        let el = el.clone();
        // Re-append every time, not just on creation: `walk_dom` visits
        // children in logical order every frame, but a *reused* node (same
        // DomKey matched from a previous frame — e.g. after switching tabs
        // and back) stays wherever it was originally appended otherwise,
        // silently breaking DOM order relative to freshly-created siblings.
        // `appendChild` on an already-attached node just moves it, which is
        // exactly the fix and is what every keyed-list reconciler does — but
        // see `reappend_if_needed` for why it's skipped when it'd be a no-op
        // move (would otherwise blur a focused hosted input every frame).
        let parent = self.mount_element(mount_point);
        Self::reappend_if_needed(&mut self.last_appended_in_frame, mount_point, &parent, &el);

        // The box acts as its own flex container unless it delegates to a
        // scroll wrapper instead.
        let own_flow = scroll.is_none().then_some(flow);

        let entry = self.nodes.get(&key).expect("just inserted");
        if entry.snapshot.geometry_differs(&rect, floating, flow_size) {
            if floating {
                Self::position(&el, &rect);
            } else {
                Self::apply_flow_size(&el, flow_size);
                // `apply_flow_size` always sets `position: static`; a
                // scroll-delegate box needs `relative` instead (see the
                // style block below) so its scrollbar-thumb sibling can
                // position itself against it — reasserted here too since
                // this branch can run on a frame where geometry changed but
                // style didn't (they're gated by separate diff checks).
                if scroll.is_some() {
                    let _ = el.style().set_property("position", "relative");
                }
            }
        }
        // CSS opacity inherits multiplicatively down the element tree, so each
        // node carries only its own factor — the native path's `box_opacity`
        // walk folds ancestors in explicitly instead.
        let opacity = style.opacity.clamp(0.0, 1.0);
        if entry.snapshot.style_differs(
            &bg,
            &border,
            corner_radius,
            inset,
            &hot,
            &own_flow,
            style.text_color,
            opacity,
        ) {
            Self::apply_paint_style(&el, &bg, &border, corner_radius, inset, opacity);
            let s = el.style();
            let _ = s.set_property("box-sizing", "border-box");
            let _ = s.set_property("color", &css_color(style.text_color));
            let _ = s.set_property("font-size", &format!("{}px", style.font_size));
            let _ = s.set_property("font-family", font_family_css(style.font_icon));
            let _ = s.set_property("line-height", "1.2");
            match own_flow {
                Some(f) => Self::apply_flex_container(&el, f),
                None => {
                    // Delegating to a scroll wrapper: this element sizes its
                    // (single, wrapper) child and is the *scroller* itself.
                    // The browser owns the whole gesture from here — wheel,
                    // trackpad, one-finger pan, momentum, overscroll, the
                    // scrollbar and its drag, keyboard paging — and mae only
                    // mirrors the offset it lands on (see the `scroll`
                    // listener in `attach_scroll_listener`). `touch-action`
                    // has to be restated because the container hands mae
                    // every one-finger gesture (`os/wasm.rs` sets
                    // `pinch-zoom`), which would leave this unpannable.
                    let _ = s.set_property("display", "block");
                    let _ = s.set_property("overflow-x", if scrolls_x { "auto" } else { "hidden" });
                    let _ = s.set_property("overflow-y", if scrolls_y { "auto" } else { "hidden" });
                    let _ = s.set_property("touch-action", "pan-x pan-y pinch-zoom");
                    let _ = s.set_property("overscroll-behavior", "contain");
                    let _ = s.set_property("position", "relative");
                }
            }
            match hot {
                Some((hover_bg, active_bg)) => {
                    let _ = s.set_property("pointer-events", "auto");
                    let _ = s.set_property("--mae-hover-bg", &css_color(hover_bg));
                    let _ = s.set_property("--mae-active-bg", &css_color(active_bg));
                    let _ = el.class_list().add_1("mae-hot");
                }
                None => {
                    // Clickable-but-not-hot-effect boxes (`clickable_row`/
                    // `clickable_column`) still need `pointer-events: auto`
                    // or the browser's hit-test — now the source of truth
                    // for hover/click, see `attach_interactive_listeners` —
                    // would skip them entirely. So does a scroller, for the
                    // same reason and a newer one: a wheel or a finger is
                    // routed to whatever the hit test lands on, so a
                    // `pointer-events: none` scroller is one the browser
                    // will not scroll — the input goes straight through it
                    // to whatever is behind.
                    let interactive = clickable || scroll.is_some();
                    let _ =
                        s.set_property("pointer-events", if interactive { "auto" } else { "none" });
                    let _ = el.class_list().remove_1("mae-hot");
                }
            }
        }
        if entry.snapshot.text_differs(&text) {
            el.set_text_content(text.as_deref());
        }
        if entry.snapshot.id_differs(id) {
            apply_id(&el, id);
        }
        if entry.snapshot.key_id_differs(key_id) {
            apply_key_id(&el, key_id);
        }

        let children_mount = match scroll {
            Some(offset) => {
                self.ensure_scroll_wrapper(key, &el, ui_key, flow, offset, scrolls_x, scrolls_y)
            }
            None => key,
        };

        let entry = self.nodes.get_mut(&key).expect("just inserted");
        // Preserved, not reset: a rich-text host's row/span child is painted
        // here, then separately tagged by `set_richtext_span` (called right
        // after, from `walk_dom`) — whose own diff check needs last frame's
        // value still in place to compare against, not a value this literal
        // just zeroed a moment earlier.
        let richtext_span = entry.snapshot.richtext_span;
        entry.snapshot = PaintSnapshot {
            key_id: key_id.map(str::to_string),
            opacity,
            rect,
            flow_size: Some(flow_size),
            bg,
            border,
            corner_radius,
            inset,
            hot,
            text,
            flow: own_flow,
            text_color: style.text_color,
            id: id.map(str::to_string),
            richtext_span,
        };
        children_mount
    }

    /// Paint a `LINE_EDIT`/`MULTILINE` box as a real `<input>`/`<textarea>`
    /// so the browser owns IME composition, selection, and spellcheck.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn paint_text_input(
        &mut self,
        key: DomKey,
        mount_point: Option<DomKey>,
        ui_key: UiKey,
        rect: RectCoords,
        flow_size: FlowSize,
        bg: Option<Color>,
        border: Option<(Color, f32)>,
        corner_radius: f32,
        style: &UIBoxStyle,
        value: &str,
        multiline: bool,
        padding: Padding,
        floating: bool,
        key_id: Option<&str>,
    ) {
        let inset = Self::inset_for(padding, style.margin);
        let want_multiline = multiline;
        let has_right_kind = matches!(
            self.nodes.get(&key),
            Some(e) if matches!((&e.node, want_multiline), (DomNode::TextArea(_), true) | (DomNode::Input(_), false))
        );
        if !has_right_kind {
            self.remove(key);
            let tag = if want_multiline { "textarea" } else { "input" };
            let el = self
                .document
                .create_element(tag)
                .expect("create input element");
            let el: HtmlElement = el.dyn_into().expect("element is an HtmlElement");
            if !want_multiline {
                let input: &HtmlInputElement = el.unchecked_ref();
                input.set_type("text");
            }
            // Nested ancestors set `pointer-events: none` (see `paint_div`)
            // so the browser can hit-test through non-interactive boxes to
            // whatever's actually clickable underneath, and `pointer-events`
            // is inherited — without this override a hosted input silently
            // inherits `none` from its wrapping row/column, which blocks the
            // browser's own click-to-focus (and IME/selection) even though
            // Rust's own hit-test still "sees" the click via the container
            // listener and rect math, independent of CSS.
            let _ = el.style().set_property("pointer-events", "auto");
            self.attach_input_listeners(key, ui_key, &el, want_multiline);
            let node = if want_multiline {
                DomNode::TextArea(el.unchecked_into())
            } else {
                DomNode::Input(el.unchecked_into())
            };
            self.nodes.insert(
                key,
                DomEntry {
                    node,
                    snapshot: PaintSnapshot::blank(),
                    seen_this_frame: false,
                },
            );
        }
        // See paint_div's comment: re-append every time, not just on
        // creation, so a reused node's DOM order stays correct.
        let parent = self.mount_element(mount_point);
        let entry = self.nodes.get_mut(&key).expect("just inserted");
        entry.seen_this_frame = true;
        let el = entry.node.as_html_element().clone();
        Self::reappend_if_needed(&mut self.last_appended_in_frame, mount_point, &parent, &el);

        if entry.snapshot.geometry_differs(&rect, floating, flow_size) {
            if floating {
                Self::position(&el, &rect);
            } else {
                Self::apply_flow_size(&el, flow_size);
            }
            // Only the browser knows how wide the text it is about to draw
            // will be — see the `.mae-fit` rule and `CssLen::FitText`. The
            // pixels `apply_flow_size` just wrote stay as the fallback for a
            // browser without `field-sizing`.
            let fits_text = matches!(flow_size.width, CssLen::FitText(_));
            let classes = el.class_list();
            let _ = if fits_text {
                classes.add_1("mae-fit")
            } else {
                classes.remove_1("mae-fit")
            };
        }
        let opacity = style.opacity.clamp(0.0, 1.0);
        if entry.snapshot.style_differs(
            &bg,
            &border,
            corner_radius,
            inset,
            &None,
            &None,
            style.text_color,
            opacity,
        ) {
            Self::apply_paint_style(&el, &bg, &border, corner_radius, inset, opacity);
            let s = el.style();
            let _ = s.set_property("box-sizing", "border-box");
            let _ = s.set_property("color", &css_color(style.text_color));
            Self::apply_hosted_font_size(&s, style.font_size);
            let _ = s.set_property("font-family", "'Mae Sans', sans-serif");
            let _ = s.set_property("line-height", "1.2");
            let _ = s.set_property("outline", "none");
            let _ = s.set_property("resize", "none");
        }

        // Never overwrite the element's value while the user has an IME
        // composition in progress, or we'd clobber it mid-composition.
        let composing = js_sys::Reflect::get(&el, &JsValue::from_str("__mae_composing"))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !composing && entry.snapshot.text_differs(&Some(value.to_string())) {
            match &entry.node {
                DomNode::Input(i) => i.set_value(value),
                DomNode::TextArea(t) => t.set_value(value),
                DomNode::Div(_) | DomNode::Img(_) | DomNode::RichText(_) => unreachable!(),
            }
        }
        // A hosted input's `display_string` (what `id` here is sourced from
        // — see `walk_dom`) is always exactly its current value, same as
        // `text` above — see `set_edit_display_text`.
        if entry.snapshot.id_differs(Some(value)) {
            apply_id(&el, Some(value));
        }
        if entry.snapshot.key_id_differs(key_id) {
            apply_key_id(&el, key_id);
        }
        entry.snapshot = PaintSnapshot {
            key_id: key_id.map(str::to_string),
            opacity,
            rect,
            flow_size: Some(flow_size),
            bg,
            border,
            corner_radius,
            inset,
            hot: None,
            text: Some(value.to_string()),
            flow: None,
            text_color: style.text_color,
            id: Some(value.to_string()),
            richtext_span: None,
        };
    }

    /// Paint a `RICH_TEXT_HOST` box (a `MULTILINE` box in `MarkdownMode::
    /// Rendered`) as a real `<div contenteditable="true">`, instead of the
    /// plain `<textarea>` `paint_text_input` hosts for every other text box.
    /// Unlike `paint_text_input`, this never sets the element's content
    /// itself — `walk_dom` doesn't early-return before the child walk for
    /// this box kind (see its `is_text_input` check), so the host's actual
    /// rendered content is exactly its row/span/image children
    /// (`emit_layout_line`'s existing box tree), painted the normal way
    /// `paint_div`/`paint_image` paint any other box and tagged right after
    /// with `data-raw-start`/`data-raw-end` (see `set_richtext_span`).
    /// `text` (the raw buffer, same as `paint_text_input`'s `value`) seeds
    /// the host's own `data-raw-end` (the landing target for a Range
    /// endpoint past the last line) and — mirroring `paint_text_input`'s own
    /// `id` handling — `data-mae-id`, so a CDP-driven driver can select this
    /// element the same "by current text" convention every other box uses.
    #[allow(clippy::too_many_arguments)]
    fn paint_richtext_host(
        &mut self,
        key: DomKey,
        mount_point: Option<DomKey>,
        ui_key: UiKey,
        rect: RectCoords,
        flow_size: FlowSize,
        bg: Option<Color>,
        border: Option<(Color, f32)>,
        corner_radius: f32,
        style: &UIBoxStyle,
        padding: Padding,
        floating: bool,
        text: &str,
        key_id: Option<&str>,
    ) -> DomKey {
        let total_raw_len = char_count(text);
        let inset = Self::inset_for(padding, style.margin);
        if !matches!(self.nodes.get(&key), Some(e) if matches!(e.node, DomNode::RichText(_))) {
            self.remove(key);
            let el = self
                .document
                .create_element("div")
                .expect("create richtext host element");
            let el: HtmlElement = el.dyn_into().expect("element is an HtmlElement");
            let _ = el.set_attribute("contenteditable", "true");
            // See `paint_text_input`'s identical override for why this is needed.
            let _ = el.style().set_property("pointer-events", "auto");
            // Rows are already individually wrapped/sized by `text_edit.rs`'s
            // own layout pass — `pre` stops the browser from re-wrapping (or
            // collapsing runs of spaces in) content we've already laid out.
            let _ = el.style().set_property("white-space", "pre");
            let _ = el.set_attribute("data-raw-start", "0");
            self.attach_richtext_listeners(key, ui_key, &el);
            self.nodes.insert(
                key,
                DomEntry {
                    node: DomNode::RichText(el),
                    snapshot: PaintSnapshot::blank(),
                    seen_this_frame: false,
                },
            );
        }
        // See paint_div's comment: re-append every time, not just on
        // creation, so a reused node's DOM order stays correct.
        let parent = self.mount_element(mount_point);
        let entry = self.nodes.get_mut(&key).expect("just inserted");
        entry.seen_this_frame = true;
        let el = entry.node.as_html_element().clone();
        Self::reappend_if_needed(&mut self.last_appended_in_frame, mount_point, &parent, &el);

        if entry.snapshot.geometry_differs(&rect, floating, flow_size) {
            if floating {
                Self::position(&el, &rect);
            } else {
                Self::apply_flow_size(&el, flow_size);
            }
        }
        let opacity = style.opacity.clamp(0.0, 1.0);
        if entry.snapshot.style_differs(
            &bg,
            &border,
            corner_radius,
            inset,
            &None,
            &None,
            style.text_color,
            opacity,
        ) {
            Self::apply_paint_style(&el, &bg, &border, corner_radius, inset, opacity);
            let s = el.style();
            let _ = s.set_property("box-sizing", "border-box");
            let _ = s.set_property("color", &css_color(style.text_color));
            Self::apply_hosted_font_size(&s, style.font_size);
            let _ = s.set_property("font-family", "'Mae Sans', sans-serif");
            let _ = s.set_property("line-height", "1.2");
            let _ = s.set_property("outline", "none");
        }
        if entry.snapshot.richtext_span != Some((0, total_raw_len)) {
            let _ = el.set_attribute("data-raw-end", &total_raw_len.to_string());
        }
        if entry.snapshot.id_differs(Some(text)) {
            apply_id(&el, Some(text));
        }
        // The stable `###` id, exactly as `paint_text_input` stamps it. Its
        // `data-mae-id` is the note's whole current text, so without this a
        // rendered-markdown editor could not be addressed by id at all —
        // and `IMUI::focused_id` (which resolves the focused element to the
        // nearest `data-mae-key`) reported nothing for one.
        if entry.snapshot.key_id_differs(key_id) {
            apply_key_id(&el, key_id);
        }
        entry.snapshot = PaintSnapshot {
            key_id: key_id.map(str::to_string),
            opacity,
            rect,
            flow_size: Some(flow_size),
            bg,
            border,
            corner_radius,
            inset,
            hot: None,
            text: None,
            flow: None,
            text_color: style.text_color,
            id: Some(text.to_string()),
            richtext_span: Some((0, total_raw_len)),
        };
        key
    }

    /// Paint a `DRAW_IMAGE` box as a real `<img>`, contain-fit and centered
    /// via CSS `object-fit: contain` (the same visual result as native's
    /// manual scale/center math in `paint.rs`, done by the browser instead).
    /// Uploads the image bytes to a `blob:` URL exactly once, at element
    /// creation — never per frame, and never re-encoded even across frames
    /// where the box's rect moves. `pixels` is `(width, height, mime, bytes)`
    /// from `IMUI::image_dom_bytes`: `mime` present means `bytes` are already
    /// PNG/JPEG/… and go straight into the `Blob`; `mime` absent means
    /// `bytes` are raw RGBA8 (the native decode path) and need PNG-encoding
    /// first — the browser never needs to decode our own encoder's output
    /// versus a store's own bytes any differently, this just picks whether
    /// that encode step is needed.
    #[allow(clippy::too_many_arguments)]
    fn paint_image(
        &mut self,
        key: DomKey,
        mount_point: Option<DomKey>,
        rect: RectCoords,
        flow_size: FlowSize,
        image_key: &str,
        pixels: Option<(u32, u32, Option<&'static str>, &[u8])>,
        floating: bool,
    ) {
        if !matches!(self.nodes.get(&key), Some(e) if matches!(e.node, DomNode::Img(_))) {
            self.remove(key);
            let el: HtmlImageElement = self
                .document
                .create_element("img")
                .expect("create img")
                .dyn_into()
                .expect("img is an HtmlImageElement");
            let style = el.style();
            let _ = style.set_property("object-fit", "contain");
            let _ = style.set_property("pointer-events", "none");
            // Harmless outside a contenteditable ancestor; inside a rich-text
            // host (see `paint_richtext_host`) this is what makes the image
            // an atomic unit for the browser's own caret/selection, instead
            // of something a user could click into and edit as content.
            let _ = el.set_attribute("contenteditable", "false");
            if let Some((width, height, mime, raw_bytes)) = pixels {
                let owned_png;
                let (mime, bytes): (&str, &[u8]) = match mime {
                    Some(mime) => (mime, raw_bytes),
                    None => {
                        owned_png = encode_png(width, height, raw_bytes);
                        ("image/png", owned_png.as_slice())
                    }
                };
                let parts = js_sys::Array::new();
                parts.push(&js_sys::Uint8Array::from(bytes));
                let opts = BlobPropertyBag::new();
                opts.set_type(mime);
                if let Ok(blob) = Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
                    && let Ok(url) = Url::create_object_url_with_blob(&blob)
                {
                    el.set_src(&url);
                    self.image_urls.insert(key, url);
                }
            }
            self.nodes.insert(
                key,
                DomEntry {
                    node: DomNode::Img(el),
                    snapshot: PaintSnapshot::blank(),
                    seen_this_frame: false,
                },
            );
        }
        // See paint_div's comment: re-append every time, not just on
        // creation, so a reused node's DOM order stays correct.
        let parent = self.mount_element(mount_point);
        let entry = self.nodes.get_mut(&key).expect("just inserted");
        entry.seen_this_frame = true;
        let DomNode::Img(el) = &entry.node else {
            unreachable!()
        };
        Self::reappend_if_needed(&mut self.last_appended_in_frame, mount_point, &parent, el);
        if entry.snapshot.geometry_differs(&rect, floating, flow_size) {
            let el: &HtmlElement = el.unchecked_ref();
            if floating {
                Self::position(el, &rect);
            } else {
                Self::apply_flow_size(el, flow_size);
            }
        }
        if entry.snapshot.id_differs(Some(image_key)) {
            apply_id(el, Some(image_key));
        }
        // Preserved, not reset — see `paint_div`'s identical comment: an
        // inline image inside a rich-text host is tagged separately by
        // `set_richtext_span`, right after this call returns.
        let richtext_span = entry.snapshot.richtext_span;
        entry.snapshot = PaintSnapshot {
            rect,
            flow_size: Some(flow_size),
            text: Some(image_key.to_string()),
            id: Some(image_key.to_string()),
            richtext_span,
            ..PaintSnapshot::blank()
        };
    }

    fn attach_input_listeners(
        &mut self,
        key: DomKey,
        ui_key: UiKey,
        el: &HtmlElement,
        multiline: bool,
    ) {
        let mut closures: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();
        let pending = self.pending_edits.clone();
        let waker = self.waker.clone();

        let read_back = {
            let pending = pending.clone();
            let waker = waker.clone();
            move |target: &web_sys::EventTarget| {
                let (value, cursor) = if multiline {
                    let t: &HtmlTextAreaElement = target.unchecked_ref();
                    (t.value(), t.selection_start().ok().flatten().unwrap_or(0))
                } else {
                    let i: &HtmlInputElement = target.unchecked_ref();
                    (i.value(), i.selection_start().ok().flatten().unwrap_or(0))
                };
                // `selectionStart` counts UTF-16 code units; mae counts chars.
                let cursor = char_offset_from_utf16(&value, cursor as usize);
                pending
                    .borrow_mut()
                    .entry(ui_key)
                    .or_default()
                    .push(PendingDomEdit::Replace { value, cursor });
                // The container-level OSEvent bridge deliberately never sees
                // typing in a hosted element (os/wasm.rs skips it so the
                // browser owns IME) — wake `run_dom`'s tick directly instead,
                // or this edit would sit unconsumed until something else
                // happens to trigger a rebuild.
                waker.wake();
            }
        };

        let on_input = {
            let read_back = read_back.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                if let Some(target) = e.target() {
                    read_back(&target);
                }
            })
        };
        let _ = el.add_event_listener_with_callback("input", on_input.as_ref().unchecked_ref());
        closures.push(on_input);

        let on_composition_start =
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                if let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) {
                    let _ = js_sys::Reflect::set(
                        &target,
                        &JsValue::from_str("__mae_composing"),
                        &JsValue::TRUE,
                    );
                }
            });
        let _ = el.add_event_listener_with_callback(
            "compositionstart",
            on_composition_start.as_ref().unchecked_ref(),
        );
        closures.push(on_composition_start);

        let on_composition_end = {
            let read_back = read_back.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                if let Some(target) = e.target() {
                    if let Ok(el) = target.clone().dyn_into::<Element>() {
                        let _ = js_sys::Reflect::set(
                            &el,
                            &JsValue::from_str("__mae_composing"),
                            &JsValue::FALSE,
                        );
                    }
                    read_back(&target);
                }
            })
        };
        let _ = el.add_event_listener_with_callback(
            "compositionend",
            on_composition_end.as_ref().unchecked_ref(),
        );
        closures.push(on_composition_end);

        self.attach_image_paste_listener(&mut closures, el);
        self.attach_focus_listeners(&mut closures, el, ui_key);
        if multiline {
            // Scrolling a `<textarea>` is entirely the browser's own doing —
            // it produces no `OSEvent` and asks for no rebuild — but the
            // collaborator-caret overlay is a separate element that has to
            // follow it (see `paint_remote_carets`). Only wake when there is
            // actually an overlay to move, which is the overwhelmingly less
            // common case: scrolling a note nobody else is in stays free.
            let waker = waker.clone();
            let document = self.document.clone();
            let on_scroll = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                if document
                    .query_selector(".mae-remote-carets")
                    .ok()
                    .flatten()
                    .is_some()
                {
                    waker.wake();
                }
            });
            let _ =
                el.add_event_listener_with_callback("scroll", on_scroll.as_ref().unchecked_ref());
            closures.push(on_scroll);
        }

        self.dom_listeners.insert(key, closures);
    }

    /// Drains every edit queued for `key` since it was last taken, in
    /// arrival order — see `pending_edits`'s doc comment for why this is a
    /// `Vec`, not a single value.
    pub(super) fn take_pending_edits(&self, key: UiKey) -> Vec<PendingDomEdit> {
        self.pending_edits
            .borrow_mut()
            .remove(&key)
            .unwrap_or_default()
    }

    /// Take any image pasted into a rich-text host since the last call.
    pub(super) fn take_pasted_image(&self) -> Option<Vec<u8>> {
        self.pending_pasted_image.borrow_mut().take()
    }

    /// See `pending_selection`'s doc comment. `(anchor, focus)`.
    pub(super) fn take_pending_selection(&self, key: UiKey) -> Option<(usize, usize)> {
        self.pending_selection.borrow_mut().remove(&key)
    }

    /// See `pending_focus`'s doc comment.
    pub(super) fn take_pending_focus(&self, key: UiKey) -> Option<bool> {
        self.pending_focus.borrow_mut().remove(&key)
    }

    /// Overlay a hosted multiline `<textarea>` with collaborator carets —
    /// the DOM counterpart of native's `paint.rs::draw_remote_carets`, which
    /// this backend never reached (it runs inside the GPU paint walk), so
    /// the web build showed *where* nobody was: presence badges said someone
    /// was in the note, and nothing said where.
    ///
    /// A `<textarea>` gives no way to ask where character *n* sits, so this
    /// mirrors it: an absolutely-positioned overlay with the same box, font
    /// and wrapping rules, holding the same text in a single transparent
    /// text node. The browser lays that out exactly as it laid out the
    /// textarea, and a `Range` over the mirrored text answers the position
    /// question directly — no Rust glyph math, which (as
    /// `richtext_synced_cursor` explains at length) has no reason to agree
    /// with the browser's own text layout pixel for pixel.
    ///
    /// Costs nothing when nobody else is in the note: with no carets the
    /// overlay is simply not painted, so `end_frame` prunes it. `signature`
    /// keeps it from being rebuilt on frames where nothing about it moved.
    fn paint_remote_carets(
        &mut self,
        key: DomKey,
        host: DomKey,
        text: &str,
        carets: &[RemoteCaret],
    ) {
        let Some(textarea) = self
            .nodes
            .get(&host)
            .map(|e| e.node.as_html_element().clone())
        else {
            return;
        };
        // Measured off the hosted element itself, never off mae's layout
        // rect. The textarea sits in *real* CSS flow (see `apply_flow_size`),
        // and where flexbox actually puts it is not guaranteed to match mae's
        // own solve pixel for pixel — an overlay a few pixels out puts every
        // caret on the wrong line. `clientWidth`/`clientHeight` are the
        // padding box, so a visible scrollbar narrows the mirror exactly as
        // it narrows the textarea's own text.
        let host_box = textarea.get_bounding_client_rect();
        let root_box = self.container.get_bounding_client_rect();
        let Some(computed) = web_sys::window().and_then(|w| {
            w.get_computed_style(textarea.unchecked_ref())
                .ok()
                .flatten()
        }) else {
            return;
        };
        let px = |name: &str| {
            computed
                .get_property_value(name)
                .ok()
                .and_then(|v| v.trim_end_matches("px").parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        let value = |name: &str| computed.get_property_value(name).unwrap_or_default();
        let left = host_box.left() - root_box.left() + px("border-left-width");
        let top = host_box.top() - root_box.top() + px("border-top-width");
        let (width, height) = (textarea.client_width(), textarea.client_height());
        let scroll_top = textarea.scroll_top();

        // Everything the rendered result depends on, hashed. Rebuilding
        // re-measures every caret (a forced layout each), so it must not
        // happen on frames where the answer cannot have changed — and the
        // check itself runs on every frame of a collaborative editing
        // session, so it hashes rather than formatting the whole note text
        // into a string to compare.
        let signature = {
            let mut hasher = FxHasher::default();
            for bits in [left.to_bits(), top.to_bits()] {
                hasher.write_u64(bits);
            }
            hasher.write_i32(width);
            hasher.write_i32(height);
            hasher.write_i32(scroll_top);
            for caret in carets {
                hasher.write_usize(caret.cursor);
                hasher.write_usize(caret.selection.map_or(usize::MAX, |(s, _)| s));
                hasher.write_usize(caret.selection.map_or(usize::MAX, |(_, e)| e));
                hasher.write(caret.label.as_bytes());
                for channel in [caret.color.r, caret.color.g, caret.color.b, caret.color.a] {
                    hasher.write_u32(channel.to_bits());
                }
            }
            hasher.write(text.as_bytes());
            hasher.finish()
        };

        let existing = matches!(self.nodes.get(&key), Some(e) if matches!(e.node, DomNode::Div(_)));
        if !existing {
            self.remove(key);
            let el: HtmlElement = self
                .document
                .create_element("div")
                .expect("create caret overlay")
                .dyn_into()
                .expect("element is an HtmlElement");
            // Named so a scenario can find the overlay: it has no `UIBox`
            // behind it, so it carries no `data-mae-*` id of its own.
            el.set_class_name("mae-remote-carets");
            let s = el.style();
            let _ = s.set_property("position", "absolute");
            let _ = s.set_property("pointer-events", "none");
            let _ = s.set_property("overflow", "hidden");
            let _ = s.set_property("box-sizing", "border-box");
            self.nodes.insert(
                key,
                DomEntry {
                    node: DomNode::Div(el),
                    snapshot: PaintSnapshot::blank(),
                    seen_this_frame: false,
                },
            );
        }
        // Mounted at the flat root, like a floating pane: an overlay cannot
        // be a child of a `<textarea>` at all, and being outside the editor's
        // own flow is exactly what it wants.
        let parent = self.mount_element(None);
        let entry = self.nodes.get_mut(&key).expect("just inserted");
        entry.seen_this_frame = true;
        let el = entry.node.as_html_element().clone();
        Self::reappend_if_needed(&mut self.last_appended_in_frame, None, &parent, &el);
        if self.caret_signatures.insert(key, signature) == Some(signature) {
            return;
        }

        let s = el.style();
        let _ = s.set_property("left", &format!("{left}px"));
        let _ = s.set_property("top", &format!("{top}px"));
        let _ = s.set_property("width", &format!("{width}px"));
        let _ = s.set_property("height", &format!("{height}px"));
        for prop in [
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
        ] {
            let _ = s.set_property(prop, &value(prop));
        }

        // The mirror carries the text and the markers together and takes the
        // textarea's scroll as one transform, so the markers — positioned
        // against the mirror — scroll with the text for free.
        let mirror: HtmlElement = self
            .document
            .create_element("div")
            .expect("create caret mirror")
            .dyn_into()
            .expect("element is an HtmlElement");
        let ms = mirror.style();
        let _ = ms.set_property("position", "relative");
        let _ = ms.set_property("transform", &format!("translateY({}px)", -scroll_top));
        let _ = ms.set_property("color", "transparent");
        // Copied rather than restated, so the mirror keeps laying out
        // identically to the textarea however the textarea's own styling
        // changes. `white-space`/`overflow-wrap` come from the UA stylesheet
        // for a `<textarea>` (`pre-wrap`/`break-word`) and are read the same
        // way as the rest.
        for prop in [
            "font-size",
            "font-family",
            "font-weight",
            "line-height",
            "letter-spacing",
            "word-spacing",
            "text-indent",
            "tab-size",
            "white-space",
            "overflow-wrap",
            "word-break",
        ] {
            let _ = ms.set_property(prop, &value(prop));
        }
        // A trailing newline opens no line box of its own unless something
        // follows it, which would put a caret at the document end on the
        // wrong line. A zero-width space is the standard fix and adds no
        // width.
        let text_node = self.document.create_text_node(&format!("{text}\u{200b}"));
        let _ = mirror.append_child(&text_node);
        el.set_inner_html("");
        let _ = el.append_child(&mirror);

        let mirror_box = mirror.get_bounding_client_rect();
        let (origin_x, origin_y) = (mirror_box.left(), mirror_box.top());
        let len = char_count(text);
        for caret in carets {
            let color = css_color(caret.color);
            if let Some((start, end)) = caret.selection
                && start < end
            {
                let mut tint = caret.color;
                tint.a *= 0.30;
                let tint = css_color(tint);
                // One rect per visual line the selection spans, which is what
                // `getClientRects` returns for a multi-line range.
                let rects = self
                    .range_over(&text_node, text, start.min(len), end.min(len))
                    .and_then(|r| r.get_client_rects());
                if let Some(rects) = rects {
                    for i in 0..rects.length() {
                        let Some(r) = rects.item(i) else { continue };
                        let band = self.absolute_marker(r.left() - origin_x, r.top() - origin_y);
                        let bs = band.style();
                        let _ = bs.set_property("width", &format!("{}px", r.width()));
                        let _ = bs.set_property("height", &format!("{}px", r.height()));
                        let _ = bs.set_property("background", &tint);
                        let _ = mirror.append_child(&band);
                    }
                }
            }

            let at = caret.cursor.min(len);
            let Some(spot) = self
                .range_over(&text_node, text, at, at)
                .map(|r| r.get_bounding_client_rect())
            else {
                continue;
            };
            let (x, y) = (spot.left() - origin_x, spot.top() - origin_y);
            let bar = self.absolute_marker(x, y);
            let bar_style = bar.style();
            let _ = bar_style.set_property("width", "2px");
            let _ = bar_style.set_property("height", &format!("{}px", spot.height().max(1.0)));
            let _ = bar_style.set_property("background", &color);
            let _ = mirror.append_child(&bar);

            let badge = self.absolute_marker(x, y - 13.0);
            let badge_style = badge.style();
            let _ = badge_style.set_property("height", "12px");
            let _ = badge_style.set_property("line-height", "12px");
            let _ = badge_style.set_property("padding", "0 4px");
            let _ = badge_style.set_property("border-radius", "6px");
            let _ = badge_style.set_property("font-size", "9px");
            let _ = badge_style.set_property("white-space", "nowrap");
            let _ = badge_style.set_property("background", &color);
            let _ = badge_style.set_property("color", "#fff");
            badge.set_text_content(Some(
                &caret
                    .label
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string(),
            ));
            let _ = mirror.append_child(&badge);
        }
    }

    /// A `Range` over `[start, end)` (char offsets) of `node`'s text, in the
    /// UTF-16 units the DOM counts in. `None` if the browser rejects the
    /// offsets, which it does for anything past the node's length.
    fn range_over(&self, node: &Text, text: &str, start: usize, end: usize) -> Option<Range> {
        let range = self.document.create_range().ok()?;
        range
            .set_start(node, utf16_offset(text, start))
            .ok()
            .and_then(|_| range.set_end(node, utf16_offset(text, end)).ok())?;
        Some(range)
    }

    /// An empty absolutely-positioned `<div>` at `(x, y)` within the mirror,
    /// ready for a caller to size and colour.
    fn absolute_marker(&self, x: f64, y: f64) -> HtmlElement {
        let el: HtmlElement = self
            .document
            .create_element("div")
            .expect("create caret marker")
            .dyn_into()
            .expect("element is an HtmlElement");
        let s = el.style();
        let _ = s.set_property("position", "absolute");
        let _ = s.set_property("left", &format!("{x}px"));
        let _ = s.set_property("top", &format!("{y}px"));
        el
    }

    /// Push Rust's own focus (`IMUI::focus_key`, as moved by `focus_box`)
    /// and caret (`text_edit_states`) onto a text-editing box's hosted
    /// element.
    ///
    /// `pending_focus` covers the *other* direction — the browser telling
    /// Rust that the user clicked into a field — and until this existed that
    /// was the only direction there was: `focus_box` set `focus_key`, the
    /// box drew its focus ring, and the browser's real focus never moved.
    /// Everything that follows from real focus was therefore missing on this
    /// backend: no caret, no IME, no selection, no browser-native editing
    /// keys — and, worse, whatever *was* focused kept receiving keystrokes.
    /// Enter and Space on the still-focused "New note" `<button>` are a
    /// browser-native activation, so typing a name after creating a note
    /// created more notes instead.
    ///
    /// `caret` is `(anchor, cursor)` in char offsets, `None` for a rich-text
    /// host (whose selection `sync_richtext_caret` owns) and for an
    /// unfocused box.
    fn sync_hosted_focus(
        &mut self,
        key: DomKey,
        ui_key: UiKey,
        focused: bool,
        value: &str,
        caret: Option<(usize, usize)>,
    ) {
        let Some(entry) = self.nodes.get(&key) else {
            return;
        };
        let el = entry.node.as_html_element().clone();
        let is_active = self
            .document
            .active_element()
            .is_some_and(|active| active.is_same_node(Some(el.unchecked_ref())));
        if focused {
            // Calling `focus()` on the already-focused element is not merely
            // wasteful — it fires another `focus` event, which wakes another
            // rebuild, which would call it again: a render loop that never
            // goes idle.
            if !is_active {
                let _ = el.focus();
            }
            if let Some((anchor, cursor)) = caret
                && self.synced_input_caret.get(&ui_key) != Some(&(anchor, cursor))
            {
                self.synced_input_caret.insert(ui_key, (anchor, cursor));
                // `set_selection_range` counts UTF-16 code units; every
                // offset on the Rust side is a char index.
                let start = utf16_offset(value, anchor.min(cursor));
                let end = utf16_offset(value, anchor.max(cursor));
                match &entry.node {
                    DomNode::Input(i) => {
                        let _ = i.set_selection_range(start, end);
                    }
                    DomNode::TextArea(t) => {
                        let _ = t.set_selection_range(start, end);
                    }
                    _ => {}
                }
            }
            self.driven_focus = Some(ui_key);
        } else {
            self.synced_input_caret.remove(&ui_key);
            // Only blur what this pushed focus into, and only while it is
            // still the active element: focus may legitimately have moved on
            // already (to another hosted field, or out of the page entirely),
            // and blurring then would be taking focus away from whatever now
            // holds it.
            if is_active && self.driven_focus == Some(ui_key) {
                let _ = el.blur();
            }
            if self.driven_focus == Some(ui_key) {
                self.driven_focus = None;
            }
        }
    }

    /// Attach `focus`/`blur` listeners to a text-editing box's hosted
    /// element, feeding `pending_focus` — see that field's doc comment for
    /// why this exists at all. Shared by `attach_input_listeners` (plain
    /// `<input>`/`<textarea>`) and `attach_richtext_listeners` (`<div
    /// contenteditable>`).
    fn attach_focus_listeners(
        &self,
        closures: &mut Vec<Closure<dyn FnMut(web_sys::Event)>>,
        el: &HtmlElement,
        ui_key: UiKey,
    ) {
        let pending_focus = self.pending_focus.clone();
        let waker = self.waker.clone();
        let on_focus = {
            let pending_focus = pending_focus.clone();
            let waker = waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                pending_focus.borrow_mut().insert(ui_key, true);
                waker.wake();
            })
        };
        let _ = el.add_event_listener_with_callback("focus", on_focus.as_ref().unchecked_ref());
        closures.push(on_focus);

        let on_blur = Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
            pending_focus.borrow_mut().insert(ui_key, false);
            waker.wake();
        });
        let _ = el.add_event_listener_with_callback("blur", on_blur.as_ref().unchecked_ref());
        closures.push(on_blur);
    }

    /// Attach the image-paste handler to a hosted text element (a rich-text
    /// host, or a plain `<input>`/`<textarea>`), so pasting a picture works
    /// the same in every editing mode.
    ///
    /// Handled on `paste` rather than `beforeinput` because an image
    /// carries no text to insert, so `insertFromPaste` may not fire for it
    /// at all — and because only `ClipboardEvent` exposes `clipboardData`,
    /// where the file actually lives. A paste with no image falls through
    /// untouched (no `preventDefault`), leaving the ordinary text path to
    /// handle it exactly as before.
    ///
    /// Reading the file is asynchronous, so the bytes cannot be returned to
    /// this frame: they land in `pending_pasted_image` and are picked up by
    /// `IMUI::take_pasted_image` on a later frame — the same API, and the
    /// same host-side "store a blob, insert a link" handling, that native's
    /// `os::clipboard_get_image` path already feeds.
    fn attach_image_paste_listener(
        &self,
        closures: &mut Vec<Closure<dyn FnMut(web_sys::Event)>>,
        el: &HtmlElement,
    ) {
        let on_paste = {
            let pending = self.pending_pasted_image.clone();
            let waker = self.waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                let Ok(ev) = e.clone().dyn_into::<web_sys::ClipboardEvent>() else {
                    return;
                };
                let Some(files) = ev.clipboard_data().and_then(|dt| dt.files()) else {
                    return;
                };
                let image = (0..files.length())
                    .filter_map(|i| files.get(i))
                    .find(|f| f.type_().starts_with("image/"));
                let Some(file) = image else { return };
                e.prevent_default();

                let Ok(reader) = web_sys::FileReader::new() else {
                    return;
                };
                let on_load = {
                    let reader = reader.clone();
                    let pending = pending.clone();
                    let waker = waker.clone();
                    Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                        let Ok(buffer) = reader.result() else { return };
                        let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                        if !bytes.is_empty() {
                            *pending.borrow_mut() = Some(bytes);
                            waker.wake();
                        }
                    })
                };
                reader.set_onload(Some(on_load.as_ref().unchecked_ref()));
                let _ = reader.read_as_array_buffer(&file);
                // A one-shot callback has to outlive this handler to fire at
                // all, and dropping it from inside its own invocation isn't
                // sound — so it's deliberately leaked. Bounded by the number
                // of images a user pastes in a session, at a closure each.
                on_load.forget();
            })
        };
        let _ = el.add_event_listener_with_callback("paste", on_paste.as_ref().unchecked_ref());
        closures.push(on_paste);
    }

    /// Attach `beforeinput`/composition listeners to a `RICH_TEXT_HOST`'s
    /// element. Unlike `attach_input_listeners` (whole-value replace off a
    /// plain `<input>`/`<textarea>`), this intercepts every editing
    /// `beforeinput` *before* the browser mutates anything
    /// (`prevent_default`), computes the intended raw-buffer edit from the
    /// event's own target range (resolved via `resolve_raw_offset`), and
    /// stages it as `PendingDomEdit::Range` — the browser never actually
    /// mutates this DOM at all; the next paint's rebuild (from the new raw
    /// buffer) is the only thing that ever changes it. IME composition is
    /// let through untouched (same `__mae_composing` guard as
    /// `attach_input_listeners`) and resolved once, on `compositionend`,
    /// against the range captured at `compositionstart` — by then the DOM
    /// inside the composing span has already diverged from what we painted
    /// (the browser owns it live during composition), so re-resolving
    /// against it wouldn't be meaningful.
    fn attach_richtext_listeners(&mut self, key: DomKey, ui_key: UiKey, el: &HtmlElement) {
        let mut closures: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();
        let pending = self.pending_edits.clone();
        let waker = self.waker.clone();
        let composing_range: Rc<RefCell<Option<(usize, usize)>>> = Rc::new(RefCell::new(None));

        let on_beforeinput = {
            let pending = pending.clone();
            let waker = waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                let Some(target_el) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                    return;
                };
                let composing =
                    js_sys::Reflect::get(&target_el, &JsValue::from_str("__mae_composing"))
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                let Ok(input_event) = e.clone().dyn_into::<InputEvent>() else {
                    return;
                };
                let input_type = input_event.input_type();
                if composing
                    || input_type.starts_with("insertComposition")
                    || input_event.is_composing()
                {
                    // Let the browser own its native composition UI — never
                    // prevented, never resolved here (see `compositionend`).
                    return;
                }
                e.prevent_default();

                // Undo/redo carries no range or data at all: it asks for a
                // whole prior state, which only the editor's own history
                // has. Staged as its own intent rather than resolved here.
                if input_type == "historyUndo" || input_type == "historyRedo" {
                    pending
                        .borrow_mut()
                        .entry(ui_key)
                        .or_default()
                        .push(PendingDomEdit::History {
                            redo: input_type == "historyRedo",
                        });
                    waker.wake();
                    return;
                }

                let ranges = input_event.get_target_ranges();
                let target_range_resolved =
                    ranges.get(0).dyn_into::<StaticRange>().ok().and_then(|r| {
                        let s = resolve_raw_offset(&r.start_container(), r.start_offset())?;
                        let e2 = resolve_raw_offset(&r.end_container(), r.end_offset())?;
                        Some((s.min(e2), s.max(e2)))
                    });
                // Where the caret actually is right now, as a raw offset —
                // `None` when there's a real (non-collapsed) selection.
                let live_collapsed_at = || {
                    let selection = web_sys::window()?.get_selection().ok().flatten()?;
                    if !selection.is_collapsed() {
                        return None;
                    }
                    resolve_raw_offset(&selection.anchor_node()?, selection.anchor_offset())
                };

                // For the everyday single-position edits, derive the range
                // from the *live selection* rather than `getTargetRanges()`
                // — and note this covers deletions too, whose target range
                // is a one-char (so non-collapsed) span.
                //
                // `getTargetRanges()` can be stale here, confirmed
                // empirically (real Chromium, real keystrokes, a full
                // rebuild between each): when the previous edit made this
                // line's spans *restructure* rather than just grow — e.g.
                // `*hello` (one span, no italic match yet) becoming
                // `*`/`hello`/`*` (three, once the closing marker completes
                // the match), or the reverse when a marker is deleted — the
                // next keystroke's target range can still describe the
                // pre-restructure DOM. Resolving those stale endpoints
                // against the *current* `data-raw-start` stamps then yields
                // a wrong raw range: typing appeared to reverse itself
                // (`*hello* ereht`), and backspacing through a construct
                // silently stopped deleting.
                //
                // The live selection has no such lag — it's the position
                // the browser is about to act on. `getTargetRanges()` is
                // still the only source for the multi-char cases (word/line
                // deletes, replacement of an actual selection), so those
                // keep using it.
                let single_position = matches!(
                    input_type.as_str(),
                    "insertText"
                        | "insertParagraph"
                        | "insertLineBreak"
                        | "deleteContentBackward"
                        | "deleteContentForward"
                );
                let resolved = single_position
                    .then(live_collapsed_at)
                    .flatten()
                    .map(|at| {
                        if input_type == "deleteContentBackward" {
                            (at.saturating_sub(1), at)
                        } else if input_type == "deleteContentForward" {
                            // Clamped against the real buffer length in
                            // `apply_pending_dom_edit` — this side has no
                            // access to it.
                            (at, at + 1)
                        } else {
                            (at, at)
                        }
                    })
                    .or(target_range_resolved);
                let Some((raw_start, raw_end)) = resolved else {
                    return;
                };

                // Where the inserted text comes from depends on the type:
                //
                // - `insertParagraph`/`insertLineBreak` (Enter) carry
                //   nothing at all; the newline is implied.
                // - A paste (and a drop) carries its content on
                //   `dataTransfer`, and its `data` is *null* — reading
                //   `data` for these, as this used to, made every Ctrl+V in
                //   a rendered-markdown editor delete the selection and
                //   insert nothing.
                // - Everything else (`insertText`, `insertReplacementText`,
                //   …) carries `data`, and a pure deletion carries neither,
                //   which correctly yields an empty replacement.
                //
                // Plain text only: this host's buffer *is* markdown source,
                // so pasted HTML would have to be converted to markdown to
                // mean anything here, and `text/plain` is what every editor
                // in this shape pastes by default anyway.
                let replacement = match input_type.as_str() {
                    "insertParagraph" | "insertLineBreak" => "\n".to_string(),
                    "insertFromPaste" | "insertFromDrop" | "insertFromPasteAsQuotation" => {
                        input_event
                            .data_transfer()
                            .and_then(|dt| dt.get_data("text/plain").ok())
                            .unwrap_or_default()
                    }
                    _ => input_event.data().unwrap_or_default(),
                };

                // The caret lands after whatever got inserted (empty for a
                // pure deletion, which correctly leaves it at raw_start).
                let cursor = raw_start + replacement.chars().count();
                pending
                    .borrow_mut()
                    .entry(ui_key)
                    .or_default()
                    .push(PendingDomEdit::Range {
                        raw_start,
                        raw_end,
                        replacement,
                        cursor,
                    });
                // Same reasoning as `attach_input_listeners`'s `read_back`:
                // typing in a hosted element never reaches the container-
                // level OSEvent bridge, so nothing else would wake the tick.
                waker.wake();
            })
        };
        let _ = el.add_event_listener_with_callback(
            "beforeinput",
            on_beforeinput.as_ref().unchecked_ref(),
        );
        closures.push(on_beforeinput);

        let on_composition_start = {
            let composing_range = composing_range.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                    return;
                };
                let _ = js_sys::Reflect::set(
                    &target,
                    &JsValue::from_str("__mae_composing"),
                    &JsValue::TRUE,
                );
                let Some(window) = web_sys::window() else {
                    return;
                };
                let Ok(Some(selection)) = window.get_selection() else {
                    return;
                };
                let range = (|| {
                    let anchor =
                        resolve_raw_offset(&selection.anchor_node()?, selection.anchor_offset())?;
                    let focus =
                        resolve_raw_offset(&selection.focus_node()?, selection.focus_offset())?;
                    Some((anchor.min(focus), anchor.max(focus)))
                })();
                *composing_range.borrow_mut() = range;
            })
        };
        let _ = el.add_event_listener_with_callback(
            "compositionstart",
            on_composition_start.as_ref().unchecked_ref(),
        );
        closures.push(on_composition_start);

        let on_composition_end = {
            let pending = pending.clone();
            let waker = waker.clone();
            let composing_range = composing_range.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                if let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) {
                    let _ = js_sys::Reflect::set(
                        &target,
                        &JsValue::from_str("__mae_composing"),
                        &JsValue::FALSE,
                    );
                }
                let Some((raw_start, raw_end)) = composing_range.borrow_mut().take() else {
                    return;
                };
                let Ok(comp_event) = e.clone().dyn_into::<web_sys::CompositionEvent>() else {
                    return;
                };
                let replacement = comp_event.data().unwrap_or_default();
                let cursor = raw_start + replacement.chars().count();
                pending
                    .borrow_mut()
                    .entry(ui_key)
                    .or_default()
                    .push(PendingDomEdit::Range {
                        raw_start,
                        raw_end,
                        replacement,
                        cursor,
                    });
                waker.wake();
            })
        };
        let _ = el.add_event_listener_with_callback(
            "compositionend",
            on_composition_end.as_ref().unchecked_ref(),
        );
        closures.push(on_composition_end);

        // Tracks the browser's own caret/selection (click, arrow keys, Home/
        // End, …) into `pending_selection`, consumed once per frame by
        // `IMUI::apply_pending_dom_selection` — see that field's doc comment
        // for why the caret must come from here and never from Rust's own
        // pixel-based click math. `selectionchange` fires on `document` for
        // *any* editable content's selection, not just this host's, hence
        // the `contains` check.
        let on_selection_change = {
            let el_for_check = el.clone();
            let pending_selection = self.pending_selection.clone();
            // See `richtext_synced_cursor`'s doc comment: `sync_richtext_
            // caret`'s own `set_base_and_extent` call fires this same event
            // right back at us — without recognizing and dropping that echo
            // here, it would be indistinguishable from the user actually
            // moving the caret there themselves, and get queued into
            // `pending_selection` right along with genuine ones.
            let richtext_synced_cursor = self.richtext_synced_cursor.clone();
            let waker = waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                let Some(window) = web_sys::window() else {
                    return;
                };
                let Ok(Some(selection)) = window.get_selection() else {
                    return;
                };
                let Some(focus_node) = selection.focus_node() else {
                    return;
                };
                let el_node: &Node = el_for_check.unchecked_ref();
                if !el_node.contains(Some(&focus_node)) {
                    return;
                }
                let composing =
                    js_sys::Reflect::get(&el_for_check, &JsValue::from_str("__mae_composing"))
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                if composing {
                    // The browser owns the live position natively during
                    // composition — `compositionstart`/`compositionend`
                    // already handle this case (see `composing_range`).
                    return;
                }
                let Some(focus) = resolve_raw_offset(&focus_node, selection.focus_offset()) else {
                    return;
                };
                // Both ends, not just the focus: a drag (or Shift+arrow)
                // moves only the focus while the anchor stays put, and
                // reporting the focus alone is indistinguishable from the
                // user having simply clicked there — which is exactly how
                // selections used to get thrown away on this backend.
                let anchor = selection
                    .anchor_node()
                    .filter(|n| el_node.contains(Some(n)))
                    .and_then(|n| resolve_raw_offset(&n, selection.anchor_offset()))
                    .unwrap_or(focus);
                if richtext_synced_cursor.borrow().get(&ui_key) == Some(&(anchor, focus)) {
                    return;
                }
                pending_selection
                    .borrow_mut()
                    .insert(ui_key, (anchor, focus));
                waker.wake();
            })
        };
        self.attach_image_paste_listener(&mut closures, el);

        let document = el
            .owner_document()
            .expect("host element has an owner document");
        let _ = document.add_event_listener_with_callback(
            "selectionchange",
            on_selection_change.as_ref().unchecked_ref(),
        );
        self.richtext_selection_listeners
            .insert(key, on_selection_change);

        // Caret motion this host computes itself, in *raw buffer* terms,
        // rather than leaving it to the browser's own DOM-position walk.
        // Unmodified `Home`/`End` and all four arrows; `Shift`+ (extending
        // a selection) and `Ctrl`+ (word/document motion) still fall
        // through to the browser.
        //
        // All of it needed because this host's DOM is one element *per
        // styled span* (each with its own `flex-direction: column` wrapper,
        // for pixel-parity with native's own layout — see
        // `paint_richtext_host`), which the browser's caret model does not
        // see as one flat line of text. Three confirmed real consequences,
        // all reproduced against real Chromium:
        //
        // - `ArrowUp`/`ArrowDown` could fail to move the caret at all.
        //   Vertical movement is resolved against line boxes, and a row of
        //   per-span flex items doesn't present as one; stepping
        //   `row_ranges` explicitly (carrying a sticky column) is reliable
        //   where the browser's own guess was not.
        //
        // - Every intra-row span boundary is *two* DOM caret positions for
        //   one raw offset (end of the left span / start of the right),
        //   and a native `ArrowRight` steps through both — so on a line
        //   like `*m* b` the caret visibly fails to move on roughly every
        //   other press. Resolving the current raw offset and jumping to
        //   `±1` via `dom_caret_target_for_raw` (which commits to one DOM
        //   position per raw offset) makes one press move exactly one raw
        //   char, matching native.
        // - After a real mouse click lands the caret inside one of those
        //   per-span wrappers, native `Home` can fail to reach the start
        //   of the line at all, leaving the caret roughly where the click
        //   landed (the same key works fine when the caret arrived by
        //   arrow key). So `Home`/`End` climb to the enclosing row — the
        //   outermost ancestor still carrying `data-raw-start` before the
        //   host itself, which is recognized by its own
        //   `contenteditable="true"` since every row and span carries
        //   `data-raw-start` too and naive climbing would sail past the
        //   row into the host — and collapse to that row's own start/end.
        let on_keydown = {
            let el_for_check = el.clone();
            let pending = pending.clone();
            let waker = waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                let Ok(kb) = e.clone().dyn_into::<KeyboardEvent>() else {
                    return;
                };
                let key = kb.key();

                // Undo/redo, taken from the keystroke rather than from a
                // `historyUndo` `beforeinput`. The browser only emits that
                // when *its own* undo stack has something in it, and this
                // host prevents every edit the browser would otherwise
                // make — so that stack is permanently empty and Ctrl+Z
                // would otherwise do nothing at all. (The `beforeinput`
                // branch is still handled, for the menu-driven Undo that
                // does not come through as a keystroke.)
                let primary = kb.ctrl_key() || kb.meta_key();
                if primary && !kb.alt_key() {
                    let lower = key.to_ascii_lowercase();
                    let redo = (lower == "z" && kb.shift_key()) || lower == "y";
                    if lower == "z" || lower == "y" {
                        e.prevent_default();
                        pending
                            .borrow_mut()
                            .entry(ui_key)
                            .or_default()
                            .push(PendingDomEdit::History { redo });
                        waker.wake();
                        return;
                    }
                }

                let horizontal_arrow = key == "ArrowLeft" || key == "ArrowRight";
                let vertical_arrow = key == "ArrowUp" || key == "ArrowDown";
                if key != "Home" && key != "End" && !horizontal_arrow && !vertical_arrow {
                    return;
                }
                if kb.shift_key() || kb.ctrl_key() || kb.alt_key() || kb.meta_key() {
                    return;
                }
                let Some(window) = web_sys::window() else {
                    return;
                };
                let Ok(Some(selection)) = window.get_selection() else {
                    return;
                };
                let Some(focus_node) = selection.focus_node() else {
                    return;
                };
                let el_node: &Node = el_for_check.unchecked_ref();
                if !el_node.contains(Some(&focus_node)) {
                    return;
                }

                if vertical_arrow {
                    if !selection.is_collapsed() {
                        return;
                    }
                    let Some(at) = resolve_raw_offset(&focus_node, selection.focus_offset()) else {
                        return;
                    };
                    let rows = row_ranges(&el_for_check);
                    let Some(current) = rows.iter().position(|&(s, e)| at >= s && at <= e) else {
                        return;
                    };
                    // Sticky column, like any text editor: moving through a
                    // short line and back out again returns to the column
                    // you started from, rather than being clamped to the
                    // short line's end. Kept on the element so it survives
                    // between keystrokes, and cleared by every caret move
                    // that isn't vertical (below).
                    let column =
                        js_sys::Reflect::get(&el_for_check, &JsValue::from_str("__maeDesiredCol"))
                            .ok()
                            .and_then(|v| v.as_f64())
                            .map_or(at - rows[current].0, |v| v as usize)
                            .max(at - rows[current].0);
                    let _ = js_sys::Reflect::set(
                        &el_for_check,
                        &JsValue::from_str("__maeDesiredCol"),
                        &JsValue::from_f64(column as f64),
                    );
                    // At the top/bottom row there is nowhere to go, so land
                    // on the document edge instead — the same thing every
                    // editor does, and what makes "press Up until you stop
                    // moving" a reliable way to reach the start.
                    let target = match (key.as_str(), current) {
                        ("ArrowUp", 0) => rows[0].0,
                        ("ArrowUp", i) => (rows[i - 1].0 + column).min(rows[i - 1].1),
                        ("ArrowDown", i) if i + 1 >= rows.len() => rows[rows.len() - 1].1,
                        ("ArrowDown", i) => (rows[i + 1].0 + column).min(rows[i + 1].1),
                        _ => return,
                    };
                    let Some((node, offset)) = dom_caret_target_for_raw(&el_for_check, target)
                    else {
                        return;
                    };
                    e.prevent_default();
                    let _ = selection.set_base_and_extent(&node, offset, &node, offset);
                    return;
                }
                // Any non-vertical caret move restarts the sticky column.
                let _ = js_sys::Reflect::set(
                    &el_for_check,
                    &JsValue::from_str("__maeDesiredCol"),
                    &JsValue::UNDEFINED,
                );

                if horizontal_arrow {
                    // A non-collapsed selection collapses to one of its
                    // own ends — the browser's own behaviour, and it has
                    // the anchor/focus pair to do it with; leave it be.
                    if !selection.is_collapsed() {
                        return;
                    }
                    let Some(at) = resolve_raw_offset(&focus_node, selection.focus_offset()) else {
                        return;
                    };
                    let target = if key == "ArrowLeft" {
                        at.checked_sub(1)
                    } else {
                        Some(at + 1)
                    };
                    // No target (already at offset 0), or none resolvable
                    // (already at the very end): consume the key so the
                    // browser can't step to a *different DOM position for
                    // the same raw offset* instead of properly doing
                    // nothing.
                    let Some((node, offset)) =
                        target.and_then(|t| dom_caret_target_for_raw(&el_for_check, t))
                    else {
                        e.prevent_default();
                        return;
                    };
                    e.prevent_default();
                    let _ = selection.set_base_and_extent(&node, offset, &node, offset);
                    return;
                }

                let start: Option<Element> = if let Some(text) = focus_node.dyn_ref::<Text>() {
                    text.parent_element()
                } else {
                    focus_node.dyn_ref::<Element>().cloned()
                };
                let Some(mut candidate) = start else { return };
                let mut row: Option<Element> = None;
                loop {
                    if candidate.get_attribute("contenteditable").as_deref() == Some("true") {
                        break;
                    }
                    if !candidate.has_attribute("data-raw-start") {
                        break;
                    }
                    row = Some(candidate.clone());
                    match candidate.parent_element() {
                        Some(p) => candidate = p,
                        None => break,
                    }
                }
                let Some(row) = row else { return };
                let offset = if key == "Home" {
                    0
                } else {
                    row.child_nodes().length()
                };
                e.prevent_default();
                let row_node: &Node = row.unchecked_ref();
                let _ = selection.set_base_and_extent(row_node, offset, row_node, offset);
            })
        };
        let _ = el.add_event_listener_with_callback("keydown", on_keydown.as_ref().unchecked_ref());
        closures.push(on_keydown);

        self.attach_focus_listeners(&mut closures, el, ui_key);

        self.dom_listeners.insert(key, closures);
    }

    /// Attach native hover/press/click listeners to a `MOUSE_CLICKABLE`
    /// box's own element, feeding `dom_hover`/`dom_pointer_edges` — read by
    /// `imui/input.rs`'s `#[cfg(feature = "dom")]` branch of
    /// `signal_from_key_and_flags` instead of that function's usual
    /// `point_in_rect` geometry check. `pointerenter`/`pointerleave` don't
    /// wake the render loop (mirrors `run_dom`'s existing skip-rebuild-on-
    /// hover-movement optimization — hover's *visual* feedback is already
    /// CSS-driven); `pointerdown`/`pointerup`/`click`/`contextmenu` do, same
    /// as `attach_input_listeners`'s `on_input`.
    /// Mirror a native scroller's offset back to mae, once per element.
    ///
    /// The browser is what scrolls now, so `scrollLeft`/`scrollTop` are the
    /// truth and mae's `scroll` is a copy of them — kept up to date because
    /// its own layout still positions children against it, and because a
    /// later `scroll_to_y` has to know where it is starting from. `wake`
    /// rather than `schedule_tick`: a scroll changes what is on screen (a
    /// list can lazily show different rows), so the next tick must rebuild.
    fn attach_scroll_listener(&mut self, key: DomKey, ui_key: UiKey, el: &HtmlElement) {
        if self.scroll_listeners.contains_key(&key) || ui_key.is_zero() {
            return;
        }
        let pending = self.pending_scrolls.clone();
        let waker = self.waker.clone();
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let Some(el) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                return;
            };
            pending
                .borrow_mut()
                .insert(ui_key, (el.scroll_left() as f32, el.scroll_top() as f32));
            waker.wake();
        });
        let _ = el.add_event_listener_with_callback("scroll", closure.as_ref().unchecked_ref());
        self.scroll_listeners.insert(key, closure);
    }

    /// Everything the browser has scrolled since this was last called.
    pub(super) fn take_pending_scrolls(&self) -> HashMap<UiKey, (f32, f32)> {
        std::mem::take(&mut *self.pending_scrolls.borrow_mut())
    }

    fn attach_interactive_listeners(&mut self, key: DomKey, ui_key: UiKey, el: &HtmlElement) {
        let mut closures: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();

        let on_pointer_enter = {
            let hover = self.dom_hover.clone();
            let waker = self.waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                hover.borrow_mut().insert(ui_key);
                // Hover is state the next frame reads, and `run_dom` only ticks
                // when something asks it to. Without this the highlight and its
                // tooltip wait for an unrelated event to drive a frame — so
                // they arrive late, or (on leave) linger after the pointer has
                // gone. Every other edge here already wakes for the same
                // reason.
                waker.wake();
            })
        };
        let _ = el.add_event_listener_with_callback(
            "pointerenter",
            on_pointer_enter.as_ref().unchecked_ref(),
        );
        closures.push(on_pointer_enter);

        let on_pointer_leave = {
            let hover = self.dom_hover.clone();
            let waker = self.waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                hover.borrow_mut().remove(&ui_key);
                waker.wake();
            })
        };
        let _ = el.add_event_listener_with_callback(
            "pointerleave",
            on_pointer_leave.as_ref().unchecked_ref(),
        );
        closures.push(on_pointer_leave);

        let on_pointer_down = {
            let edges = self.dom_pointer_edges.clone();
            let waker = self.waker.clone();
            let el: Element = el.clone().unchecked_into();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                let e: &PointerEvent = e.unchecked_ref();
                // Same one-pointer rule the container listeners follow (see
                // `os/wasm.rs`): a second finger belongs to a gesture the
                // browser is handling, not to a press on this element.
                if e.button() != 0 || !e.is_primary() {
                    return;
                }
                // Guarantees `pointerup` still targets this same element even
                // if the pointer drags outside its bounds before release —
                // otherwise `left_released` would never fire for a
                // press-drag-away-release gesture, leaving `active_left_key`
                // stuck. Doesn't affect `click`, which still only fires for a
                // genuine press-and-release-within-bounds gesture.
                let _ = el.set_pointer_capture(e.pointer_id());
                edges.borrow_mut().entry(ui_key).or_default().left_pressed = true;
                waker.wake();
            })
        };
        let _ = el.add_event_listener_with_callback(
            "pointerdown",
            on_pointer_down.as_ref().unchecked_ref(),
        );
        closures.push(on_pointer_down);

        let on_pointer_up = {
            let edges = self.dom_pointer_edges.clone();
            let waker = self.waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                let e: &PointerEvent = e.unchecked_ref();
                if e.button() != 0 || !e.is_primary() {
                    return;
                }
                edges.borrow_mut().entry(ui_key).or_default().left_released = true;
                waker.wake();
            })
        };
        let _ = el
            .add_event_listener_with_callback("pointerup", on_pointer_up.as_ref().unchecked_ref());
        closures.push(on_pointer_up);

        // A gesture the browser claims (a touch that becomes a pinch, say)
        // ends in `pointercancel` with no `pointerup` at all. Reporting it as
        // a release — and never as a click, which is the difference from
        // `pointerup` — is what keeps `active_left_key` from sticking on the
        // element the gesture happened to start on.
        let on_pointer_cancel = {
            let edges = self.dom_pointer_edges.clone();
            let waker = self.waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                let e: &PointerEvent = e.unchecked_ref();
                if !e.is_primary() {
                    return;
                }
                edges.borrow_mut().entry(ui_key).or_default().left_released = true;
                waker.wake();
            })
        };
        let _ = el.add_event_listener_with_callback(
            "pointercancel",
            on_pointer_cancel.as_ref().unchecked_ref(),
        );
        closures.push(on_pointer_cancel);

        let on_click = {
            let edges = self.dom_pointer_edges.clone();
            let waker = self.waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_e: web_sys::Event| {
                edges.borrow_mut().entry(ui_key).or_default().left_clicked = true;
                waker.wake();
            })
        };
        let _ = el.add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref());
        closures.push(on_click);

        let on_contextmenu = {
            let edges = self.dom_pointer_edges.clone();
            let waker = self.waker.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                // Right-click is an app action here (matching native
                // platforms' `RIGHT_CLICKED`), not browser chrome.
                e.prevent_default();
                edges.borrow_mut().entry(ui_key).or_default().right_clicked = true;
                waker.wake();
            })
        };
        let _ = el.add_event_listener_with_callback(
            "contextmenu",
            on_contextmenu.as_ref().unchecked_ref(),
        );
        closures.push(on_contextmenu);

        self.dom_listeners.insert(key, closures);
        self.interactive_keys.insert(key, ui_key);
    }

    /// Paint the host element behind everything mae draws.
    ///
    /// The browser counterpart of the native clear colour: a root faded below
    /// full opacity composites against whatever is behind it, which is the page
    /// otherwise. See `IMUI::set_theme`.
    pub(super) fn set_container_background(&self, color: Color) {
        if let Ok(el) = self.container.clone().dyn_into::<HtmlElement>() {
            let _ = el.style().set_property("background", &css_color(color));
        }
    }

    pub(super) fn dom_hovering(&self, key: UiKey) -> bool {
        self.dom_hover.borrow().contains(&key)
    }

    pub(super) fn take_dom_pointer_edge(&self, key: UiKey) -> DomPointerEdge {
        self.dom_pointer_edges
            .borrow_mut()
            .remove(&key)
            .unwrap_or_default()
    }

    /// Drain the `UiKey`s whose nodes were removed since the last call.
    fn take_removed_interactive(&mut self) -> Vec<UiKey> {
        std::mem::take(&mut self.removed_interactive)
    }
}

impl IMUI {
    /// `None` when this box isn't `MOUSE_CLICKABLE` or no DOM reconciler is
    /// active (e.g. testkit, which never calls `new_dom`) — see
    /// `input.rs`'s `signal_from_key_and_flags`, which falls back to
    /// geometry hit-testing in that case exactly as it always has.
    pub(super) fn dom_pointer_state(
        &self,
        key: UiKey,
        flags: UIBoxFlags,
    ) -> Option<DomPointerState> {
        if !flags.is_mouse_clickable() {
            return None;
        }
        let dom = self.dom.as_ref()?;
        let edge = dom.take_dom_pointer_edge(key);
        Some(DomPointerState {
            hovering: dom.dom_hovering(key),
            left_pressed: edge.left_pressed,
            left_released: edge.left_released,
            left_clicked: edge.left_clicked,
            right_clicked: edge.right_clicked,
        })
    }

    pub(super) fn draw_ui_dom(&mut self) {
        let Some(mut dom) = self.dom.take() else {
            return;
        };
        dom.begin_frame();
        // Distinct seeds so the root and overlay subtrees can never collide on
        // the same positional DomKey (see `walk_dom`). Both mount at the flat
        // root (`None`): the root box is the top of normal flow, and the
        // overlay tree holds only floating panes/tooltips, which always
        // escape to the root regardless of their logical parent anyway.
        self.walk_dom(
            &mut dom,
            self.root,
            Some(self.overlay_root),
            0x9E37_79B9_7F4A_7C15,
            None,
        );
        self.walk_dom(
            &mut dom,
            self.overlay_root,
            None,
            0xC2B2_AE3D_27D4_EB4F,
            None,
        );
        dom.end_frame();
        // A node removed mid-gesture never delivers its `pointerup`, so the
        // press it started leaves `active_left_key` pinned to a box that no
        // longer exists. That key is exclusive and — unlike `hot_key`, which
        // `begin_frame` clears every frame — persists until something releases
        // it, so it would gate hovering and clicks on every other box from then
        // on.
        for ui_key in dom.take_removed_interactive() {
            if self.active_left_key == Some(ui_key) {
                self.active_left_key = None;
            }
        }
        self.dom = Some(dom);
    }

    /// `path` is this box's DomKey — synthesized by the parent from its own
    /// path folded with this box's sibling index (see the recursive call
    /// below), so it stays stable across frames for a fixed tree shape even
    /// though `UiKey` is zero for every anonymous box. See `DomKey`'s doc
    /// comment for why `UiKey` alone can't be used here.
    ///
    /// `mount_point` is where the *parent* wants this box's DOM node
    /// appended — `None` for the flat root. Overridden to `None` below when
    /// this box is itself floating, since `FLOATING_X`/`FLOATING_Y` boxes
    /// always escape to the root regardless of what their logical parent is,
    /// same as native's separate overlay-root paint pass.
    ///
    /// Deliberately does **not** skip boxes scrolled out of view: every
    /// `visible` box gets a real DOM node unconditionally, same as any other
    /// webpage — the browser already owns deciding what's actually painted,
    /// via the real `overflow: hidden` a `CLIP`-flagged ancestor sets (see
    /// `apply_flex_container`'s `flow.clip`). An earlier version tried to
    /// mirror that decision in Rust by comparing each box's `rect` against
    /// an accumulated clip rect and skipping (removing from the DOM
    /// entirely) whatever didn't overlap — redundant with CSS at best, and
    /// at worst a second, independently-computed source of truth that could
    /// disagree with it, silently dropping content that should exist.
    /// What `apply_flow_size` should tell the element about box `idx`.
    ///
    /// Per axis: the box's *declared* `UISize` where CSS can express it and
    /// resolve it the same way mae's solve did, and Rust's solved pixels
    /// everywhere else. The exclusions are what makes this safe rather than
    /// a second, disagreeing layout engine:
    ///
    /// - `TextContent`/`ChildrenSum` always stay pixels — mae measures text
    ///   with harfrust, the browser with its own shaper, and a box sized to
    ///   its text is the one place the two genuinely disagree.
    /// - So does *any* axis whose parent scrolls along it. A scrolled axis's
    ///   flex container is the scroll wrapper, which shrink-wraps its content
    ///   (`ensure_scroll_wrapper`) — an indefinite main size, against which
    ///   `flex: 1 1 0` has no free space to claim and a percentage has
    ///   nothing to resolve against, so either would collapse the child to
    ///   nothing. mae resolves Fill against the *viewport* size there, and
    ///   pixels are how that is expressed to CSS.
    ///
    /// Everything else has a definite parent by construction — every box
    /// that is not `Fill`/`ParentPct` is emitted as pixels, and the root is
    /// the container itself — so a percentage always has something to
    /// resolve against.
    fn flow_size_for(&self, idx: usize, rect: &RectCoords) -> FlowSize {
        let Some(parent) = self.boxes[idx].parent else {
            // The root box *is* the window in mae's model, and in the DOM it
            // is a child of the container the host page sized. Filling that
            // rather than restating the pixels mae measured out of it is what
            // makes every percentage below it resolve against the real
            // window: with the root pinned, a browser-side resize left the
            // whole tree laying itself out against the old width, and the
            // content simply overflowed the smaller container.
            return FlowSize {
                width: CssLen::Pct(100.0),
                height: CssLen::Pct(100.0),
                min: (0.0, 0.0),
            };
        };
        let main_axis = self.boxes[parent].child_layout_axis;
        let parent_flags = self.boxes[parent].flags;
        let axis_len = |axis: Axis| {
            let solved = CssLen::Px(match axis {
                Axis::X => (rect.x1 - rect.x0).max(0.0),
                Axis::Y => (rect.y1 - rect.y0).max(0.0),
            });
            let parent_scrolls = match axis {
                Axis::X => parent_flags.scrolls_x(),
                Axis::Y => parent_flags.scrolls_y(),
            };
            if parent_scrolls {
                return solved;
            }
            match self.boxes[idx].pref_size[axis_idx(axis)] {
                UISize::ParentPct(pct) => CssLen::Pct(pct * 100.0),
                UISize::Fill if axis == main_axis => CssLen::Grow,
                UISize::Fill => CssLen::Stretch,
                UISize::TextContent(_) => match solved {
                    CssLen::Px(px) => CssLen::FitText(px),
                    other => other,
                },
                UISize::Pixels(_) | UISize::ChildrenSum => solved,
            }
        };
        let min = self.boxes[idx].min_size;
        FlowSize {
            width: axis_len(Axis::X),
            height: axis_len(Axis::Y),
            min: (min.width.max(0.0), min.height.max(0.0)),
        }
    }

    fn walk_dom(
        &mut self,
        dom: &mut DomReconciler,
        idx: usize,
        skip_idx: Option<usize>,
        path: DomKey,
        mount_point: Option<DomKey>,
    ) {
        if skip_idx == Some(idx) {
            return;
        }
        if !self.boxes[idx].visible {
            return;
        }
        let rect = self.boxes[idx].rect;
        let flow_size = self.flow_size_for(idx, &rect);

        let flags = self.boxes[idx].flags;
        let style = self.boxes[idx].style;
        let ui_key = self.boxes[idx].key;
        let padding = self.boxes[idx].padding;
        let is_text_input =
            flags.contains(UIBoxFlags::LINE_EDIT) || flags.contains(UIBoxFlags::MULTILINE);
        let is_rich_text_host =
            flags.contains(UIBoxFlags::MULTILINE) && flags.contains(UIBoxFlags::RICH_TEXT_HOST);
        let is_image = flags.contains(UIBoxFlags::DRAW_IMAGE);
        // Gated on the same flags the `paint_div` branch below reads, so a
        // hosted `<input>`/`<textarea>` shows exactly the chrome its box asked
        // for. These used to be passed unconditionally, which drew a border
        // around every hosted field on this backend — including the note
        // editor, whose `TextAreaOptions::border(false)` omits `DRAW_BORDER`
        // precisely so there is none (a chromeless editor cannot rely on a
        // transparent border colour, because the painted border blends to the
        // accent on focus).
        let hosted_bg = flags
            .contains(UIBoxFlags::DRAW_BACKGROUND)
            .then_some(self.boxes[idx].bg_color_animated);
        let hosted_border = flags
            .contains(UIBoxFlags::DRAW_BORDER)
            .then_some((self.boxes[idx].border_color_animated, style.border_size));
        let floating =
            flags.contains(UIBoxFlags::FLOATING_X) || flags.contains(UIBoxFlags::FLOATING_Y);
        let mount_point = if floating { None } else { mount_point };

        // A box centering its own text (buttons) centers via the same
        // flex alignment its children would use — for a leaf text box these
        // are the same concept, since the text is itself flex content.
        let (main_align, cross_align) = if style.text_align_center {
            (MainAxisAlign::Center, CrossAxisAlign::Center)
        } else {
            (
                self.boxes[idx].main_axis_align,
                self.boxes[idx].cross_axis_align,
            )
        };
        let flow = FlowLayout {
            axis: self.boxes[idx].child_layout_axis,
            main_align,
            cross_align,
            gap: self.boxes[idx].child_gap,
            clip: flags.contains(UIBoxFlags::CLIP),
        };
        // `scroll_target`, not `scroll`: the browser owns the offset, so
        // `scroll` is only mae's *mirror* of where it already is
        // (`adopt_dom_scrolls`) and pushing that back would say nothing. The
        // gap between the two is exactly a programmatic move — `scroll_to_y`
        // keeping a keyboard-selected row in view — which is what
        // `ensure_scroll_wrapper` has to hand to the element.
        let scroll =
            (flags.scrolls_x() || flags.scrolls_y()).then_some(self.boxes[idx].scroll_target);

        // A reused positional path (see `DomKey`'s doc comment: identity is
        // purely structural, so it's shared across e.g. different tabs' boxes
        // that happen to occupy the same sibling slot) can, from one frame to
        // the next, switch from labeling one *kind* of box to a fundamentally
        // different one — a plain label becoming a scrollable container, say.
        // `paint_div`'s in-place update path assumes it's still updating the
        // same kind of thing (e.g. it may skip creating a scroll wrapper that
        // was never needed before). Folding the kind into the actual DOM key
        // makes such a transition a clean remove-and-recreate instead: `path`
        // itself stays kind-independent so children's own positional keys
        // don't needlessly change too.
        let mut dom_key = if is_image {
            path.wrapping_add(0x1111_1111_1111_1111)
        } else if is_text_input {
            path.wrapping_add(0x2222_2222_2222_2222)
        } else if scroll.is_some() {
            path.wrapping_add(0x3333_3333_3333_3333)
        } else {
            path
        };
        // A plain div and a clickable one are different tags (`<div>` vs
        // `<button>`) — fold clickability in too, same reasoning as the
        // other kind salts above, so a reused positional key that changes
        // MOUSE_CLICKABLE status (e.g. across tabs) gets a clean recreate
        // instead of `paint_div` reusing a `<div>` that should now be a
        // `<button>` (or vice versa).
        if flags.contains(UIBoxFlags::MOUSE_CLICKABLE) {
            dom_key = dom_key.wrapping_add(0x4444_4444_4444_4444);
        }

        let children_mount = if is_image {
            let image_key = self.boxes[idx].display_string.clone().unwrap_or_default();
            // Mirrors `paint.rs`'s native draw path: a standalone `ui.image`
            // box (e.g. `image_viewer_panel`) is the only caller that never
            // already went through `text_edit.rs`'s own `request_image` (that
            // one's only reachable from the inline-editor image-line layout)
            // — so ask the host here, or an image that's never been
            // requested elsewhere would sit forever un-fulfilled. Checked and
            // requested before borrowing the bytes below (`request_image`
            // takes `&mut self`, which a live `pixels` borrow would block).
            if !self.has_image(&image_key) {
                self.request_image(&image_key);
            }
            let pixels = self.image_dom_bytes(&image_key);
            dom.paint_image(
                dom_key,
                mount_point,
                rect,
                flow_size,
                &image_key,
                pixels,
                floating,
            );
            dom_key
        } else if is_rich_text_host {
            // A `RICH_TEXT_HOST` MULTILINE box's full raw text (see the
            // `is_text_input` branch below for why it's `string`, not
            // `display_string`) — never displayed directly (the host's real
            // content is its row/span/image children, painted by the child
            // walk below), only used for `data-raw-end`/`data-mae-id`.
            let value = self.boxes[idx].string.clone().unwrap_or_default();
            let host = dom.paint_richtext_host(
                dom_key,
                mount_point,
                ui_key,
                rect,
                flow_size,
                hosted_bg,
                hosted_border,
                style.corner_radius,
                &style,
                padding,
                floating,
                &value,
                self.boxes[idx].key_id.as_deref(),
            );
            // No caret here: a rich-text host's selection is placed against
            // its painted children by `sync_richtext_caret`, after the child
            // walk below has logged them.
            dom.sync_hosted_focus(
                dom_key,
                ui_key,
                self.focus_key == Some(ui_key),
                &value,
                None,
            );
            host
        } else if is_text_input {
            // LINE_EDIT's full value (incl. masking/IME preedit) lives in
            // `display_string` (see `set_edit_display_text`), but MULTILINE
            // never populates that on the parent box — only per-line child
            // boxes get their own slice for native's line-by-line glyph
            // rendering. The parent's full text (also incl. IME preedit) is
            // in `string` instead (`text_edit.rs`'s `textarea_impl`).
            let value = if flags.contains(UIBoxFlags::MULTILINE) {
                self.boxes[idx].string.clone().unwrap_or_default()
            } else {
                self.boxes[idx].display_string.clone().unwrap_or_default()
            };
            dom.paint_text_input(
                dom_key,
                mount_point,
                ui_key,
                rect,
                flow_size,
                hosted_bg,
                hosted_border,
                style.corner_radius,
                &style,
                &value,
                flags.contains(UIBoxFlags::MULTILINE),
                padding,
                floating,
                // A hosted input's `data-mae-id` is its *value*, which changes
                // as the user types — so without the stable id a driver cannot
                // address a text field at all on this backend.
                self.boxes[idx].key_id.as_deref(),
            );
            let focused = self.focus_key == Some(ui_key);
            // An active selection restores both of its ends; a plain caret is
            // an anchor and a cursor at the same place (same shape
            // `sync_richtext_caret` is handed below).
            let caret = focused
                .then(|| self.text_edit_states.get(&ui_key))
                .flatten()
                .map(|s| (s.selection.map_or(s.cursor, |sel| sel.anchor), s.cursor));
            dom.sync_hosted_focus(dom_key, ui_key, focused, &value, caret);
            // Collaborator carets, over the textarea they belong to. Nothing
            // is painted (and the overlay from a previous frame is pruned)
            // when no one else is in this note. Offset far outside any child
            // index, like the scrollbar-thumb markers below.
            if let Some(carets) = self.remote_carets.get(&ui_key)
                && !carets.is_empty()
            {
                dom.paint_remote_carets(
                    dom_key.wrapping_add(0xFFFF_FFFF_FFFF_FFF3),
                    dom_key,
                    &value,
                    carets,
                );
            }
            dom_key
        } else {
            let bg = flags
                .contains(UIBoxFlags::DRAW_BACKGROUND)
                .then_some(self.boxes[idx].bg_color_animated);
            let border = flags
                .contains(UIBoxFlags::DRAW_BORDER)
                .then_some((self.boxes[idx].border_color_animated, style.border_size));
            let text = flags
                .contains(UIBoxFlags::DRAW_TEXT)
                .then(|| self.boxes[idx].display_string.clone())
                .flatten();
            // The fully-hot/fully-active target colors `animate_visual_state`
            // (paint.rs) would blend toward — computed once here from the base
            // style color so CSS can own the actual hover/press transition
            // instead of Rust re-lerping and rewriting it every frame.
            let hot = flags.contains(UIBoxFlags::DRAW_HOT_EFFECTS).then(|| {
                let hover_bg = color_mix(style.bg_color, self.theme.surface_hover, 0.55);
                let active_bg = color_mix(hover_bg, self.theme.accent_active, 0.35);
                (hover_bg, active_bg)
            });
            dom.paint_div(
                dom_key,
                mount_point,
                ui_key,
                rect,
                flow_size,
                bg,
                border,
                style.corner_radius,
                text,
                &style,
                padding,
                hot,
                floating,
                flow,
                scroll,
                flags.scrolls_x(),
                flags.scrolls_y(),
                flags.contains(UIBoxFlags::MOUSE_CLICKABLE),
                // Not `debug_label`: that field is only ever refreshed for a
                // *named* box (one with an explicit `##id`, via
                // `reset_for_frame`) — a plain `ui.label(...)`/`icon_label`
                // allocates anonymously (`alloc_box(None, ...)`) and sets its
                // text through a separate `set_display_string` call that
                // updates `display_string`/`string` but not `debug_label`,
                // leaving it permanently `None`. `display_string` is kept in
                // sync by both paths (and is exactly what testkit's own
                // `UiNodeSnapshot::text` reads), so it's the one reliable
                // source here regardless of how the box was created.
                self.boxes[idx].display_string.as_deref(),
                // Stamped alongside it as `data-mae-key`, never instead of it:
                // existing drivers select by display text, and the stable id is
                // an additional handle that survives a label change.
                self.boxes[idx].key_id.as_deref(),
            )
        };

        // A rich-text host's row/span/image child (see `UIBox::richtext_
        // span`, set by `emit_layout_line`) gets its raw-offset attributes
        // stamped right after its own element is created/updated above,
        // regardless of which branch (image or plain div) produced it. A
        // *row* carries the same attributes as its own leaf child (a span,
        // an image, or an empty line's spacer) purely as `resolve_raw_
        // offset`'s fallback for a Range landing on the row container
        // itself — logging it too, alongside that leaf, would double-log
        // the identical raw range and make `sync_richtext_caret`'s tie-break
        // land on the row (whose `Atomic` "landed after" math assumes *it*
        // is the leaf, not its wrapper) instead of the leaf itself. Only
        // leaves (no children of their own) go in the log.
        if let Some(span) = self.boxes[idx].richtext_span {
            dom.set_richtext_span(children_mount, span);
            if self.boxes[idx].children.is_empty()
                && let Some(log) = dom.richtext_log.as_mut()
            {
                let kind = if flags.contains(UIBoxFlags::DRAW_TEXT) {
                    RichTextAnchorKind::Text
                } else {
                    RichTextAnchorKind::Atomic
                };
                log.push((kind, span.0, span.1, children_mount));
            }
        }

        // No scrollbar is painted here at all any more. A scrollable box is
        // a real scroller on this backend (`paint_div` gives it
        // `overflow: auto`), so the browser draws its scrollbar, in the
        // platform's own style, and drags it — the same way it always did for
        // a hosted `<input>`/`<textarea>`. Rust's `scrollbar_thumb_rect` was
        // also becoming a poor description of where a thumb belonged: it
        // returns an absolute layout-space rect, which real CSS flow
        // (flexbox, gaps, ancestor padding) no longer guarantees matches the
        // box's actual on-screen position pixel for pixel.

        if is_text_input && !is_rich_text_host {
            // A LINE_EDIT/MULTILINE box's children are per-visual-line boxes
            // `text_edit.rs` builds purely for native glyph positioning and
            // caret/block-decoration math (see `draw_textarea_caret` et al in
            // `paint.rs`) — not user content. The hosted `<input>`/`<textarea>`
            // above already renders the full text itself; painting these too
            // would double it up as redundant overlaid divs. A `RICH_TEXT_
            // HOST` is the one exception (handled below): its children *are*
            // the content — there's no `.value` for them to double up with.
            return;
        }

        // A rich-text host's row/span/image children (walked normally,
        // below, just like this) are exactly what `richtext_log` needs to
        // place the caret afterward — see `set_richtext_span`'s stamping
        // above and `sync_richtext_caret` below. `take()`/restore instead of
        // a bare push/pop: a rich-text host never nests inside another one
        // in practice, but this keeps that assumption from being silently
        // required.
        let outer_richtext_log = is_rich_text_host
            .then(|| dom.richtext_log.replace(Vec::new()))
            .flatten();

        let child_len = self.boxes[idx].children.len();
        for i in 0..child_len {
            let child = self.boxes[idx].children[i];
            // Fold this box's own path with the child's sibling index (and its
            // `UiKey` when it has one, so an explicitly-`##id`'d box keeps a
            // stable identity even if siblings around it come and go).
            let child_key = self.boxes[child].key;
            let child_path = path
                .wrapping_mul(0x100_0000_01B3)
                .wrapping_add(i as u64)
                .wrapping_add(child_key.0.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            self.walk_dom(dom, child, skip_idx, child_path, Some(children_mount));
        }

        if is_rich_text_host {
            let log = dom.richtext_log.take().unwrap_or_default();
            if self.focus_key == Some(ui_key)
                && let Some((anchor, cursor)) = self.text_edit_states.get(&ui_key).map(|s| {
                    // An active selection restores both of its ends; a plain
                    // caret is just an anchor and focus at the same place.
                    (s.selection.map_or(s.cursor, |sel| sel.anchor), s.cursor)
                })
            {
                dom.sync_richtext_caret(ui_key, &log, anchor, cursor);
            }
            dom.richtext_log = outer_richtext_log;
        }
    }

    /// If a hosted `<input>`/`<textarea>`/`<div contenteditable>` (see
    /// `paint_dom.rs`) reported new edits since the last frame, apply *all*
    /// of them, in order, to `buffer` before the normal key-event-driven
    /// edit logic runs. No-op outside the DOM backend, and a no-op for boxes
    /// with no pending edits.
    ///
    /// Applying only the last one (this used to take a single `Option`, not
    /// a queue — see `DomReconciler::pending_edits`'s doc comment) silently
    /// dropped real keystrokes under fast typing: each `Range` edit's raw
    /// offsets are only valid against the buffer state at the moment its
    /// `beforeinput` fired, so a later edit in the same queued batch needs
    /// its offsets shifted by the net length change every earlier edit in
    /// *this same batch* already applied — `shift` tracks exactly that.
    #[cfg(feature = "dom")]
    pub(super) fn apply_pending_dom_edit<T: TextEditBuffer>(&mut self, key: UiKey, buffer: &mut T) {
        let Some(dom) = self.dom.as_ref() else { return };
        let pending = dom.take_pending_edits(key);
        if pending.is_empty() {
            return;
        }
        let mut new_len = char_count(&buffer.text());
        let mut cursor = self.text_edit_states.get(&key).map_or(0, |s| s.cursor);
        let mut shift: isize = 0;
        for edit in pending {
            match edit {
                PendingDomEdit::History { redo } => {
                    // The editor's own history is the only one that exists
                    // for a rich-text host, and it restores its own caret —
                    // so `cursor` is re-read from the state afterwards
                    // rather than carried over from before.
                    let mut text = buffer.text();
                    if redo {
                        self.redo_text_edit(key, buffer, &mut text);
                    } else {
                        self.undo_text_edit(key, buffer, &mut text);
                    }
                    new_len = char_count(&buffer.text());
                    cursor = self.text_edit_states.get(&key).map_or(0, |s| s.cursor);
                    shift = 0;
                }
                PendingDomEdit::Replace { value, cursor: c } => {
                    let old_len = char_count(&buffer.text());
                    self.record_undo_before_edit(key, EditKind::Boundary, &buffer.text(), false);
                    buffer.delete_range((0, old_len));
                    buffer.insert_text(0, &value);
                    new_len = char_count(&value);
                    cursor = c;
                    shift = 0;
                }
                PendingDomEdit::Range {
                    raw_start,
                    raw_end,
                    replacement,
                    cursor: c,
                } => {
                    // Clamp to the buffer's actual length: `resolve_raw_
                    // offset` computes these from `data-raw-start`/`data-
                    // raw-end` attributes that are accurate against the DOM
                    // as of this edit's own `beforeinput` (adjusted by
                    // `shift` for any earlier edit in this same batch), but
                    // a collapsed-delete's manual fallback extension (see
                    // `attach_richtext_listeners`) has no access to the real
                    // buffer length to clamp against there.
                    let len = char_count(&buffer.text()) as isize;
                    let start = (raw_start as isize + shift).clamp(0, len) as usize;
                    let end = (raw_end as isize + shift).clamp(0, len) as usize;
                    let (start, end) = (start.min(end), start.max(end));
                    // Same granularity native gets from its own key
                    // handling, derived from the edit's shape instead: a
                    // typing run and a backspace run each coalesce into one
                    // undo step, while whitespace closes the current word
                    // and anything larger (a paste, a newline, replacing a
                    // selection) is a step of its own. Nothing recorded
                    // undo state on this path at all before, so a
                    // rendered-markdown editor had an empty history and
                    // Ctrl+Z could not have done anything even once it was
                    // routed here.
                    let mut chars = replacement.chars();
                    let kind = match (chars.next(), chars.next()) {
                        (None, _) => EditKind::Delete,
                        (Some(c), None) if c.is_whitespace() && c != '\n' => EditKind::InsertBreak,
                        (Some(c), None) if c != '\n' => EditKind::Insert,
                        _ => EditKind::Boundary,
                    };
                    let replaces_selection = end > start && !replacement.is_empty();
                    self.record_undo_before_edit(key, kind, &buffer.text(), replaces_selection);
                    buffer.delete_range((start, end));
                    buffer.insert_text(start, &replacement);
                    cursor = (c as isize + shift).max(0) as usize;
                    shift += replacement.chars().count() as isize - (end - start) as isize;
                    new_len = char_count(&buffer.text());
                }
            }
        }
        let state = self.text_edit_states.entry(key).or_default();
        state.cursor = cursor.min(new_len);
        // An edit replaces whatever was selected, so nothing is selected once
        // it lands. Dropping the selection here is what `apply_pending_dom_
        // selection` already does for a rich-text host, and a plain field
        // needs it just as much: its `input` read-back reports only a caret,
        // never an anchor, so a *programmatic* selection (`set_textarea_
        // cursor` — "select the placeholder name so typing replaces it")
        // would otherwise survive every keystroke. `sync_hosted_focus` then
        // sees `(0, cursor)` change as the text grows and re-pushes it as a
        // live selection, which the next character replaces: typing
        // "Groceries" over a selected placeholder landed as "es".
        state.selection = None;
    }

    /// If a `RICH_TEXT_HOST`'s hosted `<div contenteditable>` reported a new
    /// caret position since the last frame (a plain click, arrow key, Home/
    /// End, … — see `DomReconciler::pending_selection`'s doc comment for why
    /// this exists instead of the usual pixel-based click-to-cursor path),
    /// apply it before that path runs. No-op outside the DOM backend, and a
    /// no-op for a host with no pending selection change.
    #[cfg(feature = "dom")]
    pub(super) fn apply_pending_dom_selection(&mut self, key: UiKey, text: &str) {
        let Some(dom) = self.dom.as_ref() else { return };
        let Some((anchor, cursor)) = dom.take_pending_selection(key) else {
            return;
        };
        let len = char_count(text);
        let (anchor, cursor) = (anchor.min(len), cursor.min(len));
        let state = self.text_edit_states.entry(key).or_default();
        state.cursor = cursor;
        // A collapsed browser selection means no selection at all here —
        // not a zero-width `TextSelection`, which `selection_range()` would
        // report as `None` anyway but which would leave stale state around.
        state.selection = (anchor != cursor).then_some(TextSelection { anchor, cursor });
    }

    /// If a text-editing box's hosted DOM element reported a native `focus`/
    /// `blur` since the last frame, apply it to `self.focus_key` — see
    /// `DomReconciler::pending_focus`'s doc comment for why this exists
    /// (`click_to_focus`'s usual path, `apply_click_to_focus`, never fires
    /// for one of these on this backend at all). No-op outside the DOM
    /// backend, and a no-op for a box with no pending focus change.
    #[cfg(feature = "dom")]
    pub(super) fn apply_pending_dom_focus(&mut self, key: UiKey) {
        let Some(dom) = self.dom.as_ref() else { return };
        let Some(gained) = dom.take_pending_focus(key) else {
            return;
        };
        if gained {
            self.focus_key = Some(key);
        } else if self.focus_key == Some(key) {
            self.focus_key = None;
        }
    }
}
