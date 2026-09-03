use std::{collections::HashMap, ops::Range};

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

use crate::{
    draw::Drawer,
    os::{self, OSCursor, OSEvent, OSEventFlag, OSEventType, OSKey, OSKeyCode},
    render::{self, RectCoords, V4f32},
};

mod input;
mod layout;
mod lifecycle;
mod paint;
#[cfg(feature = "dom")]
mod paint_dom;
mod scroll;
#[cfg(test)]
mod tests;
mod text_edit;
mod toast;
mod widgets;

pub use toast::ToastLevel;

pub mod uibox {
    pub use super::{
        Color, Padding, ThemeKind, UIBox, UIBoxFlags as UIBoxFlag, UIBoxHandle, UIBoxParams,
        UIBoxStyle, UISignal as UIBoxSignal, UITheme, u64_hash_from_string,
    };
}

pub type Color = V4f32;

impl Color {
    pub fn transparent() -> Self {
        Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }
    }

    pub fn new(text: &str) -> Self {
        parse_hex_color(text).unwrap_or(Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        })
    }
}

fn color_lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

fn color_mix(a: Color, b: Color, t: f32) -> Color {
    color_lerp(a, b, t)
}

fn color_mul_alpha(mut color: Color, alpha: f32) -> Color {
    color.a *= alpha.clamp(0.0, 1.0);
    color
}

pub fn color_rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

fn parse_hex_color(text: &str) -> Option<Color> {
    let bytes = text.as_bytes();
    if bytes.first().copied()? != b'#' {
        return None;
    }
    match bytes.len() {
        4 => Some(Color {
            r: hex1(bytes[1])? as f32 / 15.0,
            g: hex1(bytes[2])? as f32 / 15.0,
            b: hex1(bytes[3])? as f32 / 15.0,
            a: 1.0,
        }),
        7 | 9 => {
            let a = if bytes.len() == 9 {
                hex2(bytes[7], bytes[8])? as f32 / 255.0
            } else {
                1.0
            };
            Some(Color {
                r: hex2(bytes[1], bytes[2])? as f32 / 255.0,
                g: hex2(bytes[3], bytes[4])? as f32 / 255.0,
                b: hex2(bytes[5], bytes[6])? as f32 / 255.0,
                a,
            })
        }
        _ => None,
    }
}

fn hex1(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex2(a: u8, b: u8) -> Option<u8> {
    Some((hex1(a)? << 4) | hex1(b)?)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    x: f32,
    y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn x(&self) -> f32 {
        self.x
    }

    pub fn y(&self) -> f32 {
        self.y
    }

    fn axis(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
        }
    }

    fn set_axis(&mut self, axis: Axis, value: f32) {
        match axis {
            Axis::X => self.x = value,
            Axis::Y => self.y = value,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub fn from(value: (f32, f32)) -> Self {
        Self {
            width: value.0,
            height: value.1,
        }
    }

    fn axis(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.width,
            Axis::Y => self.height,
        }
    }

    fn set_axis(&mut self, axis: Axis, value: f32) {
        match axis {
            Axis::X => self.width = value,
            Axis::Y => self.height = value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

const SCROLLBAR_THICKNESS: f32 = 3.0;
const SCROLLBAR_HOVER_THICKNESS: f32 = 8.0;
const SCROLLBAR_EDGE_INSET: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseButton {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarDrag {
    key: UiKey,
    axis: Axis,
    thumb_grab_offset: f32,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarHitArea {
    key: UiKey,
    rect: RectCoords,
}

#[derive(Clone, Copy, Debug, Default)]
struct TextClickStreak {
    key: UiKey,
    pos: Point,
    time: f64,
    count: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UISize {
    Pixels(f32),
    TextContent(f32),
    ParentPct(f32),
    ChildrenSum,
    Fill,
}

impl UISize {
    pub fn px(value: f32) -> Self {
        Self::Pixels(value)
    }

    pub fn text(padding: f32) -> Self {
        Self::TextContent(padding)
    }

    pub fn pct(value: f32) -> Self {
        Self::ParentPct(value)
    }

    pub fn children_sum() -> Self {
        Self::ChildrenSum
    }

    pub fn fill() -> Self {
        Self::Fill
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum MainAxisAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }

    fn axis(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.horizontal(),
            Axis::Y => self.vertical(),
        }
    }

    fn min_axis(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.left,
            Axis::Y => self.top,
        }
    }

    fn max_axis(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.right,
            Axis::Y => self.bottom,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct UiKey(pub u64);

impl UiKey {
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UIBoxHandle {
    idx: usize,
    key: UiKey,
    signal: UISignal,
}

impl UIBoxHandle {
    pub fn key(&self) -> UiKey {
        self.key
    }

    pub fn signal(&self) -> UISignal {
        self.signal
    }

    pub fn pressed(&self) -> bool {
        self.signal.pressed()
    }

    pub fn released(&self) -> bool {
        self.signal.released()
    }

    pub fn clicked(&self) -> bool {
        self.signal.clicked()
    }

    pub fn right_clicked(&self) -> bool {
        self.signal.right_clicked()
    }

    pub fn dragging(&self) -> bool {
        self.signal.dragging()
    }

    pub fn hover(&self) -> bool {
        self.signal.hovering()
    }

    pub fn idx(&self) -> usize {
        self.idx
    }

    pub fn width(self, ui: &mut IMUI, width: UISize) -> Self {
        ui.width(self, width);
        self
    }

    pub fn height(self, ui: &mut IMUI, height: UISize) -> Self {
        ui.height(self, height);
        self
    }

    pub fn min_width(self, ui: &mut IMUI, width: f32) -> Self {
        ui.min_width(self, width);
        self
    }

    pub fn min_height(self, ui: &mut IMUI, height: f32) -> Self {
        ui.min_height(self, height);
        self
    }

    pub fn background(self, ui: &mut IMUI, color: Color) -> Self {
        ui.background(self, color);
        self
    }

    pub fn text_color(self, ui: &mut IMUI, color: Color) -> Self {
        ui.text_color(self, color);
        self
    }

    pub fn font_size(self, ui: &mut IMUI, size: f32) -> Self {
        ui.font_size(self, size);
        self
    }

    pub fn border_color(self, ui: &mut IMUI, color: Color) -> Self {
        ui.border_color(self, color);
        self
    }

    pub fn padding_all(self, ui: &mut IMUI, value: f32) -> Self {
        ui.padding_all(self, value);
        self
    }

    /// Per-side padding, in CSS order (top, right, bottom, left).
    pub fn padding(self, ui: &mut IMUI, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        ui.padding(self, top, right, bottom, left);
        self
    }

    pub fn gap(self, ui: &mut IMUI, value: f32) -> Self {
        ui.gap(self, value);
        self
    }

    /// Scale this box's painted alpha (and every descendant's) by `opacity`,
    /// clamped to `[0, 1]`. Compose an app-driven fade with it — hold the
    /// animated value in app state and step it with [`IMUI::dt`] +
    /// [`animate_scalar`]. Floating panes additionally fade themselves in on
    /// appearance; the two multiply.
    pub fn opacity(self, ui: &mut IMUI, opacity: f32) -> Self {
        ui.opacity(self, opacity);
        self
    }

    pub fn corner_radius(self, ui: &mut IMUI, radius: f32) -> Self {
        ui.corner_radius(self, radius);
        self
    }

    /// Override the box's text/content margin (default `2.0`). Set to `0.0` for
    /// inline text segments that must butt up against each other seamlessly.
    pub fn margin(self, ui: &mut IMUI, margin: f32) -> Self {
        ui.margin(self, margin);
        self
    }

    /// Paint a highlight rectangle (in `color`) behind each given byte range of
    /// this box's text — e.g. to highlight search matches inside one continuous
    /// label without splitting it into separately-clipped segments.
    pub fn text_highlights(self, ui: &mut IMUI, ranges: Vec<(usize, usize)>, color: Color) -> Self {
        ui.text_highlights(self, ranges, color);
        self
    }

    pub fn cursor(self, ui: &mut IMUI, cursor: OSCursor) -> Self {
        ui.cursor(self, cursor);
        self
    }

    pub fn hit_padding_x(self, ui: &mut IMUI, value: f32) -> Self {
        ui.hit_padding_x(self, value);
        self
    }

    pub fn scroll_y(self, ui: &mut IMUI, enabled: bool) -> Self {
        ui.scroll_y(self, enabled);
        self
    }

    pub fn scroll_x(self, ui: &mut IMUI, enabled: bool) -> Self {
        ui.scroll_x(self, enabled);
        self
    }

    pub fn clip(self, ui: &mut IMUI, enabled: bool) -> Self {
        ui.clip(self, enabled);
        self
    }

    pub fn align(self, ui: &mut IMUI, main: MainAxisAlign, cross: CrossAxisAlign) -> Self {
        ui.align(self, main, cross);
        self
    }

    /// Center this box's own text within its content area (see
    /// [`UIBoxStyle::text_align_center`]).
    pub fn text_center(self, ui: &mut IMUI, center: bool) -> Self {
        ui.text_center(self, center);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct UISignal {
    pub flags: u32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub left_press_pos: Option<Point>,
}

impl UISignal {
    pub const LEFT_PRESSED: u32 = 1 << 0;
    pub const LEFT_DRAGGING: u32 = 1 << 1;
    pub const LEFT_RELEASED: u32 = 1 << 2;
    pub const LEFT_CLICKED: u32 = 1 << 3;
    pub const HOVERING: u32 = 1 << 4;
    pub const MOUSE_OVER: u32 = 1 << 5;
    pub const COMMIT: u32 = 1 << 6;
    pub const RIGHT_CLICKED: u32 = 1 << 7;

    pub fn pressed(self) -> bool {
        self.flags & Self::LEFT_PRESSED != 0
    }

    pub fn released(self) -> bool {
        self.flags & Self::LEFT_RELEASED != 0
    }

    pub fn clicked(self) -> bool {
        self.flags & Self::LEFT_CLICKED != 0
    }

    pub fn right_clicked(self) -> bool {
        self.flags & Self::RIGHT_CLICKED != 0
    }

    pub fn dragging(self) -> bool {
        self.flags & Self::LEFT_DRAGGING != 0
    }

    pub fn hovering(self) -> bool {
        self.flags & Self::HOVERING != 0
    }

    pub fn mouse_over(self) -> bool {
        self.flags & Self::MOUSE_OVER != 0
    }

    pub fn committed(self) -> bool {
        self.flags & Self::COMMIT != 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: usize,
    pub cursor: usize,
}

impl TextSelection {
    pub fn normalized(self) -> Option<(usize, usize)> {
        if self.anchor == self.cursor {
            None
        } else if self.anchor < self.cursor {
            Some((self.anchor, self.cursor))
        } else {
            Some((self.cursor, self.anchor))
        }
    }
}

/// A point-in-time text-edit state that undo/redo can restore to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UndoSnapshot {
    pub text: String,
    pub cursor: usize,
}

/// What kind of edit produced an undo entry. Consecutive same-kind edits with no
/// intervening cursor move or selection are coalesced into a single undo step, so a
/// typed word (or a backspace run) undoes in one go rather than character by character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    /// A single typed character — coalesces with an adjacent typing run.
    Insert,
    /// A typed whitespace character — joins the current word run but closes it, so the
    /// next word becomes a separate undo step.
    InsertBreak,
    /// A single-character backspace/forward-delete — coalesces with an adjacent run.
    Delete,
    /// Paste, cut, newline, etc. — always its own undo step and breaks any run.
    Boundary,
}

/// Bounded undo/redo history for one textarea. Entries hold the state *before* an edit
/// (undo) and *before* an undo (redo).
#[derive(Clone, Debug, Default)]
pub struct UndoHistory {
    undo: Vec<UndoSnapshot>,
    redo: Vec<UndoSnapshot>,
    /// The kind of the in-progress coalescing run, if the last change can still absorb
    /// the next one. Cleared by cursor moves and undo/redo to break the run.
    coalescing: Option<EditKind>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEditState {
    pub cursor: usize,
    pub selection: Option<TextSelection>,
    pub desired_column: Option<usize>,
    pub last_interaction_time: f64,
    /// Cursor position the last time caret-follow scrolling ran. Used to only follow
    /// the caret when it actually moves, so manual scrolling away from the caret sticks.
    pub scroll_follow_cursor: Option<usize>,
}

impl TextEditState {
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        self.selection.and_then(TextSelection::normalized)
    }

    fn clear_selection(&mut self) {
        self.selection = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UIBoxFlags(u64);

impl UIBoxFlags {
    // Interaction capability flags. These are still box flags by design:
    // widget helpers compose behavior by applying capabilities to retained boxes.
    pub const NONE: Self = Self(0);
    pub const MOUSE_CLICKABLE: Self = Self(1 << 0);
    pub const KEYBOARD_CLICKABLE: Self = Self(1 << 1);
    pub const CLICK_TO_FOCUS: Self = Self(1 << 2);
    pub const SCROLL_X: Self = Self(1 << 3);
    pub const SCROLL_Y: Self = Self(1 << 4);

    // Layout flags.
    pub const FLOATING_X: Self = Self(1 << 5);
    pub const FLOATING_Y: Self = Self(1 << 6);
    pub const FIXED_WIDTH: Self = Self(1 << 7);
    pub const FIXED_HEIGHT: Self = Self(1 << 8);

    // Paint flags.
    pub const DRAW_BACKGROUND: Self = Self(1 << 16);
    pub const DRAW_BORDER: Self = Self(1 << 17);
    pub const DRAW_TEXT: Self = Self(1 << 18);
    pub const DRAW_HOT_EFFECTS: Self = Self(1 << 19);
    pub const CLIP: Self = Self(1 << 20);

    // Text editing capability.
    pub const TEXT_INPUT: Self = Self(1 << 21);
    pub const NO_WRAP_X: Self = Self(1 << 22);
    /// Identity bit marking a multi-line text area. Used to tell textareas apart from
    /// line edits when drawing the caret/selection, independent of styling flags like
    /// `DRAW_BORDER` (which a chromeless editor omits).
    pub const MULTILINE: Self = Self(1 << 23);
    /// Marks a box that paints arbitrary geometry via a deferred callback (see
    /// [`IMUI::canvas`]). The callback runs in the paint pass, after this box's
    /// background/border and before its children, clipped to the box.
    pub const CUSTOM_DRAW: Self = Self(1 << 24);

    /// Paints an inline image: the box's `display_string` is the image link key
    /// (`./blob/<name>`), resolved against the [`IMUI`] image registry.
    pub const DRAW_IMAGE: Self = Self(1 << 25);

    /// Word-wraps a `DRAW_TEXT` box's text to its resolved content width across
    /// multiple lines. Its `TextContent` height then follows the wrapped line
    /// count (layout solves width before height, so the width is known). See
    /// [`IMUI::wrapping_label`].
    pub const TEXT_WRAP: Self = Self(1 << 26);

    /// A `MULTILINE` box in `TextAreaLineStyle::Markdown` +
    /// `MarkdownMode::Rendered` (set by `textarea_impl`). DOM-backend-only:
    /// tells `paint_dom.rs` to host this box as a `contenteditable` `<div>`
    /// that paints its real rich-span children (bold/headers/hidden markers/
    /// inline images), instead of a plain `<textarea>` showing raw text —
    /// see `paint_dom.rs`'s `paint_richtext_host`. Native ignores this flag
    /// entirely (`paint.rs` always renders a `MULTILINE` box's children the
    /// same way regardless).
    pub const RICH_TEXT_HOST: Self = Self(1 << 27);

    pub const CLICKABLE: Self = Self(Self::MOUSE_CLICKABLE.0 | Self::KEYBOARD_CLICKABLE.0);
    pub const SCROLL: Self = Self(Self::SCROLL_X.0 | Self::SCROLL_Y.0);
    pub const BUTTON: Self = Self(
        Self::CLICKABLE.0
            | Self::DRAW_BACKGROUND.0
            | Self::DRAW_BORDER.0
            | Self::DRAW_TEXT.0
            | Self::DRAW_HOT_EFFECTS.0,
    );
    pub const LINE_EDIT: Self = Self(
        Self::MOUSE_CLICKABLE.0
            | Self::CLICK_TO_FOCUS.0
            | Self::TEXT_INPUT.0
            | Self::DRAW_BACKGROUND.0
            | Self::DRAW_BORDER.0
            | Self::DRAW_TEXT.0,
    );
    pub const TEXTAREA: Self = Self(
        Self::MOUSE_CLICKABLE.0
            | Self::CLICK_TO_FOCUS.0
            | Self::TEXT_INPUT.0
            | Self::DRAW_BACKGROUND.0
            | Self::SCROLL_Y.0
            | Self::CLIP.0
            | Self::MULTILINE.0,
    );

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    fn is_mouse_clickable(self) -> bool {
        self.contains(Self::MOUSE_CLICKABLE)
    }

    fn is_keyboard_clickable(self) -> bool {
        self.contains(Self::KEYBOARD_CLICKABLE)
    }

    fn click_to_focus(self) -> bool {
        self.contains(Self::CLICK_TO_FOCUS)
    }

    fn accepts_text_input(self) -> bool {
        self.contains(Self::TEXT_INPUT)
    }

    fn scrolls_x(self) -> bool {
        self.contains(Self::SCROLL_X)
    }

    fn scrolls_y(self) -> bool {
        self.contains(Self::SCROLL_Y)
    }
}

impl std::ops::BitOr for UIBoxFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for UIBoxFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct UIBoxStyle {
    pub margin: f32,
    pub border_size: f32,
    pub font_size: f32,
    pub bg_color: Color,
    pub text_color: Color,
    pub border_color: Color,
    pub font_icon: bool,
    pub corner_radius: f32,
    /// Center the single-line text within the content box (both axes) instead of
    /// the default top-left placement. Used by buttons so labels stay centered
    /// when the box is wider/taller than its text.
    pub text_align_center: bool,
    /// Multiplies this box's painted alpha, and every descendant's — the tree
    /// walk in `box_opacity` folds it in alongside the floating-pane appear
    /// animation. 1.0 = fully opaque. Set via [`UIBoxHandle::opacity`].
    pub opacity: f32,
}

impl Default for UIBoxStyle {
    fn default() -> Self {
        Self {
            margin: 2.0,
            border_size: 1.0,
            font_size: 14.0,
            bg_color: Color::new("#2b2d31"),
            text_color: Color::new("#f2f2f2"),
            border_color: Color::new("#55595f"),
            font_icon: false,
            corner_radius: 0.0,
            text_align_center: false,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UIBoxParams {
    pub width: Option<UISize>,
    pub height: Option<UISize>,
    pub bg_color: Option<Color>,
}

#[derive(Clone, Copy, Debug)]
struct MeasuredText {
    font_size: f32,
    font_icon: bool,
    size: Size,
}

/// Cached line breaks for a `TEXT_WRAP` box: `lines` are byte ranges into the
/// box's `display_string`. Recomputed only when the font, wrap width or text
/// changes (so wrapping is not per-frame work).
#[derive(Clone, Debug)]
struct WrappedText {
    font_size: f32,
    font_icon: bool,
    max_width: f32,
    lines: Vec<std::ops::Range<usize>>,
}

#[derive(Clone, Copy, Debug)]
pub struct TextAreaOptions {
    pub wrap_x: bool,
    pub scroll_x: bool,
    pub scroll_y: bool,
    /// Allow focus, selection, navigation and copy without mutating the buffer.
    pub read_only: bool,
    /// Font size to use while emitting wrapped visual lines.
    pub font_size: Option<f32>,
    /// Padding to use while emitting wrapped visual lines.
    pub padding: Option<Padding>,
    /// Draw the input border (and its focus ring). Disable for a chromeless editor.
    pub border: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextAreaLineStyle {
    Plain,
    Markdown,
}

impl Default for TextAreaOptions {
    fn default() -> Self {
        Self {
            wrap_x: true,
            scroll_x: false,
            scroll_y: true,
            read_only: false,
            font_size: None,
            padding: None,
            border: true,
        }
    }
}

impl TextAreaOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn wrap_x(mut self, wrap_x: bool) -> Self {
        self.wrap_x = wrap_x;
        self
    }

    pub fn scroll_x(mut self, scroll_x: bool) -> Self {
        self.scroll_x = scroll_x;
        self
    }

    pub fn scroll_y(mut self, scroll_y: bool) -> Self {
        self.scroll_y = scroll_y;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = Some(font_size.max(1.0));
        self
    }

    pub fn padding(mut self, padding: Padding) -> Self {
        self.padding = Some(padding);
        self
    }

    pub fn border(mut self, border: bool) -> Self {
        self.border = border;
        self
    }
}

/// How markdown syntax is presented inside an editable text area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownMode {
    /// Keep the literal syntax markers visible but styled (Obsidian "source" view).
    Source,
    /// Hide the syntax markers and show only the rendered result.
    Rendered,
}

/// One contiguous run of identically-styled glyphs on a visual line — or,
/// when `hidden`, one contiguous run of hidden markdown markers (`**`, `#`,
/// …) in `MarkdownMode::Rendered`: `text` then holds one placeholder char
/// per hidden raw char (not the literal markers — see `make_layout_line`),
/// used only so the span has the *right length*. Native never draws these
/// (`emit_layout_line` skips creating a box for them there at all — same
/// zero cost as before this field existed); the DOM backend still needs a
/// real (`font-size: 0`, effectively invisible) DOM text node at the right
/// raw offset for a hidden run, or the browser has nowhere to land a caret
/// that belongs *inside* it — landing one raw char short/long instead (this
/// was a real, confirmed bug: navigating onto a markdown line for the first
/// time could never place the caret exactly before/after a still-hidden
/// marker, only on the nearest already-visible text).
#[derive(Clone)]
struct LayoutSpan {
    text: String,
    color: Color,
    /// Raw-buffer char offset of this span's first character. Spans on a
    /// line are not generally contiguous in raw offset even discounting
    /// `hidden` ones (word-wrap splits a line into several `LayoutLine`s,
    /// each restarting its own spans) — each needs its own start rather than
    /// being inferable from the previous span's length. Used by the DOM
    /// backend to map a browser selection/edit position back to a raw offset
    /// (see `paint_dom.rs`'s rich-text host); native ignores it.
    raw_start: usize,
    hidden: bool,
}

/// A single visual (post-wrap) line of the editor.
#[derive(Clone)]
struct LayoutLine {
    /// Inclusive char offset (into the raw buffer) of this visual line's first char.
    raw_start: usize,
    /// Exclusive char offset of the last char (the trailing newline is excluded).
    raw_end: usize,
    font_size: f32,
    height: f32,
    /// Cumulative pixel x for each raw-char boundary; `cum_x.len() == raw_end - raw_start + 1`.
    /// Hidden marker chars contribute a zero-width step so cursor offsets still map cleanly.
    cum_x: Vec<f32>,
    /// Visible, drawable spans in left-to-right order.
    spans: Vec<LayoutSpan>,
    padding: Padding,
    /// Set when this line is a standalone inline image (`![alt](./blob/...)`):
    /// it is painted as a texture instead of text and reserves `height`.
    image: Option<ImageLine>,
}

impl LayoutLine {
    /// Height of the row box `emit_layout_line` gives this line. Kept here
    /// rather than read back off that box: with virtualization the box only
    /// exists while the line is inside the emitted window, and the cached
    /// layout has to be able to answer for every line regardless.
    fn row_height(&self) -> f32 {
        self.height + self.padding.vertical()
    }

    /// Width of the row box: its text (whole-run advances, so this is exactly
    /// what the glyphs occupy) plus its own horizontal padding.
    fn row_width(&self) -> f32 {
        match &self.image {
            Some(image) => image.width + self.padding.horizontal(),
            None => self.cum_x.last().copied().unwrap_or(0.0) + self.padding.horizontal(),
        }
    }
}

/// Which dimension an image link pins (`?h=` or `?w=`); the other is derived
/// from the intrinsic aspect ratio. `h` is the default for new images.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SizeAxis {
    Width,
    Height,
}

/// An image-only visual line, resolved from `![alt](./blob/<name>?h=NNN)`.
#[derive(Clone)]
struct ImageLine {
    /// `./blob/<name>` resolve key (link target minus the query).
    key: String,
    /// Display width in logical px (derived or pinned; clamped to content width).
    width: f32,
    /// Display height in logical px (derived or pinned).
    height: f32,
    /// Which dimension the link pins — the one a resize rewrites.
    control: SizeAxis,
}

/// A resize result produced by a corner drag: the new value for the pinned
/// dimension. The host rewrites the matching `?h=`/`?w=` in the link.
#[derive(Clone, Copy, Debug)]
pub enum ImageResize {
    Width(f32),
    Height(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkdownBlockKind {
    Quote,
    Code,
}

#[derive(Clone, Copy, Debug)]
struct LayoutBlock {
    kind: MarkdownBlockKind,
    first_visual_line: usize,
    last_visual_line: usize,
    padding: Padding,
    bg_color: Color,
    corner_radius: f32,
    label: Option<&'static str>,
    label_width: f32,
}

/// Cached, fully-resolved layout for a single text area. Recomputed only when the
/// content, wrap width, font, line style, or markdown mode actually changes — never
/// per frame for unchanged text.
struct EditorLayout {
    hash: u64,
    char_len: usize,
    width_key: u32,
    font_key: u32,
    line_style: TextAreaLineStyle,
    md_mode: MarkdownMode,
    reveal_line_start: Option<usize>,
    /// `IMUI::images_rev` at build time; a change recomputes image line heights.
    images_rev: u64,
    lines: Vec<LayoutLine>,
    blocks: Vec<LayoutBlock>,
    /// Prefix sums of the visual lines' row heights, gaps *excluded*:
    /// `line_tops[i] == Σ_{j<i} lines[j].row_height()`, with a final entry
    /// past the last line (so `len() == lines.len() + 1`). The text area's
    /// `child_gap` is folded back in by `line_top`, which keeps this vector
    /// independent of a gap the layout doesn't know about at build time.
    line_tops: Vec<f32>,
    /// Widest visual line. Only the *emitted* rows reach `max_child_size`, so
    /// a virtualized editor's horizontal scroll extent has to come from here
    /// instead — otherwise it would grow and shrink as you scroll vertically.
    max_line_width: f32,
    /// The visual lines the last frame actually emitted row boxes for. Lines
    /// outside it are stood in for by the two spacer boxes, so a line index
    /// is not a child index any more — go through `textarea_line_box`.
    emitted: Range<usize>,
}

impl EditorLayout {
    /// Content-space y of visual line `i`'s row box, `gap` being the text
    /// area's `child_gap`. Defined for `i == lines.len()` too, where it is the
    /// bottom of the last line — that is what the trailing spacer measures
    /// against.
    fn line_top(&self, i: usize, gap: f32) -> f32 {
        let i = i.min(self.lines.len());
        self.line_tops[i] + i as f32 * gap
    }

    /// The visual line containing content-space offset `y`, clamped into
    /// range. `line_top` is strictly increasing, so this is a binary search.
    fn line_at_offset(&self, y: f32, gap: f32) -> usize {
        let count = self.lines.len();
        if count == 0 {
            return 0;
        }
        let (mut lo, mut hi) = (0usize, count - 1);
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if self.line_top(mid, gap) <= y {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        lo
    }
}

/// Where one visual line's row sits on screen, derived from `EditorLayout`
/// alone so it is available for lines outside the emitted window too.
#[derive(Clone, Copy)]
struct LineRect {
    y0: f32,
    y1: f32,
    /// The row's own padding (a quote/code block indents its lines).
    padding: Padding,
    font_size: f32,
}

struct BuiltEditorLayout {
    lines: Vec<LayoutLine>,
    blocks: Vec<LayoutBlock>,
}

/// One raw char and how it should be presented: `display == None` means the char is a
/// hidden marker (zero advance); otherwise `Some(ch)` is the glyph to draw (which may
/// differ from the raw char, e.g. a rendered bullet).
type StyledChar = (Option<char>, Color);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CodeBlockExit {
    insert_newline_at: Option<usize>,
    cursor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodeLanguage {
    Generic,
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Shell,
    Json,
    Toml,
}

fn colors_eq(a: Color, b: Color) -> bool {
    a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a
}

fn find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == target)
}

fn find_double(chars: &[char], from: usize, target: char) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == target && chars[i + 1] == target {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn push_marker(out: &mut Vec<StyledChar>, ch: char, hidden: bool, color: Color) {
    out.push((if hidden { None } else { Some(ch) }, color));
}

fn code_fence_language(raw_line: &str) -> Option<CodeLanguage> {
    let trimmed = raw_line.trim_start_matches(|c| c == ' ' || c == '\t');
    let rest = trimmed.strip_prefix("```")?;
    let language = rest
        .trim_start()
        .split(|c: char| c.is_whitespace() || c == '{' || c == ',' || c == '}')
        .next()
        .unwrap_or("");
    Some(code_language_from_info(language))
}

fn code_language_from_info(language: &str) -> CodeLanguage {
    if language.eq_ignore_ascii_case("rs") || language.eq_ignore_ascii_case("rust") {
        CodeLanguage::Rust
    } else if matches_ignore_ascii(language, &["js", "jsx", "javascript", "mjs", "cjs"]) {
        CodeLanguage::JavaScript
    } else if matches_ignore_ascii(language, &["ts", "tsx", "typescript"]) {
        CodeLanguage::TypeScript
    } else if matches_ignore_ascii(language, &["py", "python", "python3"]) {
        CodeLanguage::Python
    } else if matches_ignore_ascii(language, &["sh", "bash", "zsh", "shell"]) {
        CodeLanguage::Shell
    } else if language.eq_ignore_ascii_case("json") {
        CodeLanguage::Json
    } else if language.eq_ignore_ascii_case("toml") {
        CodeLanguage::Toml
    } else {
        CodeLanguage::Generic
    }
}

impl CodeLanguage {
    fn label(self) -> &'static str {
        match self {
            CodeLanguage::Generic => "Plain text",
            CodeLanguage::Rust => "Rust",
            CodeLanguage::JavaScript => "JavaScript",
            CodeLanguage::TypeScript => "TypeScript",
            CodeLanguage::Python => "Python",
            CodeLanguage::Shell => "Shell",
            CodeLanguage::Json => "JSON",
            CodeLanguage::Toml => "TOML",
        }
    }
}

fn matches_ignore_ascii(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn style_code_fence_line(raw_line: &str, visible: bool, marker_color: Color) -> Vec<StyledChar> {
    raw_line
        .chars()
        .map(|ch| (if visible { Some(ch) } else { None }, marker_color))
        .collect()
}

fn code_block_padding() -> Padding {
    Padding {
        top: 3.0,
        right: 8.0,
        bottom: 3.0,
        left: 8.0,
    }
}

fn quote_block_padding() -> Padding {
    Padding {
        top: 2.0,
        right: 8.0,
        bottom: 2.0,
        left: 8.0,
    }
}

fn horizontal_padding(mut padding: Padding) -> Padding {
    padding.top = 0.0;
    padding.bottom = 0.0;
    padding
}

fn code_block_bg(theme: &UITheme) -> Color {
    match theme.kind {
        ThemeKind::Dark => Color::new("#20242bee"),
        ThemeKind::Light => Color::new("#eef1f4ee"),
    }
}

fn quote_block_bg(theme: &UITheme) -> Color {
    let mut color = theme.surface_hover;
    color.a *= 0.55;
    color
}

fn is_markdown_quote_line(raw_line: &str) -> bool {
    raw_line
        .trim_start_matches(|c| c == ' ' || c == '\t')
        .starts_with("> ")
}

fn raw_line_start_for_cursor(text: &str, cursor: usize) -> usize {
    let mut line_start = 0;
    for (idx, ch) in text.chars().enumerate() {
        if idx >= cursor {
            break;
        }
        if ch == '\n' {
            line_start = idx + 1;
        }
    }
    line_start
}

fn markdown_code_block_start_for_line_start(text: &str, line_start: usize) -> Option<usize> {
    let mut code_block_start = None;
    let mut offset = 0usize;
    let mut raw_lines = text.split('\n').peekable();
    while let Some(raw_line) = raw_lines.next() {
        if offset == line_start {
            return if code_fence_language(raw_line).is_some() {
                Some(code_block_start.unwrap_or(offset))
            } else {
                code_block_start
            };
        }
        if offset > line_start {
            break;
        }
        if code_fence_language(raw_line).is_some() {
            if code_block_start.is_some() {
                code_block_start = None;
            } else {
                code_block_start = Some(offset);
            }
        }
        offset += raw_line.chars().count() + usize::from(raw_lines.peek().is_some());
    }
    None
}

fn markdown_code_fence_enter_insert(text: &str, cursor: usize) -> Option<(String, usize)> {
    let cursor = cursor.min(char_count(text));
    if cursor != line_end(text, cursor) {
        return None;
    }
    let line_start = line_home(text, cursor);
    let line = substring_chars(text, (line_start, cursor));
    code_fence_language(&line)?;
    if markdown_in_code_block_before_line(text, line_start) {
        return None;
    }

    let indent_len = line
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .count();
    let indent = substring_chars(&line, (0, indent_len));
    let insert = format!("\n{indent}\n{indent}```");
    let caret_delta = 1 + indent_len;
    Some((insert, caret_delta))
}

fn markdown_in_code_block_before_line(text: &str, line_start: usize) -> bool {
    let mut in_code = false;
    let mut offset = 0;
    for raw_line in text.split('\n') {
        if offset >= line_start {
            break;
        }
        if code_fence_language(raw_line).is_some() {
            in_code = !in_code;
        }
        offset += raw_line.chars().count() + 1;
    }
    in_code
}

fn markdown_exit_code_block_after_current_line(text: &str, cursor: usize) -> Option<CodeBlockExit> {
    let cursor = cursor.min(char_count(text));
    let current_start = line_home(text, cursor);
    let current_end = line_end(text, cursor);
    let in_code = markdown_in_code_block_before_line(text, current_start);
    if !in_code {
        return None;
    }

    let closing_start =
        if code_fence_language(&substring_chars(text, (current_start, current_end))).is_some() {
            current_start
        } else if current_end < char_count(text) {
            current_end + 1
        } else {
            return None;
        };
    let closing_end = line_end(text, closing_start);
    let closing_line = substring_chars(text, (closing_start, closing_end));
    code_fence_language(&closing_line)?;

    if closing_end < char_count(text) {
        Some(CodeBlockExit {
            insert_newline_at: None,
            cursor: closing_end + 1,
        })
    } else {
        Some(CodeBlockExit {
            insert_newline_at: Some(closing_end),
            cursor: closing_end + 1,
        })
    }
}

fn push_layout_line(
    lines: &mut Vec<LayoutLine>,
    blocks: &mut Vec<LayoutBlock>,
    line: LayoutLine,
    block: Option<(
        MarkdownBlockKind,
        Padding,
        Color,
        f32,
        Option<&'static str>,
        f32,
    )>,
) {
    let visual_line = lines.len();
    if let Some((kind, padding, bg_color, corner_radius, label, label_width)) = block {
        if let Some(last) = blocks.last_mut()
            && last.kind == kind
            && last.last_visual_line + 1 == visual_line
            && last.padding == padding
            && colors_eq(last.bg_color, bg_color)
            && (last.corner_radius - corner_radius).abs() < 0.001
            && last.label == label
        {
            last.last_visual_line = visual_line;
        } else {
            blocks.push(LayoutBlock {
                kind,
                first_visual_line: visual_line,
                last_visual_line: visual_line,
                padding,
                bg_color,
                corner_radius,
                label,
                label_width,
            });
        }
    }
    lines.push(line);
}

fn style_code_line(raw_line: &str, language: CodeLanguage, theme: &UITheme) -> Vec<StyledChar> {
    let chars: Vec<char> = raw_line.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if code_line_comment_starts(&chars, i, language) {
            for &ch in &chars[i..] {
                out.push((Some(ch), theme.text_muted));
            }
            break;
        }

        let ch = chars[i];
        if code_string_quote(ch, language) {
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < chars.len() {
                let current = chars[i];
                i += 1;
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == ch {
                    break;
                }
            }
            for &string_ch in &chars[start..i] {
                out.push((Some(string_ch), theme.accent_active));
            }
        } else if ch.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < chars.len() && is_code_number_char(chars[i]) {
                i += 1;
            }
            for &number_ch in &chars[start..i] {
                out.push((Some(number_ch), theme.text_accent));
            }
        } else if is_code_ident_start(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && is_code_ident_continue(chars[i]) {
                i += 1;
            }
            let color = if code_keyword(language, &chars[start..i]) {
                theme.accent
            } else {
                theme.text
            };
            for &ident_ch in &chars[start..i] {
                out.push((Some(ident_ch), color));
            }
        } else {
            out.push((Some(ch), theme.text));
            i += 1;
        }
    }
    out
}

fn code_line_comment_starts(chars: &[char], i: usize, language: CodeLanguage) -> bool {
    match language {
        CodeLanguage::Rust | CodeLanguage::JavaScript | CodeLanguage::TypeScript => {
            chars.get(i) == Some(&'/') && chars.get(i + 1) == Some(&'/')
        }
        CodeLanguage::Python | CodeLanguage::Shell | CodeLanguage::Toml => {
            chars.get(i) == Some(&'#')
        }
        CodeLanguage::Generic | CodeLanguage::Json => false,
    }
}

fn code_string_quote(ch: char, language: CodeLanguage) -> bool {
    ch == '"'
        || ch == '\''
        || (ch == '`'
            && matches!(
                language,
                CodeLanguage::JavaScript | CodeLanguage::TypeScript | CodeLanguage::Shell
            ))
}

fn is_code_number_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '.' || ch == '_'
}

fn is_code_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_code_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn code_keyword(language: CodeLanguage, token: &[char]) -> bool {
    let table = match language {
        CodeLanguage::Rust => RUST_KEYWORDS,
        CodeLanguage::JavaScript => JS_KEYWORDS,
        CodeLanguage::TypeScript => TS_KEYWORDS,
        CodeLanguage::Python => PYTHON_KEYWORDS,
        CodeLanguage::Shell => SHELL_KEYWORDS,
        CodeLanguage::Json => JSON_KEYWORDS,
        CodeLanguage::Toml => TOML_KEYWORDS,
        CodeLanguage::Generic => GENERIC_CODE_KEYWORDS,
    };
    table
        .iter()
        .any(|keyword| char_slice_eq_str(token, keyword))
}

fn char_slice_eq_str(chars: &[char], text: &str) -> bool {
    chars.len() == text.len()
        && chars
            .iter()
            .zip(text.bytes())
            .all(|(&ch, byte)| ch as u8 == byte)
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

const JS_KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "if",
    "import",
    "in",
    "let",
    "new",
    "null",
    "return",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "yield",
];

const TS_KEYWORDS: &[&str] = &[
    "any",
    "as",
    "async",
    "await",
    "boolean",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "interface",
    "keyof",
    "let",
    "namespace",
    "never",
    "new",
    "null",
    "number",
    "private",
    "protected",
    "public",
    "readonly",
    "return",
    "string",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "unknown",
    "var",
    "void",
    "while",
    "yield",
];

const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

const SHELL_KEYWORDS: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in", "then",
    "until", "while",
];

const JSON_KEYWORDS: &[&str] = &["false", "null", "true"];
const TOML_KEYWORDS: &[&str] = &["false", "true"];
const GENERIC_CODE_KEYWORDS: &[&str] = &[
    "class", "const", "def", "else", "false", "fn", "for", "function", "if", "let", "null",
    "return", "true", "var", "while",
];

/// Inline markdown scan (bold/italic/code/link) over already-de-blocked content chars.
/// Appends one [`StyledChar`] per raw char so offset math stays exact.
fn scan_inline(
    content: &[char],
    base_color: Color,
    marker_color: Color,
    hidden: bool,
    theme: &UITheme,
    out: &mut Vec<StyledChar>,
) {
    let n = content.len();
    let mut i = 0;
    while i < n {
        let c = content[i];

        // Inline code: `code` — no nested formatting inside.
        if c == '`' {
            if let Some(close) = find_char(content, i + 1, '`') {
                push_marker(out, '`', hidden, marker_color);
                for &cc in &content[i + 1..close] {
                    out.push((Some(cc), theme.accent_active));
                }
                push_marker(out, '`', hidden, marker_color);
                i = close + 1;
                continue;
            }
        }

        // Bold: **text** or __text__ (allows nested emphasis).
        if (c == '*' || c == '_') && content.get(i + 1) == Some(&c) {
            if let Some(close) = find_double(content, i + 2, c) {
                push_marker(out, c, hidden, marker_color);
                push_marker(out, c, hidden, marker_color);
                scan_inline(
                    &content[i + 2..close],
                    theme.text_accent,
                    marker_color,
                    hidden,
                    theme,
                    out,
                );
                push_marker(out, c, hidden, marker_color);
                push_marker(out, c, hidden, marker_color);
                i = close + 2;
                continue;
            }
        }

        // Italic: *text* or _text_.
        if c == '*' || c == '_' {
            if let Some(close) = find_char(content, i + 1, c) {
                if close > i + 1 {
                    push_marker(out, c, hidden, marker_color);
                    for &cc in &content[i + 1..close] {
                        out.push((Some(cc), theme.accent));
                    }
                    push_marker(out, c, hidden, marker_color);
                    i = close + 1;
                    continue;
                }
            }
        }

        // Link: [text](url) — show text, hide the URL machinery in rendered mode.
        if c == '[' {
            if let Some(rbr) = find_char(content, i + 1, ']') {
                if content.get(rbr + 1) == Some(&'(') {
                    if let Some(rpar) = find_char(content, rbr + 2, ')') {
                        push_marker(out, '[', hidden, marker_color);
                        for &cc in &content[i + 1..rbr] {
                            out.push((Some(cc), theme.accent));
                        }
                        push_marker(out, ']', hidden, marker_color);
                        push_marker(out, '(', hidden, marker_color);
                        for &cc in &content[rbr + 2..rpar] {
                            push_marker(out, cc, hidden, marker_color);
                        }
                        push_marker(out, ')', hidden, marker_color);
                        i = rpar + 1;
                        continue;
                    }
                }
            }
        }

        out.push((Some(c), base_color));
        i += 1;
    }
}

/// Resolve a single raw buffer line into styled chars plus its block font size/height.
fn style_raw_line(
    raw_line: &str,
    base_font: f32,
    line_style: TextAreaLineStyle,
    md_mode: MarkdownMode,
    is_focused_line: bool,
    theme: &UITheme,
) -> (Vec<StyledChar>, f32, f32) {
    let chars: Vec<char> = raw_line.chars().collect();
    let mut font_size = base_font;
    let mut height = base_font + 4.0;

    if line_style != TextAreaLineStyle::Markdown {
        let out = chars.iter().map(|&c| (Some(c), theme.text)).collect();
        return (out, font_size, height);
    }

    let marker_color = theme.text_muted;
    // Markers stay visible on the line the caret is currently on — same
    // reveal-on-focus treatment `reveal_code_block_start` already gives code
    // fences (see `build_editor_layout_revealing_line`). Without this, a line
    // that is *only* markers (e.g. "# " before any title text is typed) has
    // every char hidden, which collapses to no `LayoutLine` at all and the
    // rich-text host loses its DOM anchor for that raw range mid-edit.
    let hidden = md_mode == MarkdownMode::Rendered && !is_focused_line;
    let mut base_color = theme.text;

    let indent = chars
        .iter()
        .take_while(|c| **c == ' ' || **c == '\t')
        .count();
    let mut out: Vec<StyledChar> = Vec::with_capacity(chars.len());
    for &c in &chars[..indent] {
        out.push((Some(c), base_color));
    }
    let rest = &chars[indent..];

    let hashes = rest.iter().take_while(|c| **c == '#').count();
    let content_start;
    if (1..=6).contains(&hashes) && rest.get(hashes) == Some(&' ') {
        // Size is a *rendered* affordance only. In source view the markers are
        // part of the text being edited, and scaling the line reflows it under
        // the caret as the `#`s are typed or deleted — the line jumps size
        // mid-keystroke and every column offset on it moves with it. Colour
        // still distinguishes a heading, which is all the source view needs.
        if md_mode == MarkdownMode::Rendered {
            font_size = match hashes {
                1 => base_font * 1.85,
                2 => base_font * 1.55,
                3 => base_font * 1.32,
                _ => base_font * 1.16,
            };
            height = font_size + 10.0;
        }
        base_color = if hashes == 1 {
            theme.text_accent
        } else {
            theme.text
        };
        for &c in &rest[..hashes] {
            push_marker(&mut out, c, hidden, marker_color);
        }
        push_marker(&mut out, ' ', hidden, marker_color);
        content_start = hashes + 1;
    } else if rest.first() == Some(&'>') && rest.get(1) == Some(&' ') {
        base_color = theme.text_muted;
        height = base_font + 8.0;
        push_marker(&mut out, '>', false, marker_color);
        push_marker(&mut out, ' ', false, marker_color);
        content_start = 2;
    } else if matches!(rest.first(), Some('-') | Some('*') | Some('+')) && rest.get(1) == Some(&' ')
    {
        height = base_font + 6.0;
        let disp = if hidden { '\u{2022}' } else { rest[0] };
        out.push((Some(disp), theme.accent));
        out.push((Some(' '), base_color));
        content_start = 2;
    } else {
        content_start = 0;
    }

    scan_inline(
        &rest[content_start..],
        base_color,
        marker_color,
        hidden,
        theme,
        &mut out,
    );
    (out, font_size, height)
}

/// Zero-width placeholder for a hidden markdown marker char (see
/// `LayoutSpan`'s doc comment) — any single `char` would do since it's never
/// drawn at a real size; U+200B reads as "intentionally invisible" if ever
/// inspected in a DOM dump.
const HIDDEN_MARKER_PLACEHOLDER: char = '\u{200B}';

/// Assemble one visual line (cum_x + drawable spans) from a styled-char slice.
fn make_layout_line(
    styled: &[StyledChar],
    advances: &[f32],
    start: usize,
    end: usize,
    char_offset: usize,
    font_size: f32,
    height: f32,
    padding: Padding,
) -> LayoutLine {
    let mut cum_x = Vec::with_capacity(end - start + 1);
    cum_x.push(0.0);
    let mut x = 0.0;
    let mut spans: Vec<LayoutSpan> = Vec::new();
    for k in start..end {
        x += advances[k];
        cum_x.push(x);
        let (ch, color, hidden) = match styled[k] {
            (Some(ch), color) => (ch, color, false),
            (None, color) => (HIDDEN_MARKER_PLACEHOLDER, color, true),
        };
        if let Some(last) = spans.last_mut() {
            if last.hidden == hidden && colors_eq(last.color, color) {
                last.text.push(ch);
                continue;
            }
        }
        spans.push(LayoutSpan {
            text: ch.to_string(),
            color,
            raw_start: char_offset + k,
            hidden,
        });
    }
    LayoutLine {
        raw_start: char_offset + start,
        raw_end: char_offset + end,
        font_size,
        height,
        cum_x,
        spans,
        padding,
        image: None,
    }
}

pub trait TextEditBuffer {
    fn text(&self) -> String;
    fn insert_text(&mut self, index: usize, text: &str);
    fn delete_range(&mut self, range: (usize, usize));
}

impl TextEditBuffer for String {
    fn text(&self) -> String {
        self.clone()
    }

    fn insert_text(&mut self, index: usize, text: &str) {
        let byte = char_to_byte(self, index);
        self.insert_str(byte, text);
    }

    fn delete_range(&mut self, range: (usize, usize)) {
        delete_char_range(self, range);
    }
}

impl UIBoxParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn width(&mut self, width: UISize) -> &mut Self {
        self.width = Some(width);
        self
    }

    pub fn height(&mut self, height: UISize) -> &mut Self {
        self.height = Some(height);
        self
    }

    pub fn bg_color(&mut self, color: Color) -> &mut Self {
        self.bg_color = Some(color);
        self
    }
}

/// A deferred paint callback for [`UIBoxFlags::CUSTOM_DRAW`] boxes, registered
/// via [`IMUI::canvas`]. Called once during the paint pass with `(drawer,
/// content_rect, clip_rect)`: `content_rect` is the box's full logical rect (use
/// it to map your geometry), `clip_rect` is the visible region after scroll/clip
/// (intersect your draws with it, since `Drawer::draw_rect` does not clip).
///
/// Stored in a per-frame arena on [`IMUI`], so it must be `'static`: capture
/// owned/`Copy`/`Arc` data (e.g. an `Arc` waveform peak cache), never borrows
/// from the per-frame build closure.
pub type CanvasPaint = Box<dyn FnMut(&mut Drawer, RectCoords, RectCoords)>;

/// A decoded inline image. Owned by [`IMUI::images`]. The GPU upload is deferred
/// to the paint pass (like font atlases) — uploading during the build pass can
/// hit a renderer/context state where texture creation silently fails.
struct ImageEntry {
    width: u32,
    height: u32,
    /// GPU texture id once uploaded (0 until then). Always 0 for an
    /// `encoded_mime` entry — the DOM backend never uploads a GPU texture.
    tex_id: u32,
    /// Awaiting upload during paint (native) or a DOM `<img>` Blob (web);
    /// taken once uploaded natively. Interpreted per `encoded_mime` below.
    pending: Option<Vec<u8>>,
    /// `None`: `pending` is raw RGBA8, as native decode produces (see
    /// [`IMUI::provide_image`]). `Some(mime)`: `pending` is already-encoded
    /// image bytes (PNG/JPEG/…) in this MIME type, as the DOM backend
    /// receives it (see [`IMUI::provide_image_encoded`]) — the browser
    /// decodes those bytes itself via a `<img src="blob:...">`, so they're
    /// never turned into pixels on the Rust side at all. Only ever read by
    /// the DOM paint path (`image_dom_bytes`); native never sets or reads it.
    #[cfg_attr(not(feature = "dom"), allow(dead_code))]
    encoded_mime: Option<&'static str>,
}

/// Live drag state while resizing an inline image by its bottom-right corner.
struct ImageDrag {
    key: String,
    /// Which dimension is being resized (matches the link's pinned axis).
    control: SizeAxis,
    /// Pinned dimension's value when the drag began, so it tracks the cursor
    /// rather than running away as the link is rewritten each frame.
    start: f32,
    press_pos: Point,
}

/// Minimum displayed size (either axis) an image can be resized to, in px.
const MIN_IMAGE_SIZE: f32 = 48.0;

#[derive(Clone, Debug)]
pub struct UIBox {
    pub key: UiKey,
    parent: Option<usize>,
    children: Vec<usize>,
    debug_label: Option<String>,
    /// The `###`-suffix half of the widget's label — the part that seeds
    /// [`UiKey`] and therefore stays put when the visible text changes. Retained
    /// so tests and the DOM backend can address a widget by its stable id
    /// instead of its display text; `alloc_box` already builds this string every
    /// frame, so keeping it costs no extra allocation.
    ///
    /// Only read by the test snapshot (`feature = "testkit"`) and the DOM
    /// backend's `data-mae-key` (`feature = "dom"`), so a plain build sees it
    /// as dead — it is still cheaper to always retain than to cfg the field
    /// through every constructor.
    #[cfg_attr(
        not(any(feature = "testkit", feature = "dom")),
        allow(
            dead_code,
            reason = "read only by the testkit snapshot and the DOM backend"
        )
    )]
    key_id: Option<String>,
    string: Option<String>,
    display_string: Option<String>,
    flags: UIBoxFlags,
    pref_size: [UISize; 2],
    min_size: Size,
    scroll: Point,
    scroll_target: Point,
    scroll_max: Point,
    content_size: Size,
    measured_text: Option<MeasuredText>,
    /// Cached wrap layout for a `TEXT_WRAP` box (see [`WrappedText`]).
    wrapped: Option<WrappedText>,
    fixed_position: Point,
    computed_size: Size,
    rect: RectCoords,
    previous_clip_rect: RectCoords,
    cursor: Option<OSCursor>,
    hit_padding: Padding,
    child_layout_axis: Axis,
    padding: Padding,
    child_gap: f32,
    main_axis_align: MainAxisAlign,
    cross_axis_align: CrossAxisAlign,
    style: UIBoxStyle,
    /// Byte ranges within `display_string` to paint a highlight rect behind
    /// (e.g. search-match highlighting). Reset every frame.
    text_highlights: Vec<(usize, usize)>,
    highlight_color: Color,
    bg_color_animated: Color,
    border_color_animated: Color,
    hot_t: f32,
    active_t: f32,
    focus_t: f32,
    appear_t: f32,
    scrollbar_x_t: f32,
    scrollbar_y_t: f32,
    // Per-frame interaction result for the retained box.
    signal: UISignal,
    /// Index into `IMUI::canvas_paints` for a `CUSTOM_DRAW` box; set fresh each
    /// frame by `canvas()` and cleared (to `None`) on reset.
    canvas_paint: Option<usize>,
    visible: bool,
    first_touched_frame: u64,
    last_touched_frame: u64,
    /// `(raw_start, raw_end)` for a row/span/image child of a `RICH_TEXT_HOST`
    /// textarea (set by `emit_layout_line`), `None` for every other box. The
    /// DOM backend stamps these as `data-raw-start`/`data-raw-end` attributes
    /// so a browser Selection/Range can be translated back to a raw buffer
    /// offset (see `paint_dom.rs`'s rich-text host); native ignores it.
    richtext_span: Option<(usize, usize)>,
}

impl UIBox {
    fn new(
        key: UiKey,
        flags: UIBoxFlags,
        string: Option<String>,
        key_id: Option<String>,
        theme: &UITheme,
    ) -> Self {
        Self {
            key,
            parent: None,
            children: Vec::new(),
            key_id,
            debug_label: string.clone(),
            string: string.clone(),
            display_string: string,
            flags,
            pref_size: [UISize::ChildrenSum, UISize::ChildrenSum],
            min_size: Size::default(),
            scroll: Point::default(),
            scroll_target: Point::default(),
            scroll_max: Point::default(),
            content_size: Size::default(),
            measured_text: None,
            wrapped: None,
            fixed_position: Point::default(),
            computed_size: Size::default(),
            rect: RectCoords::from_size(0.0, 0.0, 0.0, 0.0),
            previous_clip_rect: RectCoords::from_size(0.0, 0.0, 0.0, 0.0),
            cursor: None,
            hit_padding: Padding::default(),
            child_layout_axis: Axis::Y,
            padding: Padding::default(),
            child_gap: 0.0,
            main_axis_align: MainAxisAlign::Start,
            cross_axis_align: CrossAxisAlign::Start,
            style: UIBoxStyle {
                font_size: theme.size_text,
                bg_color: theme.color_bg_popup,
                text_color: theme.color_text,
                ..UIBoxStyle::default()
            },
            text_highlights: Vec::new(),
            highlight_color: Color::transparent(),
            bg_color_animated: theme.color_bg_popup,
            border_color_animated: theme.border,
            hot_t: 0.0,
            active_t: 0.0,
            focus_t: 0.0,
            appear_t: 0.0,
            scrollbar_x_t: 0.0,
            scrollbar_y_t: 0.0,
            signal: UISignal::default(),
            canvas_paint: None,
            visible: true,
            first_touched_frame: 0,
            last_touched_frame: 0,
            richtext_span: None,
        }
    }

    pub fn bounds(&self) -> RectCoords {
        self.rect
    }

    fn reset_for_frame(
        &mut self,
        key: UiKey,
        flags: UIBoxFlags,
        string: Option<String>,
        key_id: Option<String>,
        theme: &UITheme,
    ) {
        let rect = self.rect;
        let computed_size = self.computed_size;
        let previous_clip_rect = self.previous_clip_rect;
        let scroll = self.scroll;
        let scroll_target = self.scroll_target;
        let scroll_max = self.scroll_max;
        let content_size = self.content_size;
        let keep_text = self.display_string == string;
        let measured_text = if keep_text { self.measured_text } else { None };
        let wrapped = if keep_text {
            std::mem::take(&mut self.wrapped)
        } else {
            None
        };
        let bg_color_animated = self.bg_color_animated;
        let border_color_animated = self.border_color_animated;
        let hot_t = self.hot_t;
        let active_t = self.active_t;
        let focus_t = self.focus_t;
        let appear_t = self.appear_t;
        let scrollbar_x_t = self.scrollbar_x_t;
        let scrollbar_y_t = self.scrollbar_y_t;
        let first_touched_frame = self.first_touched_frame;
        let last_touched_frame = self.last_touched_frame;

        *self = Self::new(key, flags, string, key_id, theme);
        self.debug_label = self.display_string.clone();
        self.rect = rect;
        self.computed_size = computed_size;
        self.previous_clip_rect = previous_clip_rect;
        self.scroll = scroll;
        self.scroll_target = scroll_target;
        self.scroll_max = scroll_max;
        self.content_size = content_size;
        self.measured_text = measured_text;
        self.wrapped = wrapped;
        self.bg_color_animated = bg_color_animated;
        self.border_color_animated = border_color_animated;
        self.hot_t = hot_t;
        self.active_t = active_t;
        self.focus_t = focus_t;
        self.appear_t = appear_t;
        self.scrollbar_x_t = scrollbar_x_t;
        self.scrollbar_y_t = scrollbar_y_t;
        self.first_touched_frame = first_touched_frame;
        self.last_touched_frame = last_touched_frame;
    }
}

/// Preferred side for an [`IMUI::anchored_pane`], relative to its anchor rect.
/// The pane flips to the opposite side when the preferred one would overflow
/// the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopoverSide {
    /// Below the anchor, left edges aligned — dropdowns, the space switcher.
    Below,
    /// Above the anchor, left edges aligned.
    Above,
    /// To the anchor's right, top edges aligned — submenus.
    Right,
    /// To the anchor's left, top edges aligned.
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug)]
pub struct UIMotion {
    pub hot_rate: f32,
    pub active_rate: f32,
    pub focus_rate: f32,
    pub menu_rate: f32,
    pub tooltip_rate: f32,
    pub scroll_rate: f32,
    pub epsilon: f32,
}

impl Default for UIMotion {
    fn default() -> Self {
        Self {
            hot_rate: 34.0,
            active_rate: 42.0,
            focus_rate: 28.0,
            menu_rate: 30.0,
            tooltip_rate: 24.0,
            scroll_rate: 26.0,
            epsilon: 0.01,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct UITheme {
    pub kind: ThemeKind,
    pub color_bg: Color,
    pub color_bg_popup: Color,
    pub color_main: Color,
    pub color_text: Color,
    pub app_bg: Color,
    pub sidebar_bg: Color,
    pub panel_bg: Color,
    pub surface_bg: Color,
    pub surface_hover: Color,
    pub surface_active: Color,
    pub input_bg: Color,
    pub popover_bg: Color,
    pub border: Color,
    pub border_muted: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_accent: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_active: Color,
    /// Semantic status colours (toasts, badges, validation).
    pub info: Color,
    pub warning: Color,
    pub danger: Color,
    pub selection: Color,
    pub scrollbar: Color,
    pub size_text: f32,
    pub control_h: f32,
    pub toolbar_h: f32,
    pub sidebar_w: f32,
    pub radius: f32,
    pub gap_sm: f32,
    pub gap_md: f32,
    pub gap_lg: f32,
    pub pad_sm: f32,
    pub pad_md: f32,
    pub pad_lg: f32,
    pub motion: UIMotion,
}

impl Default for UITheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl UITheme {
    pub fn dark() -> Self {
        Self {
            kind: ThemeKind::Dark,
            color_bg: Color::new("#11151b"),
            color_bg_popup: Color::new("#1f2630"),
            color_main: Color::new("#3f9f95"),
            color_text: Color::new("#eef2f6"),
            app_bg: Color::new("#0d111700"),
            sidebar_bg: Color::new("#151b23d8"),
            panel_bg: Color::new("#111820dc"),
            surface_bg: Color::new("#1c2430e5"),
            surface_hover: Color::new("#26313eee"),
            surface_active: Color::new("#183f4aee"),
            input_bg: Color::new("#0f151de8"),
            popover_bg: Color::new("#18202af2"),
            border: Color::new("#33404f"),
            border_muted: Color::new("#24303d"),
            text: Color::new("#eef2f6"),
            text_muted: Color::new("#9ba8b5"),
            text_accent: Color::new("#8fc7ff"),
            accent: Color::new("#3f9f95"),
            accent_hover: Color::new("#51b7ac"),
            accent_active: Color::new("#2d776f"),
            info: Color::new("#5aa9ff"),
            warning: Color::new("#f2b13c"),
            danger: Color::new("#f0645f"),
            selection: Color::new("#285f73"),
            scrollbar: Color::new("#7f8a9655"),
            size_text: 14.0,
            control_h: 32.0,
            toolbar_h: 44.0,
            sidebar_w: 216.0,
            radius: 6.0,
            gap_sm: 6.0,
            gap_md: 10.0,
            gap_lg: 14.0,
            pad_sm: 6.0,
            pad_md: 10.0,
            pad_lg: 14.0,
            motion: UIMotion::default(),
        }
    }

    pub fn light() -> Self {
        Self {
            kind: ThemeKind::Light,
            color_bg: Color::new("#f4f7fb"),
            color_bg_popup: Color::new("#ffffff"),
            color_main: Color::new("#247f78"),
            color_text: Color::new("#17202a"),
            app_bg: Color::new("#edf2f700"),
            sidebar_bg: Color::new("#f8fafcd8"),
            panel_bg: Color::new("#ffffffdc"),
            surface_bg: Color::new("#f3f6fae5"),
            surface_hover: Color::new("#e6edf5ee"),
            surface_active: Color::new("#d8f0eeee"),
            input_bg: Color::new("#ffffffe8"),
            popover_bg: Color::new("#fffffff2"),
            border: Color::new("#c8d3df"),
            border_muted: Color::new("#dde5ee"),
            text: Color::new("#17202a"),
            text_muted: Color::new("#617080"),
            text_accent: Color::new("#256fa6"),
            accent: Color::new("#247f78"),
            accent_hover: Color::new("#2f968e"),
            accent_active: Color::new("#1d655f"),
            info: Color::new("#2f7fe0"),
            warning: Color::new("#c9821b"),
            danger: Color::new("#d63d3d"),
            selection: Color::new("#b9e3e0"),
            scrollbar: Color::new("#8392a655"),
            size_text: 14.0,
            control_h: 32.0,
            toolbar_h: 44.0,
            sidebar_w: 216.0,
            radius: 6.0,
            gap_sm: 6.0,
            gap_md: 10.0,
            gap_lg: 14.0,
            pad_sm: 6.0,
            pad_md: 10.0,
            pad_lg: 14.0,
            motion: UIMotion::default(),
        }
    }

    pub fn for_kind(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Dark => Self::dark(),
            ThemeKind::Light => Self::light(),
        }
    }
}

/// A remote collaborator's caret inside a textarea: a colored bar at a char
/// index, with a small initial-letter badge above it and an optional
/// translucent selection highlight.
#[derive(Clone, Debug)]
pub struct RemoteCaret {
    /// Caret position as a char index into the textarea's text.
    pub cursor: usize,
    /// Selected char range (normalized, start < end), highlighted in a
    /// translucent version of `color`.
    pub selection: Option<(usize, usize)>,
    pub color: Color,
    /// Collaborator label; only the first char is drawn in the badge.
    pub label: String,
}

/// Shared slot holding whatever the active `run_dom` loop needs to actually
/// schedule its next `requestAnimationFrame` tick — set once `run_dom` starts
/// (empty before that, since a `wake()` before the loop exists has nothing
/// useful to do). Boxed `Fn()` rather than a generic so `RepaintWaker` stays a
/// plain, unparameterized type usable from `os/wasm.rs`'s listener closures.
#[cfg(target_arch = "wasm32")]
pub(super) type TickScheduler = std::rc::Rc<std::cell::RefCell<Option<Box<dyn Fn()>>>>;

/// Handle that wakes the event loop so it rebuilds the UI (e.g. when a sync
/// engine receives a remote update while the app is idle). On native
/// platforms this is a Send + Sync flag honored on the next loop iteration
/// (`eventloop_with_shutdown` polls it every pass, including cross-thread).
/// On wasm32, `run_dom` ticks only in direct response to a real trigger rather
/// than polling forever (see its doc comment): every path that can make new work available — DOM
/// pointer/edit listeners (`paint_dom.rs`), the container-level input
/// listeners (`os/wasm.rs`), and any other holder of a `RepaintWaker` — goes
/// through this same type, and both its methods drive the exact same
/// underlying [`TickScheduler`], so there is exactly one place responsible
/// for actually scheduling a tick.
///
/// The two methods differ only in whether they *force* the next tick to
/// rebuild:
/// - [`wake`](Self::wake) also sets the cross-thread repaint flag, so the
///   next tick rebuilds unconditionally — right for a discrete, already-
///   meaningful event (a DOM click/edit, a background update) that should
///   always produce a new frame.
/// - [`schedule_tick`](Self::schedule_tick) only ensures a tick runs soon,
///   leaving whether it *rebuilds* to `run_dom`'s own per-event-type check
///   (`has_actionable_event`) — right for raw input where a pure mousemove
///   with no button held shouldn't force a rebuild (hover feedback there is
///   CSS-driven; see `os/wasm.rs`), but the queue still needs draining soon
///   rather than sitting unprocessed until something else happens to wake.
#[derive(Clone)]
pub struct RepaintWaker {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(target_arch = "wasm32")]
    scheduler: TickScheduler,
}

impl RepaintWaker {
    pub fn wake(&self) {
        self.flag.store(true, std::sync::atomic::Ordering::Release);
        self.schedule_tick();
    }

    /// See the type doc comment for how this differs from [`wake`](Self::wake).
    /// A no-op on native, where there's no separate tick to schedule.
    pub fn schedule_tick(&self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(schedule) = self.scheduler.borrow().as_ref() {
            schedule();
        }
    }
}

pub struct IMUI {
    drawer: Option<Drawer>,
    size: Size,
    events: Vec<OSEvent>,
    /// Where a mouse button was *pressed* during the frame under construction,
    /// recorded as the events arrive and cleared when the frame ends.
    ///
    /// Separate from `events` because that queue is *consumed*: the first
    /// clickable box under the pointer removes the press from it (see
    /// `remove_event`). [`press_outside`](IMUI::press_outside) — the dismissal
    /// test every popover, menu and palette runs — must still see a press that
    /// landed on a widget outside the pane, which is precisely the common case:
    /// dismissing a palette by clicking a row in the sidebar behind it.
    frame_presses: Vec<Point>,
    mouse: Option<Point>,
    left_mouse_down: bool,
    right_mouse_down: bool,
    hot_key: Option<UiKey>,
    /// Hover winner (`hot_key`) from the previous frame. Compared at the end of
    /// each frame so a hover change can request a repaint even when nothing else
    /// is animating.
    prev_hot_key: Option<UiKey>,
    pointer_blacklist_rects: Vec<RectCoords>,
    scrollbar_hit_areas: Vec<ScrollbarHitArea>,
    active_left_key: Option<UiKey>,
    active_right_key: Option<UiKey>,
    drag_start_mouse: Option<Point>,
    text_click_streak: TextClickStreak,
    active_scrollbar: Option<ScrollbarDrag>,
    focus_key: Option<UiKey>,
    next_focus_key: Option<UiKey>,
    /// Active IME preedit (composing) string from the OS, rendered inline at the
    /// focused editor's caret. Refreshed each frame; not part of any buffer.
    ime_preedit: Option<String>,
    cursor: OSCursor,
    text_edit_states: HashMap<UiKey, TextEditState>,
    editor_layouts: HashMap<UiKey, EditorLayout>,
    /// Per-textarea undo/redo history, keyed like [`Self::text_edit_states`].
    undo_states: HashMap<UiKey, UndoHistory>,
    markdown_mode: MarkdownMode,
    clipboard: String,
    /// Inline-document images, keyed by their `./blob/<name>` link target. The
    /// host feeds decoded RGBA via [`IMUI::provide_image`]; values hold the
    /// uploaded GPU texture id + intrinsic size for layout/paint.
    images: HashMap<String, ImageEntry>,
    /// Link names referenced this frame but not yet in `images`. The host drains
    /// it via [`IMUI::take_requested_images`], decodes, and calls `provide_image`.
    requested_images: std::collections::HashSet<String>,
    /// Bumped whenever the image registry changes (provide/drop). Cached editor
    /// layouts include it so an image arriving recomputes line heights once.
    images_rev: u64,
    /// Encoded image bytes pasted into a multiline editor this frame, awaiting
    /// the host to upload + insert a link (drained via [`IMUI::take_pasted_image`]).
    pasted_image: Option<Vec<u8>>,
    /// GPU image textures awaiting deletion (freed during paint, where the
    /// renderer context is in a valid state).
    images_to_free: Vec<u32>,
    /// In-progress inline-image corner drag (stable start width + press point).
    image_drag: Option<ImageDrag>,
    /// A resize result `(link_key, new_size)` for the host to write back into
    /// the `?h=`/`?w=` of the image link this frame (via `take_image_resize`).
    image_resize_out: Option<(String, ImageResize)>,
    /// Inline-image keys the host has flagged as not-yet-synced (e.g. an upload
    /// that failed): a warning badge is drawn on the image. Set each frame via
    /// [`IMUI::mark_image_unsynced`]; cleared at `begin_frame`.
    image_unsynced: std::collections::HashSet<String>,
    /// Cross-thread repaint requests (see [`IMUI::repaint_waker`]). Checked
    /// once per event-loop iteration; cleared when honored.
    external_repaint: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Remote collaborator carets per textarea, set during build, drawn during
    /// paint, cleared at the next `begin_frame`.
    remote_carets: HashMap<UiKey, Vec<RemoteCaret>>,

    boxes: Vec<UIBox>,
    box_table: HashMap<UiKey, usize>,
    free_boxes: Vec<usize>,
    frame_boxes: Vec<usize>,
    /// Per-frame arena of deferred paint callbacks for `CUSTOM_DRAW` boxes.
    /// Cleared each frame in `begin_frame`; boxes hold indices into this.
    canvas_paints: Vec<CanvasPaint>,
    root: usize,
    overlay_root: usize,
    parent_stack: Vec<usize>,
    build_index: u64,
    render_continuously: bool,
    vsync_enabled: bool,
    cap_fps_to_refresh_rate: bool,
    refresh_rate_hz: f32,
    repaint_requested: bool,
    // GLX (and similar double-buffered backends) reallocate the window back
    // buffer lazily inside the buffer swap, so the first frame drawn after a
    // resize lands in the old, stale-sized buffer. Force a few extra redraws
    // after a resize so a correctly-sized frame gets presented even when the
    // app is otherwise idle (e.g. a single resize from a tiling WM).
    pending_resize_redraws: u32,
    quit_requested: bool,
    timer_frequency: f64,
    last_frame_time: f64,
    animation_dt: f32,
    fps_window_start: f64,
    fps_frame_count: u32,
    fps: f32,

    /// Live corner notifications, retained across frames (see `toast.rs`).
    toasts: Vec<toast::Toast>,
    next_toast_id: u64,

    pub theme: UITheme,

    /// Browser window/event bridge for the DOM render path, used when there is
    /// no `Drawer` (no GPU backend at all — see `imui/lifecycle.rs::new_dom`).
    #[cfg(feature = "dom")]
    wasm_window: Option<os::Window>,
    /// Backing slot for every `RepaintWaker` handed out on wasm32 (see
    /// [`TickScheduler`]) — shared, not per-waker, so all of them drive the
    /// exact same scheduling decision.
    #[cfg(target_arch = "wasm32")]
    tick_scheduler: TickScheduler,
    /// DOM reconciler for the web target (`imui/paint_dom.rs`); paints by
    /// walking `boxes` directly instead of going through `Drawer`.
    #[cfg(feature = "dom")]
    dom: Option<paint_dom::DomReconciler>,
}

trait RectAxis {
    fn rect_axis_min(&self, axis: Axis) -> f32;
}

impl RectAxis for UIBox {
    fn rect_axis_min(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.rect.x0,
            Axis::Y => self.rect.y0,
        }
    }
}

fn axis_idx(axis: Axis) -> usize {
    match axis {
        Axis::X => 0,
        Axis::Y => 1,
    }
}

fn flags_match(required: Option<OSEventFlag>, actual: Option<OSEventFlag>) -> bool {
    match (required, actual) {
        (Some(required), Some(actual)) => (required as u32) & (actual as u32) != 0,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

/// Frame-rate-independent smoothing factor for `rate` over `dt` seconds.
///
/// Feeds [`animate_scalar`]: `rate` is a per-second convergence speed (the
/// values in [`UIMotion`]), so the same motion plays identically at 60 and 144
/// Hz. Public so applications can drive their own animated values — a sliding
/// panel, a cross-fading view — with the framework's easing instead of
/// hand-rolling one; see [`IMUI::dt`].
pub fn smooth_rate(rate: f32, dt: f32) -> f32 {
    (1.0 - 2.0_f32.powf(-rate.max(0.0) * dt.max(0.0))).clamp(0.0, 1.0)
}

/// Step `current` toward `target` by `rate` (from [`smooth_rate`]), snapping
/// once the remaining distance falls under `epsilon` so the value settles
/// exactly instead of creeping forever. See [`UIMotion::epsilon`].
pub fn animate_scalar(current: f32, target: f32, rate: f32, epsilon: f32) -> f32 {
    let next = current + (target - current) * rate;
    if (target - next).abs() <= epsilon {
        target
    } else {
        next
    }
}

fn color_distance(a: Color, b: Color) -> f32 {
    (a.r - b.r)
        .abs()
        .max((a.g - b.g).abs())
        .max((a.b - b.b).abs())
        .max((a.a - b.a).abs())
}

fn has_flag(flags: Option<OSEventFlag>, flag: OSEventFlag) -> bool {
    flags
        .map(|flags| (flags as u32) & (flag as u32) != 0)
        .unwrap_or(false)
}

fn primary_modifier(flags: Option<OSEventFlag>) -> bool {
    has_flag(flags, OSEventFlag::command())
}

fn char_count(text: &str) -> usize {
    text.chars().count()
}

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

fn delete_char_range(text: &mut String, range: (usize, usize)) {
    let start = char_to_byte(text, range.0);
    let end = char_to_byte(text, range.1);
    if start <= end && end <= text.len() {
        text.replace_range(start..end, "");
    }
}

fn substring_chars(text: &str, range: (usize, usize)) -> String {
    text.chars()
        .skip(range.0)
        .take(range.1.saturating_sub(range.0))
        .collect()
}

fn selected_text(text: &str, range: Option<(usize, usize)>) -> Option<String> {
    range.map(|range| substring_chars(text, range))
}

/// Char index of the grapheme-cluster boundary adjacent to `cursor`. Moving by
/// grapheme (UAX #29) keeps the caret off the middle of combining sequences, ZWJ
/// emoji, flags, etc. `forward` picks the next boundary, otherwise the previous.
fn grapheme_boundary(text: &str, cursor: usize, forward: bool) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let total = char_count(text);
    let cursor = cursor.min(total);
    if forward {
        let mut boundary = 0usize;
        for g in text.graphemes(true) {
            boundary += g.chars().count();
            if boundary > cursor {
                return boundary;
            }
        }
        total
    } else {
        let mut prev = 0usize;
        for g in text.graphemes(true) {
            let next = prev + g.chars().count();
            if next >= cursor {
                return prev;
            }
            prev = next;
        }
        prev
    }
}

fn cursor_left(text: &str, cursor: usize) -> usize {
    grapheme_boundary(text, cursor, false)
}

fn cursor_right(text: &str, cursor: usize) -> usize {
    grapheme_boundary(text, cursor, true)
}

fn line_home(text: &str, cursor: usize) -> usize {
    let mut pos = cursor.min(char_count(text));
    let chars: Vec<char> = text.chars().collect();
    while pos > 0 && chars[pos - 1] != '\n' {
        pos -= 1;
    }
    pos
}

fn line_end(text: &str, cursor: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = cursor.min(chars.len());
    while pos < chars.len() && chars[pos] != '\n' {
        pos += 1;
    }
    pos
}

fn text_word_range(text: &str, cursor: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return (0, 0);
    }

    let len = chars.len();
    let cursor = cursor.min(len);
    let mut idx = cursor.min(len.saturating_sub(1));
    if cursor > 0 && cursor == len {
        idx = len - 1;
    } else if cursor > 0 && !is_word_char(chars[idx]) && is_word_char(chars[cursor - 1]) {
        idx = cursor - 1;
    }

    if is_word_char(chars[idx]) {
        let mut start = idx;
        while start > 0 && is_word_char(chars[start - 1]) {
            start -= 1;
        }
        let mut end = idx + 1;
        while end < len && is_word_char(chars[end]) {
            end += 1;
        }
        return (start, end);
    }

    let mut start = idx;
    while start > 0 && !is_word_char(chars[start - 1]) && chars[start - 1] != '\n' {
        start -= 1;
    }
    let mut end = idx + 1;
    while end < len && !is_word_char(chars[end]) && chars[end] != '\n' {
        end += 1;
    }
    (start, end)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn text_line_range(text: &str, cursor: usize) -> (usize, usize) {
    let len = char_count(text);
    if len == 0 {
        return (0, 0);
    }
    let cursor = cursor.min(len);
    (line_home(text, cursor), line_end(text, cursor))
}

pub fn u64_hash_from_string(seed: u64, string: &str) -> u64 {
    let mut hash: u64 = 5381 + seed;
    for byte in string.bytes() {
        hash = (hash << 5).wrapping_add(hash).wrapping_add(byte as u64);
    }
    hash
}

fn hash_part_from_key_string(string: &str) -> String {
    if let Some(idx) = string.find("###") {
        string[idx..].to_string()
    } else {
        string.to_string()
    }
}

fn display_part_from_key_string(string: &str) -> String {
    if let Some(idx) = string.find("##") {
        string[..idx].to_string()
    } else {
        string.to_string()
    }
}

fn point_in_rect(rect: &RectCoords, point: Option<Point>) -> bool {
    let Some(point) = point else {
        return false;
    };
    point.x >= rect.x0 && point.x <= rect.x1 && point.y >= rect.y0 && point.y <= rect.y1
}

fn expanded_rect(rect: RectCoords, padding: Padding) -> RectCoords {
    RectCoords::from_size(
        rect.x0 - padding.left,
        rect.y0 - padding.top,
        rect.width() + padding.horizontal(),
        rect.height() + padding.vertical(),
    )
}

fn intersect_rects(a: RectCoords, b: RectCoords) -> RectCoords {
    let x0 = a.x0.max(b.x0);
    let y0 = a.y0.max(b.y0);
    let x1 = a.x1.min(b.x1);
    let y1 = a.y1.min(b.y1);
    if x1 <= x0 || y1 <= y0 {
        RectCoords::from_size(x0, y0, 0.0, 0.0)
    } else {
        RectCoords { x0, y0, x1, y1 }
    }
}

fn rect_min_axis(rect: RectCoords, axis: Axis) -> f32 {
    match axis {
        Axis::X => rect.x0,
        Axis::Y => rect.y0,
    }
}

fn rect_size_axis(rect: RectCoords, axis: Axis) -> f32 {
    match axis {
        Axis::X => rect.width(),
        Axis::Y => rect.height(),
    }
}

fn scrollbar_thumb_len(track_len: f32, content_len: f32) -> f32 {
    track_len * (track_len / (content_len + track_len)).clamp(0.08, 1.0)
}

fn mouse_button_from_key(key: OSKey) -> Option<MouseButton> {
    match key {
        OSKey::LeftMouseButton => Some(MouseButton::Left),
        OSKey::RightMouseButton => Some(MouseButton::Right),
        _ => None,
    }
}

#[cfg(test)]
mod grapheme_nav_tests {
    use super::{cursor_left, cursor_right};

    // "e" + combining acute accent = 2 chars, 1 grapheme cluster.
    const COMBINED_E: &str = "e\u{0301}";
    // Regional-indicator pair = 2 chars, 1 grapheme (flag).
    const FLAG: &str = "\u{1F1EB}\u{1F1F7}";

    #[test]
    fn moves_over_whole_combining_cluster() {
        assert_eq!(cursor_right(COMBINED_E, 0), 2);
        assert_eq!(cursor_left(COMBINED_E, 2), 0);
    }

    #[test]
    fn moves_over_whole_flag_cluster() {
        assert_eq!(cursor_right(FLAG, 0), 2);
        assert_eq!(cursor_left(FLAG, 2), 0);
    }

    #[test]
    fn ascii_moves_one_char() {
        assert_eq!(cursor_right("ab", 0), 1);
        assert_eq!(cursor_left("ab", 1), 0);
    }

    #[test]
    fn clamps_at_bounds() {
        assert_eq!(cursor_left("ab", 0), 0);
        assert_eq!(cursor_right("ab", 2), 2);
    }
}
