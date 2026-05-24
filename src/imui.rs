use std::collections::HashMap;

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

use crate::{
    draw::Drawer,
    os::{self, OSCursor, OSEvent, OSEventFlag, OSEventType, OSKey, OSKeyCode},
    render::{self, RectCoords, V4f32},
};

pub mod uibox {
    pub use super::{
        Color, Padding, ThemeKind, UIBox, UIBoxFlags as UIBoxFlag, UIBoxHandle, UIBoxParams,
        UIBoxStyle, UITheme, UiSignal as UIBoxSignal, u64_hash_from_string,
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

#[derive(Clone, Copy, Debug, Default)]
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
    signal: UiSignal,
}

impl UIBoxHandle {
    pub fn key(&self) -> UiKey {
        self.key
    }

    pub fn signal(&self) -> UiSignal {
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

    pub fn border_color(self, ui: &mut IMUI, color: Color) -> Self {
        ui.border_color(self, color);
        self
    }

    pub fn padding_all(self, ui: &mut IMUI, value: f32) -> Self {
        ui.padding_all(self, value);
        self
    }

    pub fn gap(self, ui: &mut IMUI, value: f32) -> Self {
        ui.gap(self, value);
        self
    }

    pub fn corner_radius(self, ui: &mut IMUI, radius: f32) -> Self {
        ui.corner_radius(self, radius);
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
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct UiSignal {
    pub flags: u32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub left_press_pos: Option<Point>,
}

impl UiSignal {
    pub const LEFT_PRESSED: u32 = 1 << 0;
    pub const LEFT_DRAGGING: u32 = 1 << 1;
    pub const LEFT_RELEASED: u32 = 1 << 2;
    pub const LEFT_CLICKED: u32 = 1 << 3;
    pub const HOVERING: u32 = 1 << 4;
    pub const MOUSE_OVER: u32 = 1 << 5;
    pub const COMMIT: u32 = 1 << 6;

    pub fn pressed(self) -> bool {
        self.flags & Self::LEFT_PRESSED != 0
    }

    pub fn released(self) -> bool {
        self.flags & Self::LEFT_RELEASED != 0
    }

    pub fn clicked(self) -> bool {
        self.flags & Self::LEFT_CLICKED != 0
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEditState {
    pub cursor: usize,
    pub selection: Option<TextSelection>,
    pub desired_column: Option<usize>,
    pub last_interaction_time: f64,
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
    // Interaction capability flags. These are still box flags by design, like RAD:
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
            | Self::DRAW_BORDER.0
            | Self::SCROLL_Y.0
            | Self::CLIP.0,
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
pub struct TextAreaOptions {
    pub wrap_x: bool,
    pub scroll_x: bool,
    pub scroll_y: bool,
}

impl Default for TextAreaOptions {
    fn default() -> Self {
        Self {
            wrap_x: true,
            scroll_x: false,
            scroll_y: true,
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

#[derive(Clone, Debug)]
pub struct UIBox {
    pub key: UiKey,
    parent: Option<usize>,
    children: Vec<usize>,
    debug_label: Option<String>,
    string: Option<String>,
    display_string: Option<String>,
    flags: UIBoxFlags,
    pref_size: [UISize; 2],
    min_size: Size,
    scroll: Point,
    scroll_target: Point,
    scroll_max: Point,
    content_size: Size,
    fixed_position: Point,
    computed_size: Size,
    rect: RectCoords,
    cursor: Option<OSCursor>,
    hit_padding: Padding,
    child_layout_axis: Axis,
    padding: Padding,
    child_gap: f32,
    main_axis_align: MainAxisAlign,
    cross_axis_align: CrossAxisAlign,
    style: UIBoxStyle,
    bg_color_animated: Color,
    border_color_animated: Color,
    hot_t: f32,
    active_t: f32,
    focus_t: f32,
    appear_t: f32,
    scrollbar_x_t: f32,
    scrollbar_y_t: f32,
    // Per-frame interaction result for the retained box.
    signal: UiSignal,
    visible: bool,
    first_touched_frame: u64,
    last_touched_frame: u64,
}

impl UIBox {
    fn new(key: UiKey, flags: UIBoxFlags, string: Option<String>, theme: &UITheme) -> Self {
        Self {
            key,
            parent: None,
            children: Vec::new(),
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
            fixed_position: Point::default(),
            computed_size: Size::default(),
            rect: RectCoords::from_size(0.0, 0.0, 0.0, 0.0),
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
            bg_color_animated: theme.color_bg_popup,
            border_color_animated: theme.border,
            hot_t: 0.0,
            active_t: 0.0,
            focus_t: 0.0,
            appear_t: 0.0,
            scrollbar_x_t: 0.0,
            scrollbar_y_t: 0.0,
            signal: UiSignal::default(),
            visible: true,
            first_touched_frame: 0,
            last_touched_frame: 0,
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
        theme: &UITheme,
    ) {
        let rect = self.rect;
        let computed_size = self.computed_size;
        let scroll = self.scroll;
        let scroll_target = self.scroll_target;
        let scroll_max = self.scroll_max;
        let content_size = self.content_size;
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

        *self = Self::new(key, flags, string, theme);
        self.debug_label = self.display_string.clone();
        self.rect = rect;
        self.computed_size = computed_size;
        self.scroll = scroll;
        self.scroll_target = scroll_target;
        self.scroll_max = scroll_max;
        self.content_size = content_size;
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

pub struct IMUI {
    drawer: Option<Drawer>,
    size: Size,
    events: Vec<OSEvent>,
    mouse: Option<Point>,
    left_mouse_down: bool,
    right_mouse_down: bool,
    hot_key: Option<UiKey>,
    active_left_key: Option<UiKey>,
    active_right_key: Option<UiKey>,
    drag_start_mouse: Option<Point>,
    active_scrollbar: Option<ScrollbarDrag>,
    focus_key: Option<UiKey>,
    next_focus_key: Option<UiKey>,
    cursor: OSCursor,
    text_edit_states: HashMap<UiKey, TextEditState>,
    clipboard: String,

    boxes: Vec<UIBox>,
    box_table: HashMap<UiKey, usize>,
    free_boxes: Vec<usize>,
    frame_boxes: Vec<usize>,
    root: usize,
    overlay_root: usize,
    parent_stack: Vec<usize>,
    build_index: u64,
    render_continuously: bool,
    vsync_enabled: bool,
    repaint_requested: bool,
    timer_frequency: f64,
    last_frame_time: f64,
    animation_dt: f32,
    fps_window_start: f64,
    fps_frame_count: u32,
    fps: f32,

    pub theme: UITheme,
}

impl IMUI {
    #[cfg(not(target_os = "android"))]
    pub fn new(w: u32, h: u32) -> Self {
        let window = os::Window::new(w, h);
        Self::new_body(window)
    }

    #[cfg(target_os = "android")]
    pub fn android(app: AndroidApp) -> Self {
        let win = os::Window::new(app);
        win.wait_for_native_window();
        Self::new_body(win)
    }

    fn new_body(window: os::Window) -> Self {
        let renderer = render::Renderer::new(window);
        let drawer = Drawer::new(renderer);
        Self::with_drawer(Some(drawer), Size::default())
    }

    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn new_for_test(w: f32, h: f32) -> Self {
        Self::with_drawer(
            None,
            Size {
                width: w,
                height: h,
            },
        )
    }

    fn with_drawer(drawer: Option<Drawer>, size: Size) -> Self {
        let theme = UITheme::default();
        let mut ui = Self {
            drawer,
            size,
            events: Vec::new(),
            mouse: None,
            left_mouse_down: false,
            right_mouse_down: false,
            hot_key: None,
            active_left_key: None,
            active_right_key: None,
            drag_start_mouse: None,
            active_scrollbar: None,
            focus_key: None,
            next_focus_key: None,
            cursor: OSCursor::Arrow,
            text_edit_states: HashMap::new(),
            clipboard: String::new(),
            boxes: Vec::new(),
            box_table: HashMap::new(),
            free_boxes: Vec::new(),
            frame_boxes: Vec::new(),
            root: 0,
            overlay_root: 0,
            parent_stack: Vec::new(),
            build_index: 0,
            render_continuously: false,
            vsync_enabled: true,
            repaint_requested: true,
            timer_frequency: os::timer_init(),
            last_frame_time: 0.0,
            animation_dt: 1.0 / 60.0,
            fps_window_start: 0.0,
            fps_frame_count: 0,
            fps: 0.0,
            theme,
        };
        if let Some(drawer) = ui.drawer.as_mut() {
            drawer.renderer.vsync(ui.vsync_enabled);
        }
        let now = ui.now_seconds();
        ui.last_frame_time = now;
        ui.fps_window_start = now;
        ui.begin_frame();
        ui
    }

    pub fn eventloop(&mut self, mut build_ui_func: impl FnMut(&mut IMUI)) {
        loop {
            self.pull_consume_events();
            let had_events = !self.events.is_empty();
            let mut resized = false;
            if let Some(drawer) = self.drawer.as_mut() {
                let maybe_new_size = drawer.renderer.win.get_size();
                if maybe_new_size.0 != self.size.width || maybe_new_size.1 != self.size.height {
                    self.resize();
                    resized = true;
                }
            }

            if had_events || resized {
                self.repaint_requested = true;
            }

            if !self.render_continuously && !self.repaint_requested {
                std::thread::sleep(core::time::Duration::from_millis(16));
                continue;
            }

            self.repaint_requested = false;
            self.begin_frame();
            build_ui_func(self);
            self.end_frame();
            self.update_fps();
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        let now = self.now_seconds();
        self.animation_dt = (now - self.last_frame_time).clamp(1.0 / 240.0, 1.0 / 15.0) as f32;
        self.last_frame_time = now;
        self.build_index += 1;
        self.frame_boxes.clear();
        self.parent_stack.clear();
        self.cursor = OSCursor::Arrow;
        self.hot_key = None;
        self.focus_key = self.next_focus_key.take().or(self.focus_key);

        let root = self.alloc_box(Some("#root"), UIBoxFlags::NONE);
        self.root = root.idx;
        self.parent_stack.push(root.idx);
        self.boxes[root.idx].pref_size = [
            UISize::Pixels(self.size.width),
            UISize::Pixels(self.size.height),
        ];
        self.boxes[root.idx].computed_size = self.size;
        self.boxes[root.idx].rect =
            RectCoords::from_size(0.0, 0.0, self.size.width, self.size.height);
        self.boxes[root.idx].child_layout_axis = Axis::X;

        let overlay_root = self.alloc_box(Some("###overlay_root"), UIBoxFlags::NONE);
        self.overlay_root = overlay_root.idx;
        self.boxes[overlay_root.idx].flags |= UIBoxFlags::FLOATING_X | UIBoxFlags::FLOATING_Y;
        self.boxes[overlay_root.idx].fixed_position = Point::new(0.0, 0.0);
        self.boxes[overlay_root.idx].pref_size = [
            UISize::Pixels(self.size.width),
            UISize::Pixels(self.size.height),
        ];
        self.boxes[overlay_root.idx].computed_size = self.size;
        self.boxes[overlay_root.idx].rect =
            RectCoords::from_size(0.0, 0.0, self.size.width, self.size.height);
        self.boxes[overlay_root.idx].child_layout_axis = Axis::Y;
    }

    pub(crate) fn end_frame(&mut self) {
        self.animate_scroll_offsets();
        self.layout_root(self.root);
        self.refresh_passive_signals();
        self.animate_visual_state();
        self.draw_ui_all();

        if let Some(drawer) = self.drawer.as_mut() {
            drawer.renderer.render_frame();
            drawer.renderer.win.set_cursor(self.cursor);
        }

        self.prune_boxes();
    }

    #[cfg(feature = "testkit")]
    pub(crate) fn end_test_frame(&mut self) -> crate::testkit::UiSnapshot {
        self.animate_scroll_offsets();
        self.layout_root(self.root);
        self.refresh_passive_signals();
        self.animate_visual_state();
        self.draw_ui_all();
        let snapshot = self.snapshot();
        self.prune_boxes();
        snapshot
    }

    fn now_seconds(&self) -> f64 {
        os::timer_value() as f64 / self.timer_frequency
    }

    fn update_fps(&mut self) {
        self.fps_frame_count += 1;
        let now = self.now_seconds();
        let elapsed = now - self.fps_window_start;
        if elapsed >= 0.5 {
            self.fps = self.fps_frame_count as f32 / elapsed as f32;
            self.fps_frame_count = 0;
            self.fps_window_start = now;
        }
    }

    pub fn pull_consume_events(&mut self) {
        if let Some(drawer) = self.drawer.as_mut() {
            self.events = drawer.renderer.win.get_events();
        }

        for ev in self.events.clone() {
            self.apply_event_side_effects(&ev);
        }
    }

    fn apply_event_side_effects(&mut self, ev: &OSEvent) {
        if let Some(pos) = ev.pos {
            self.mouse = Some(pos);
        }
        if ev.key == OSKey::LeftMouseButton {
            match ev.ty {
                OSEventType::Press => self.left_mouse_down = true,
                OSEventType::Release => self.left_mouse_down = false,
                _ => {}
            }
        }
        if ev.key == OSKey::RightMouseButton {
            match ev.ty {
                OSEventType::Press => self.right_mouse_down = true,
                OSEventType::Release => self.right_mouse_down = false,
                _ => {}
            }
        }
        if ev.ty == OSEventType::Press && ev.key == OSKey::Keyboard(OSKeyCode::KeyEscape) {
            self.focus_key = None;
            self.next_focus_key = None;
        }
    }

    #[cfg(feature = "testkit")]
    pub(crate) fn push_test_event(&mut self, ev: OSEvent) {
        self.apply_event_side_effects(&ev);
        self.events.push(ev);
    }

    pub(crate) fn resize(&mut self) -> Size {
        if let Some(drawer) = self.drawer.as_mut() {
            self.size = Size::from(drawer.renderer.win.get_size());
            let render_size = drawer.renderer.win.get_render_size();
            drawer.renderer.resize(render_size.0, render_size.1);
        }
        self.size
    }

    pub fn input(&mut self, key: OSKey, flags: Option<OSEventFlag>) -> bool {
        let mut handled = false;
        self.events.retain(|ev| {
            if ev.ty == OSEventType::Press && ev.key == key && flags_match(flags, ev.flags) {
                handled = true;
                false
            } else {
                true
            }
        });
        handled
    }

    pub fn mouse_position(&self) -> Option<Point> {
        self.mouse
    }

    pub fn mouse_down(&self) -> bool {
        self.left_mouse_down
    }

    pub fn fps(&self) -> f32 {
        self.fps
    }

    pub fn render_continuously(&self) -> bool {
        self.render_continuously
    }

    pub fn set_render_continuously(&mut self, enabled: bool) {
        if self.render_continuously != enabled {
            self.render_continuously = enabled;
            self.request_repaint();
        }
    }

    pub fn renderer_backend(&self) -> render::Backend {
        self.drawer
            .as_ref()
            .map(|drawer| drawer.renderer.backend())
            .unwrap_or_else(render::Backend::default_backend)
    }

    pub fn set_renderer_backend(&mut self, backend: render::Backend) {
        if let Some(drawer) = self.drawer.as_mut() {
            drawer.renderer.set_backend(backend);
            drawer.renderer.vsync(self.vsync_enabled);
        }
        self.request_repaint();
    }

    pub fn vsync_enabled(&self) -> bool {
        self.vsync_enabled
    }

    pub fn set_vsync_enabled(&mut self, enabled: bool) {
        if self.vsync_enabled != enabled {
            self.vsync_enabled = enabled;
            if let Some(drawer) = self.drawer.as_mut() {
                drawer.renderer.vsync(enabled);
            }
            self.request_repaint();
        }
    }

    pub fn request_repaint(&mut self) {
        self.repaint_requested = true;
    }

    pub fn theme(&self) -> &UITheme {
        &self.theme
    }

    pub fn set_theme(&mut self, theme: UITheme) {
        let changed = self.theme.kind != theme.kind;
        self.theme = theme;
        if changed {
            self.request_repaint();
        }
    }

    pub fn reset_text_input_state(&mut self) {
        self.focus_key = None;
        self.next_focus_key = None;
    }

    pub fn set_focus_active(&mut self, id: &str) {
        let seed = self.boxes.get(self.root).map(|b| b.key).unwrap_or_default();
        self.next_focus_key = Some(UiKey(u64_hash_from_string(seed.0, id)));
    }

    pub fn bounds(&self, handle: UIBoxHandle) -> RectCoords {
        self.boxes
            .get(handle.idx)
            .map(|b| b.rect)
            .unwrap_or_else(|| RectCoords::from_size(0.0, 0.0, 0.0, 0.0))
    }

    #[cfg(feature = "testkit")]
    pub fn snapshot(&self) -> crate::testkit::UiSnapshot {
        let nodes = self
            .frame_boxes
            .iter()
            .filter_map(|idx| self.boxes.get(*idx).map(|node| (*idx, node)))
            .map(|(idx, node)| crate::testkit::UiNodeSnapshot {
                key: node.key,
                label: node.debug_label.clone(),
                text: node.string.clone(),
                bounds: node.rect,
                computed_size: node.computed_size,
                scroll: node.scroll,
                scroll_max: node.scroll_max,
                content_size: node.content_size,
                clip_rect: self.clipped_rect(idx),
                signal: node.signal,
                visible: node.visible,
                focused: self.focus_key == Some(node.key),
                mouse_clickable: node.flags.is_mouse_clickable(),
                text_input: node.flags.accepts_text_input(),
                scroll_x: node.flags.scrolls_x(),
                scroll_y: node.flags.scrolls_y(),
                text_edit: self.text_edit_states.get(&node.key).cloned(),
            })
            .collect();
        crate::testkit::UiSnapshot { nodes }
    }

    pub fn row(&mut self, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(None, Axis::X, UIBoxFlags::NONE, children)
    }

    pub fn column(&mut self, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(None, Axis::Y, UIBoxFlags::NONE, children)
    }

    pub fn named_row(&mut self, id: &str, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(Some(id), Axis::X, UIBoxFlags::NONE, children)
    }

    pub fn named_column(&mut self, id: &str, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(Some(id), Axis::Y, UIBoxFlags::NONE, children)
    }

    fn container(
        &mut self,
        label: Option<&str>,
        axis: Axis,
        flags: UIBoxFlags,
        children: impl FnOnce(&mut IMUI),
    ) -> UIBoxHandle {
        let handle = self.alloc_box(label, flags);
        self.boxes[handle.idx].child_layout_axis = axis;
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::ChildrenSum];
        if axis == Axis::X {
            self.boxes[handle.idx].pref_size = [UISize::ChildrenSum, UISize::ParentPct(1.0)];
        }
        self.parent_stack.push(handle.idx);
        children(self);
        self.parent_stack.pop();
        handle
    }

    pub fn label(&mut self, label: &str) -> UIBoxHandle {
        let handle = self.alloc_box(None, UIBoxFlags::DRAW_TEXT);
        self.boxes[handle.idx].string = Some(label.to_string());
        self.boxes[handle.idx].display_string = Some(label.to_string());
        self.boxes[handle.idx].pref_size = [UISize::TextContent(0.0), UISize::TextContent(0.0)];
        handle
    }

    pub fn button(&mut self, label: &str, tooltip_text: Option<&str>) -> UIBoxHandle {
        let handle = self.alloc_box(Some(label), UIBoxFlags::BUTTON);
        self.configure_button_box(handle);
        self.show_tooltip_for_hover(handle, tooltip_text);
        handle
    }

    pub fn button_icon(&mut self, label: &str, tooltip_text: Option<&str>) -> UIBoxHandle {
        let handle = self.button(label, tooltip_text);
        self.boxes[handle.idx].style.font_icon = true;
        self.boxes[handle.idx].style.font_size = 24.0;
        self.width(handle, UISize::Pixels(32.0));
        self.height(handle, UISize::Pixels(32.0));
        handle
    }

    pub fn button_icon_plain(&mut self, label: &str, tooltip_text: Option<&str>) -> UIBoxHandle {
        let handle = self.alloc_box(Some(label), UIBoxFlags::CLICKABLE | UIBoxFlags::DRAW_TEXT);
        self.boxes[handle.idx].style.font_icon = true;
        self.boxes[handle.idx].style.font_size = 24.0;
        self.boxes[handle.idx].style.text_color = if handle.dragging() || handle.pressed() {
            self.theme.accent_active
        } else if handle.hover() {
            self.theme.accent_hover
        } else {
            self.theme.text_muted
        };
        self.boxes[handle.idx].padding = Padding::all(2.0);
        self.width(handle, UISize::Pixels(32.0));
        self.height(handle, UISize::Pixels(32.0));
        self.show_tooltip_for_hover(handle, tooltip_text);
        handle
    }

    pub fn line_edit(&mut self, id: &str, buffer: &mut String, masked: bool) -> UIBoxHandle {
        let handle = self.alloc_box(Some(id), UIBoxFlags::LINE_EDIT);
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::Pixels(32.0)];
        self.boxes[handle.idx].padding = Padding::all(7.0);
        self.boxes[handle.idx].style.bg_color = self.theme.input_bg;
        self.boxes[handle.idx].style.border_color = self.theme.border;
        self.boxes[handle.idx].style.corner_radius = self.theme.radius;
        self.apply_click_to_focus(handle);
        self.apply_line_edit_mouse_selection(handle, buffer);
        if self.box_is_focused(handle) {
            self.apply_text_input(handle, buffer, false);
            self.boxes[handle.idx].style.border_color = self.theme.accent;
        }
        self.set_edit_display_text(handle, buffer, masked);
        handle
    }

    pub fn textarea(&mut self, id: &str, buffer: &mut String) -> UIBoxHandle {
        self.textarea_with_options(id, buffer, TextAreaOptions::default())
    }

    pub fn textarea_with_options(
        &mut self,
        id: &str,
        buffer: &mut String,
        options: TextAreaOptions,
    ) -> UIBoxHandle {
        let mut flags = UIBoxFlags::MOUSE_CLICKABLE
            | UIBoxFlags::CLICK_TO_FOCUS
            | UIBoxFlags::TEXT_INPUT
            | UIBoxFlags::DRAW_BACKGROUND
            | UIBoxFlags::DRAW_BORDER
            | UIBoxFlags::CLIP;
        if options.scroll_x {
            flags |= UIBoxFlags::SCROLL_X;
        }
        if options.scroll_y {
            flags |= UIBoxFlags::SCROLL_Y;
        }
        let handle = self.alloc_box(Some(id), flags);
        self.boxes[handle.idx].child_layout_axis = Axis::Y;
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::ParentPct(1.0)];
        self.boxes[handle.idx].padding = Padding::all(10.0);
        self.boxes[handle.idx].style.bg_color = self.theme.input_bg;
        self.boxes[handle.idx].style.border_color = self.theme.border;
        self.boxes[handle.idx].style.corner_radius = self.theme.radius;
        self.boxes[handle.idx].child_gap = 2.0;
        self.apply_click_to_focus(handle);
        self.apply_textarea_mouse_selection(handle, buffer);
        if self.box_is_focused(handle) {
            self.apply_text_input(handle, buffer, true);
            self.boxes[handle.idx].style.border_color = self.theme.accent;
        }
        self.boxes[handle.idx].string = Some(buffer.clone());

        let content_width = (self.boxes[handle.idx].rect.x1
            - self.boxes[handle.idx].rect.x0
            - self.boxes[handle.idx].padding.horizontal()
            - self.boxes[handle.idx].style.margin * 2.0)
            .max(0.0);

        self.parent_stack.push(handle.idx);
        let wrapped_lines = if options.wrap_x {
            self.wrap_text_lines(
                buffer,
                content_width,
                self.boxes[handle.idx].style.font_size,
            )
        } else {
            buffer.lines().map(str::to_string).collect()
        };
        if wrapped_lines.is_empty() {
            let empty = self.label("");
            self.height(empty, UISize::Pixels(self.theme.size_text + 4.0));
        } else {
            for (idx, line) in wrapped_lines.iter().enumerate() {
                let line_id = format!("{line}###textarea_line_{idx}");
                let row = self.alloc_box(Some(&line_id), UIBoxFlags::DRAW_TEXT);
                self.boxes[row.idx].string = Some(line.clone());
                self.boxes[row.idx].display_string = Some(line.clone());
                self.boxes[row.idx].pref_size =
                    [UISize::TextContent(0.0), UISize::TextContent(0.0)];
                self.height(row, UISize::Pixels(self.theme.size_text + 4.0));
            }
        }
        self.parent_stack.pop();
        handle
    }

    fn wrap_text_lines(&mut self, text: &str, max_width: f32, font_size: f32) -> Vec<String> {
        if max_width <= 0.0 {
            return text.lines().map(str::to_string).collect();
        }
        let mut out = Vec::new();
        for raw_line in text.lines() {
            if raw_line.is_empty() {
                out.push(String::new());
                continue;
            }
            let mut current = String::new();
            for ch in raw_line.chars() {
                let mut next = current.clone();
                next.push(ch);
                let w = self.text_size(font_size, &next).0;
                if !current.is_empty() && w > max_width {
                    out.push(current);
                    current = ch.to_string();
                } else {
                    current = next;
                }
            }
            out.push(current);
        }
        out
    }

    fn compute_visual_line_ranges(
        &mut self,
        text: &str,
        max_width: f32,
        font_size: f32,
    ) -> Vec<(usize, usize)> {
        if text.is_empty() {
            return vec![(0, 0)];
        }
        let mut ranges = Vec::new();
        let mut line_char_start = 0;

        for raw_line in text.lines() {
            let raw_line_len = raw_line.chars().count();
            if raw_line_len == 0 {
                ranges.push((line_char_start, line_char_start));
                line_char_start += 1;
                continue;
            }
            if max_width <= 0.0 {
                ranges.push((line_char_start, line_char_start + raw_line_len));
                line_char_start += raw_line_len + 1;
                continue;
            }
            let mut current_start = line_char_start;
            let mut current = String::new();
            for ch in raw_line.chars() {
                let mut next = current.clone();
                next.push(ch);
                let w = self.text_size(font_size, &next).0;
                if !current.is_empty() && w > max_width {
                    ranges.push((current_start, current_start + current.chars().count()));
                    current_start = line_char_start + current.chars().count();
                    current = ch.to_string();
                } else {
                    current = next;
                }
            }
            ranges.push((current_start, current_start + current.chars().count()));
            line_char_start += raw_line_len + 1;
        }
        ranges
    }

    fn visual_line_col_from_cursor_with_ranges(
        &self,
        ranges: &[(usize, usize)],
        cursor: usize,
    ) -> (usize, usize) {
        for (line_idx, &(start, end)) in ranges.iter().enumerate() {
            if start == end && cursor == start {
                return (line_idx, 0);
            }
            if cursor >= start && cursor < end {
                return (line_idx, cursor - start);
            }
            if cursor == end {
                return (line_idx, end - start);
            }
        }
        if let Some(&(start, end)) = ranges.last() {
            let col = cursor.saturating_sub(start);
            if end > start {
                (ranges.len() - 1, col.min(end - start))
            } else {
                (ranges.len() - 1, 0)
            }
        } else {
            (0, 0)
        }
    }

    fn cursor_from_visual_line_col_with_ranges(
        &self,
        ranges: &[(usize, usize)],
        visual_line: usize,
        col: usize,
    ) -> usize {
        if visual_line >= ranges.len() {
            ranges.last().map(|&(_, end)| end).unwrap_or(0)
        } else {
            let (start, end) = ranges[visual_line];
            (start + col).min(end)
        }
    }

    pub fn floating_pane_at(
        &mut self,
        pos: Point,
        id: Option<&str>,
        children: impl FnOnce(&mut IMUI),
    ) -> UIBoxHandle {
        self.parent_stack.push(self.overlay_root);
        let handle = self.alloc_box(id, UIBoxFlags::DRAW_BACKGROUND | UIBoxFlags::DRAW_BORDER);
        self.boxes[handle.idx].fixed_position = pos;
        self.boxes[handle.idx].flags |= UIBoxFlags::FLOATING_X | UIBoxFlags::FLOATING_Y;
        self.boxes[handle.idx].child_layout_axis = Axis::Y;
        self.boxes[handle.idx].pref_size = [UISize::ChildrenSum, UISize::ChildrenSum];
        self.parent_stack.push(handle.idx);
        children(self);
        self.parent_stack.pop();
        self.parent_stack.pop();
        handle
    }

    fn width(&mut self, handle: UIBoxHandle, width: UISize) -> &mut Self {
        self.boxes[handle.idx].pref_size[axis_idx(Axis::X)] = width;
        self
    }

    fn height(&mut self, handle: UIBoxHandle, height: UISize) -> &mut Self {
        self.boxes[handle.idx].pref_size[axis_idx(Axis::Y)] = height;
        self
    }

    fn min_width(&mut self, handle: UIBoxHandle, width: f32) -> &mut Self {
        self.boxes[handle.idx].min_size.width = width;
        self
    }

    fn min_height(&mut self, handle: UIBoxHandle, height: f32) -> &mut Self {
        self.boxes[handle.idx].min_size.height = height;
        self
    }

    fn background(&mut self, handle: UIBoxHandle, color: Color) -> &mut Self {
        self.boxes[handle.idx].flags |= UIBoxFlags::DRAW_BACKGROUND;
        self.boxes[handle.idx].style.bg_color = color;
        self
    }

    fn text_color(&mut self, handle: UIBoxHandle, color: Color) -> &mut Self {
        self.boxes[handle.idx].style.text_color = color;
        self
    }

    fn border_color(&mut self, handle: UIBoxHandle, color: Color) -> &mut Self {
        self.boxes[handle.idx].flags |= UIBoxFlags::DRAW_BORDER;
        self.boxes[handle.idx].style.border_color = color;
        self
    }

    fn corner_radius(&mut self, handle: UIBoxHandle, radius: f32) -> &mut Self {
        self.boxes[handle.idx].style.corner_radius = radius.max(0.0);
        self
    }

    fn cursor(&mut self, handle: UIBoxHandle, cursor: OSCursor) -> &mut Self {
        self.boxes[handle.idx].cursor = Some(cursor);
        self
    }

    fn hit_padding_x(&mut self, handle: UIBoxHandle, value: f32) -> &mut Self {
        let value = value.max(0.0);
        self.boxes[handle.idx].hit_padding.left = value;
        self.boxes[handle.idx].hit_padding.right = value;
        self
    }

    fn padding_all(&mut self, handle: UIBoxHandle, value: f32) -> &mut Self {
        self.boxes[handle.idx].padding = Padding::all(value);
        self
    }

    fn gap(&mut self, handle: UIBoxHandle, value: f32) -> &mut Self {
        self.boxes[handle.idx].child_gap = value;
        self
    }

    fn scroll_x(&mut self, handle: UIBoxHandle, enabled: bool) -> &mut Self {
        if enabled {
            self.boxes[handle.idx].flags |= UIBoxFlags::SCROLL_X;
            self.absorb_pending_scroll_for_box(handle.idx);
            let key = self.boxes[handle.idx].key;
            let flags = self.boxes[handle.idx].flags;
            self.apply_scrollbar_events(handle.idx, key, flags);
        } else {
            self.boxes[handle.idx].flags.0 &= !UIBoxFlags::SCROLL_X.0;
        }
        self
    }

    fn scroll_y(&mut self, handle: UIBoxHandle, enabled: bool) -> &mut Self {
        if enabled {
            self.boxes[handle.idx].flags |= UIBoxFlags::SCROLL_Y;
            self.absorb_pending_scroll_for_box(handle.idx);
            let key = self.boxes[handle.idx].key;
            let flags = self.boxes[handle.idx].flags;
            self.apply_scrollbar_events(handle.idx, key, flags);
        } else {
            self.boxes[handle.idx].flags.0 &= !UIBoxFlags::SCROLL_Y.0;
        }
        self
    }

    fn absorb_pending_scroll_for_box(&mut self, idx: usize) {
        let flags = self.boxes[idx].flags;
        if !flags.scrolls_x() && !flags.scrolls_y() {
            return;
        }

        let rect = self.boxes[idx].rect;
        let mut signal = UiSignal::default();
        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            if ev.ty != OSEventType::Scroll || !point_in_rect(&rect, ev.pos.or(self.mouse)) {
                ev_idx += 1;
                continue;
            }

            let mut taken = false;
            if flags.scrolls_y() {
                signal.scroll_y += ev.delta;
                taken = true;
            }
            if flags.scrolls_x() {
                signal.scroll_x += ev.delta;
                taken = true;
            }

            if taken {
                self.events.remove(ev_idx);
            } else {
                ev_idx += 1;
            }
        }

        if signal.scroll_x != 0.0 || signal.scroll_y != 0.0 {
            self.boxes[idx].signal.scroll_x += signal.scroll_x;
            self.boxes[idx].signal.scroll_y += signal.scroll_y;
            self.apply_scroll_signal(idx);
        }
    }

    fn clip(&mut self, handle: UIBoxHandle, enabled: bool) -> &mut Self {
        if enabled {
            self.boxes[handle.idx].flags |= UIBoxFlags::CLIP;
        } else {
            self.boxes[handle.idx].flags.0 &= !UIBoxFlags::CLIP.0;
        }
        self
    }

    fn align(
        &mut self,
        handle: UIBoxHandle,
        main: MainAxisAlign,
        cross: CrossAxisAlign,
    ) -> &mut Self {
        self.boxes[handle.idx].main_axis_align = main;
        self.boxes[handle.idx].cross_axis_align = cross;
        self
    }

    fn configure_button_box(&mut self, handle: UIBoxHandle) {
        self.boxes[handle.idx].pref_size = [UISize::TextContent(16.0), UISize::TextContent(10.0)];
        self.boxes[handle.idx].padding = Padding {
            top: 5.0,
            right: 8.0,
            bottom: 5.0,
            left: 8.0,
        };
        self.boxes[handle.idx].style.bg_color = self.theme.surface_bg;
        self.boxes[handle.idx].style.border_color = self.theme.border;
        self.boxes[handle.idx].style.corner_radius = self.theme.radius;
    }

    fn show_tooltip_for_hover(&mut self, handle: UIBoxHandle, tooltip_text: Option<&str>) {
        let (Some(text), Some(mouse)) = (tooltip_text, self.mouse) else {
            return;
        };
        if !handle.hover() {
            return;
        }

        let tooltip = self.floating_pane_at(
            Point::new(mouse.x + 12.0, mouse.y + 12.0),
            Some("#tooltip"),
            |ui| {
                let label = ui.label(text);
                ui.padding_all(label, 5.0);
            },
        );
        self.background(tooltip, self.theme.popover_bg);
        self.border_color(tooltip, self.theme.border);
        self.corner_radius(tooltip, self.theme.radius);
        self.padding_all(tooltip, 4.0);
    }

    fn apply_click_to_focus(&mut self, handle: UIBoxHandle) {
        if self.boxes[handle.idx].flags.click_to_focus() && (handle.pressed() || handle.clicked()) {
            self.focus_key = Some(handle.key);
        }
    }

    fn box_is_focused(&self, handle: UIBoxHandle) -> bool {
        self.focus_key == Some(handle.key)
    }

    fn set_edit_display_text(&mut self, handle: UIBoxHandle, buffer: &str, masked: bool) {
        let display = if masked {
            "*".repeat(buffer.chars().count())
        } else {
            buffer.to_string()
        };
        self.boxes[handle.idx].string = Some(display.clone());
        self.boxes[handle.idx].display_string = Some(display);
    }

    fn apply_text_input(&mut self, handle: UIBoxHandle, buffer: &mut String, multiline: bool) {
        let key = handle.key;
        self.ensure_text_state(key, buffer);
        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            if ev.ty != OSEventType::Press {
                ev_idx += 1;
                continue;
            }
            let taken = self.apply_text_event(key, buffer, multiline, ev);
            if taken {
                self.events.remove(ev_idx);
            } else {
                ev_idx += 1;
            }
        }
    }

    fn ensure_text_state(&mut self, key: UiKey, buffer: &str) {
        let len = char_count(buffer);
        let state = self.text_edit_states.entry(key).or_default();
        state.cursor = state.cursor.min(len);
        if let Some(selection) = state.selection.as_mut() {
            selection.anchor = selection.anchor.min(len);
            selection.cursor = selection.cursor.min(len);
            if selection.anchor == selection.cursor {
                state.selection = None;
            }
        }
    }

    fn apply_text_event(
        &mut self,
        key: UiKey,
        buffer: &mut String,
        multiline: bool,
        ev: OSEvent,
    ) -> bool {
        let OSKey::Keyboard(key_code) = ev.key else {
            return false;
        };
        let shift = has_flag(ev.flags, OSEventFlag::Shift);
        let primary = primary_modifier(ev.flags);

        if primary {
            match key_code {
                OSKeyCode::KeyA => {
                    let len = char_count(buffer);
                    let state = self.text_edit_states.entry(key).or_default();
                    state.cursor = len;
                    state.selection = Some(TextSelection {
                        anchor: 0,
                        cursor: len,
                    });
                    self.reset_caret_blink(key);
                    return true;
                }
                OSKeyCode::KeyC => {
                    if let Some(text) = selected_text(
                        buffer,
                        self.text_edit_states
                            .get(&key)
                            .and_then(TextEditState::selection_range),
                    ) {
                        self.clipboard = text;
                    }
                    return true;
                }
                OSKeyCode::KeyX => {
                    let range = self
                        .text_edit_states
                        .get(&key)
                        .and_then(TextEditState::selection_range);
                    if let Some(text) = selected_text(buffer, range) {
                        self.clipboard = text;
                        delete_char_range(buffer, range.unwrap());
                        let state = self.text_edit_states.entry(key).or_default();
                        state.cursor = range.unwrap().0;
                        state.clear_selection();
                    }
                    self.reset_caret_blink(key);
                    return true;
                }
                OSKeyCode::KeyV => {
                    let text = self.clipboard.clone();
                    self.replace_selection_or_insert(key, buffer, &text);
                    self.reset_caret_blink(key);
                    return true;
                }
                _ => {}
            }
        }

        match key_code {
            OSKeyCode::KeyBackspace => {
                if !self.delete_selection(key, buffer) {
                    let state = self.text_edit_states.entry(key).or_default();
                    if state.cursor > 0 {
                        let pos = state.cursor;
                        delete_char_range(buffer, (pos - 1, pos));
                        state.cursor -= 1;
                    }
                }
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyDelete => {
                if !self.delete_selection(key, buffer) {
                    let state = self.text_edit_states.entry(key).or_default();
                    let len = char_count(buffer);
                    if state.cursor < len {
                        let pos = state.cursor;
                        delete_char_range(buffer, (pos, pos + 1));
                    }
                }
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyEnter if multiline => {
                self.replace_selection_or_insert(key, buffer, "\n");
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyEscape => {
                self.focus_key = None;
                true
            }
            OSKeyCode::KeyLeftArrow => {
                self.move_text_cursor(
                    key,
                    buffer,
                    cursor_left(buffer, self.text_cursor(key)),
                    shift,
                );
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyRightArrow => {
                self.move_text_cursor(
                    key,
                    buffer,
                    cursor_right(buffer, self.text_cursor(key)),
                    shift,
                );
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyHome => {
                self.move_text_cursor(key, buffer, line_home(buffer, self.text_cursor(key)), shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyEnd => {
                self.move_text_cursor(key, buffer, line_end(buffer, self.text_cursor(key)), shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyUpArrow if multiline => {
                self.move_vertical(key, buffer, -1, shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyDownArrow if multiline => {
                self.move_vertical(key, buffer, 1, shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyPageUp => {
                self.move_text_cursor(key, buffer, 0, shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyPageDown => {
                self.move_text_cursor(key, buffer, char_count(buffer), shift);
                self.reset_caret_blink(key);
                true
            }
            _ => {
                if let Some(c) = ev.chars {
                    if !c.is_ascii_control() {
                        let mut s = String::new();
                        s.push(c);
                        self.replace_selection_or_insert(key, buffer, &s);
                        self.reset_caret_blink(key);
                        return true;
                    }
                }
                false
            }
        }
    }

    fn text_cursor(&self, key: UiKey) -> usize {
        self.text_edit_states
            .get(&key)
            .map(|state| state.cursor)
            .unwrap_or(0)
    }

    fn reset_caret_blink(&mut self, key: UiKey) {
        let now = self.now_seconds();
        let state = self.text_edit_states.entry(key).or_default();
        state.last_interaction_time = now;
    }

    fn set_text_cursor(&mut self, key: UiKey, cursor: usize, extend_selection: bool) {
        let state = self.text_edit_states.entry(key).or_default();
        if extend_selection {
            let anchor = state.selection.map(|s| s.anchor).unwrap_or(state.cursor);
            state.selection = Some(TextSelection { anchor, cursor });
        } else {
            state.selection = None;
        }
        state.cursor = cursor;
        state.desired_column = None;
    }

    fn move_text_cursor(
        &mut self,
        key: UiKey,
        buffer: &str,
        cursor: usize,
        extend_selection: bool,
    ) {
        self.set_text_cursor(key, cursor.min(char_count(buffer)), extend_selection);
    }

    fn move_vertical(&mut self, key: UiKey, buffer: &str, delta: isize, extend_selection: bool) {
        let cursor = self.text_cursor(key);
        let Some(idx) = self.box_from_key(key) else {
            return;
        };
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let style = self.boxes[idx].style;
        let content_width =
            (rect.x1 - rect.x0 - padding.horizontal() - style.margin * 2.0).max(0.0);
        let ranges = self.compute_visual_line_ranges(buffer, content_width, style.font_size);
        let (visual_line, col) = self.visual_line_col_from_cursor_with_ranges(&ranges, cursor);
        let state = self.text_edit_states.entry(key).or_default();
        let desired_col = state.desired_column.unwrap_or(col);
        state.desired_column = Some(desired_col);
        let line_count = ranges.len().max(1);
        let next_line = (visual_line as isize + delta).clamp(0, line_count as isize - 1) as usize;
        let next_cursor =
            self.cursor_from_visual_line_col_with_ranges(&ranges, next_line, desired_col);
        self.move_text_cursor(key, buffer, next_cursor, extend_selection);
        if let Some(state) = self.text_edit_states.get_mut(&key) {
            state.desired_column = Some(desired_col);
        }
    }

    fn replace_selection_or_insert(&mut self, key: UiKey, buffer: &mut String, text: &str) {
        self.delete_selection(key, buffer);
        let state = self.text_edit_states.entry(key).or_default();
        let cursor = state.cursor.min(char_count(buffer));
        let byte = char_to_byte(buffer, cursor);
        buffer.insert_str(byte, text);
        state.cursor = cursor + char_count(text);
        state.clear_selection();
        state.desired_column = None;
    }

    fn delete_selection(&mut self, key: UiKey, buffer: &mut String) -> bool {
        let range = self
            .text_edit_states
            .get(&key)
            .and_then(TextEditState::selection_range);
        let Some(range) = range else {
            return false;
        };
        delete_char_range(buffer, range);
        let state = self.text_edit_states.entry(key).or_default();
        state.cursor = range.0;
        state.clear_selection();
        state.desired_column = None;
        true
    }

    fn apply_line_edit_mouse_selection(&mut self, handle: UIBoxHandle, buffer: &str) {
        if handle.pressed() {
            let cursor =
                self.cursor_from_line_edit_point(handle, buffer, handle.signal.left_press_pos);
            self.set_text_cursor(handle.key, cursor, false);
            self.reset_caret_blink(handle.key);
        }
        if handle.dragging() {
            let cursor = self.cursor_from_line_edit_point(handle, buffer, self.mouse);
            self.set_text_cursor(handle.key, cursor, true);
            self.reset_caret_blink(handle.key);
        }
    }

    fn apply_textarea_mouse_selection(&mut self, handle: UIBoxHandle, buffer: &str) {
        if handle.pressed() {
            let cursor =
                self.cursor_from_textarea_point(handle, buffer, handle.signal.left_press_pos);
            self.set_text_cursor(handle.key, cursor, false);
            self.reset_caret_blink(handle.key);
        }
        if handle.dragging() {
            let cursor = self.cursor_from_textarea_point(handle, buffer, self.mouse);
            self.set_text_cursor(handle.key, cursor, true);
            self.reset_caret_blink(handle.key);
        }
    }

    fn cursor_from_line_edit_point(
        &mut self,
        handle: UIBoxHandle,
        buffer: &str,
        point: Option<Point>,
    ) -> usize {
        let Some(point) = point else {
            return char_count(buffer);
        };
        let rect = self.boxes[handle.idx].rect;
        let padding = self.boxes[handle.idx].padding;
        let style = self.boxes[handle.idx].style;
        let local_x = (point.x - rect.x0 - padding.left - style.margin).max(0.0);
        self.cursor_from_x(buffer, style.font_size, local_x)
    }

    fn cursor_from_textarea_point(
        &mut self,
        handle: UIBoxHandle,
        buffer: &str,
        point: Option<Point>,
    ) -> usize {
        let Some(point) = point else {
            return char_count(buffer);
        };
        let rect = self.boxes[handle.idx].rect;
        let padding = self.boxes[handle.idx].padding;
        let style = self.boxes[handle.idx].style;
        let line_h = self.theme.size_text + 6.0;
        let visual_line = ((point.y - rect.y0 - padding.top + self.boxes[handle.idx].scroll.y)
            / line_h)
            .floor()
            .max(0.0) as usize;
        let content_width =
            (rect.x1 - rect.x0 - padding.horizontal() - style.margin * 2.0).max(0.0);
        let ranges = self.compute_visual_line_ranges(buffer, content_width, style.font_size);
        let (line_start, line_end) = if visual_line < ranges.len() {
            ranges[visual_line]
        } else {
            ranges.last().copied().unwrap_or((0, char_count(buffer)))
        };
        let line_text = substring_chars(buffer, (line_start, line_end));
        let local_x = (point.x - rect.x0 - padding.left - style.margin).max(0.0);
        line_start + self.cursor_from_x(&line_text, style.font_size, local_x)
    }

    fn cursor_from_x(&mut self, text: &str, font_size: f32, x: f32) -> usize {
        let mut last = 0;
        for idx in 0..=char_count(text) {
            let prefix = substring_chars(text, (0, idx));
            let width = self.text_size(font_size, &prefix).0;
            if width > x {
                return if idx == 0 { 0 } else { last };
            }
            last = idx;
        }
        last
    }

    fn box_from_key(&self, key: UiKey) -> Option<usize> {
        if key.is_zero() {
            None
        } else {
            self.box_table.get(&key).copied()
        }
    }

    fn allocate_box_storage(
        &mut self,
        key: UiKey,
        flags: UIBoxFlags,
        display_string: Option<String>,
    ) -> usize {
        if let Some(idx) = self.free_boxes.pop() {
            self.boxes[idx] = UIBox::new(key, flags, display_string, &self.theme);
            idx
        } else {
            let idx = self.boxes.len();
            self.boxes
                .push(UIBox::new(key, flags, display_string, &self.theme));
            idx
        }
    }

    fn alloc_box(&mut self, label: Option<&str>, flags: UIBoxFlags) -> UIBoxHandle {
        let parent_idx = self.parent_stack.last().copied();
        let seed = parent_idx.map(|idx| self.boxes[idx].key.0).unwrap_or(0);
        let key_string = label.map(hash_part_from_key_string);
        let mut key = key_string
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| UiKey(u64_hash_from_string(seed, s)))
            .unwrap_or_default();
        let display_string = label
            .map(display_part_from_key_string)
            .filter(|s| !s.is_empty());
        let mut existing_idx = self.box_from_key(key);
        if let Some(idx) = existing_idx {
            if self.boxes[idx].last_touched_frame == self.build_index {
                key = UiKey::default();
                existing_idx = None;
            }
        }

        let signal = self.signal_from_key_and_flags(key, flags, existing_idx);
        let idx = if let Some(idx) = existing_idx {
            self.boxes[idx].reset_for_frame(key, flags, display_string.clone(), &self.theme);
            idx
        } else {
            self.allocate_box_storage(key, flags, display_string.clone())
        };

        if !key.is_zero() && existing_idx.is_none() {
            self.box_table.insert(key, idx);
            self.boxes[idx].first_touched_frame = self.build_index;
        }

        self.boxes[idx].parent = parent_idx;
        self.boxes[idx].signal = signal;
        self.boxes[idx].last_touched_frame = self.build_index;
        self.apply_scroll_signal(idx);

        if let Some(parent_idx) = parent_idx {
            self.boxes[parent_idx].children.push(idx);
        }
        self.frame_boxes.push(idx);
        UIBoxHandle { idx, key, signal }
    }

    fn apply_scroll_signal(&mut self, idx: usize) {
        let signal = self.boxes[idx].signal;
        if self.boxes[idx].flags.scrolls_x() && signal.scroll_x != 0.0 {
            self.boxes[idx].scroll_target.x -= signal.scroll_x * 16.0;
        }
        if self.boxes[idx].flags.scrolls_y() && signal.scroll_y != 0.0 {
            self.boxes[idx].scroll_target.y -= signal.scroll_y * 16.0;
        }
    }

    fn scrollbar_track_rect(&self, idx: usize, axis: Axis) -> Option<RectCoords> {
        if !self.scrollbar_available(idx, axis) {
            return None;
        }
        let rect = self.boxes[idx].rect;
        let thickness = SCROLLBAR_HOVER_THICKNESS;
        let inset = SCROLLBAR_EDGE_INSET;
        Some(match axis {
            Axis::X => RectCoords::from_size(
                rect.x0,
                rect.y1 - thickness - inset * 2.0,
                rect.width(),
                thickness + inset * 2.0,
            ),
            Axis::Y => RectCoords::from_size(
                rect.x1 - thickness - inset * 2.0,
                rect.y0,
                thickness + inset * 2.0,
                rect.height(),
            ),
        })
    }

    fn scrollbar_thumb_rect(&self, idx: usize, axis: Axis, thickness: f32) -> Option<RectCoords> {
        if !self.scrollbar_available(idx, axis) {
            return None;
        }
        let rect = self.boxes[idx].rect;
        let content = self.boxes[idx].content_size;
        let scroll = self.boxes[idx].scroll;
        let scroll_max = self.boxes[idx].scroll_max;
        let inset = SCROLLBAR_EDGE_INSET;

        Some(match axis {
            Axis::X => {
                let track_w = rect.width().max(1.0);
                let thumb_w = scrollbar_thumb_len(track_w, content.width)
                    .max(12.0)
                    .min(track_w);
                let thumb_x =
                    rect.x0 + (track_w - thumb_w) * (scroll.x / scroll_max.x).clamp(0.0, 1.0);
                RectCoords::from_size(thumb_x, rect.y1 - thickness - inset, thumb_w, thickness)
            }
            Axis::Y => {
                let track_h = rect.height().max(1.0);
                let thumb_h = scrollbar_thumb_len(track_h, content.height)
                    .max(12.0)
                    .min(track_h);
                let thumb_y =
                    rect.y0 + (track_h - thumb_h) * (scroll.y / scroll_max.y).clamp(0.0, 1.0);
                RectCoords::from_size(rect.x1 - thickness - inset, thumb_y, thickness, thumb_h)
            }
        })
    }

    fn scrollbar_available(&self, idx: usize, axis: Axis) -> bool {
        let flags = self.boxes[idx].flags;
        let scroll_max = self.boxes[idx].scroll_max;
        let content = self.boxes[idx].content_size;
        match axis {
            Axis::X => flags.scrolls_x() && scroll_max.x > 0.0 && content.width > 0.0,
            Axis::Y => flags.scrolls_y() && scroll_max.y > 0.0 && content.height > 0.0,
        }
    }

    fn scrollbar_is_hot_or_active(&self, idx: usize, axis: Axis) -> bool {
        let key = self.boxes[idx].key;
        if self
            .active_scrollbar
            .is_some_and(|drag| drag.key == key && drag.axis == axis)
        {
            return true;
        }
        self.scrollbar_track_rect(idx, axis)
            .is_some_and(|rect| point_in_rect(&rect, self.mouse))
    }

    fn scrollbar_thickness(&self, idx: usize, axis: Axis) -> f32 {
        let t = match axis {
            Axis::X => self.boxes[idx].scrollbar_x_t,
            Axis::Y => self.boxes[idx].scrollbar_y_t,
        }
        .clamp(0.0, 1.0);
        SCROLLBAR_THICKNESS + (SCROLLBAR_HOVER_THICKNESS - SCROLLBAR_THICKNESS) * t
    }

    fn apply_scrollbar_events(&mut self, idx: usize, key: UiKey, flags: UIBoxFlags) {
        if key.is_zero() || (!flags.scrolls_x() && !flags.scrolls_y()) {
            return;
        }

        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            let mut taken = false;

            match ev.ty {
                OSEventType::Press if ev.key == OSKey::LeftMouseButton => {
                    if let Some((axis, pos)) = self.scrollbar_hit(idx, ev.pos.or(self.mouse)) {
                        self.begin_scrollbar_drag(idx, key, axis, pos);
                        taken = true;
                    }
                }
                OSEventType::MouseMove
                    if self.active_scrollbar.is_some_and(|drag| drag.key == key)
                        && self.left_mouse_down =>
                {
                    if let Some(pos) = ev.pos.or(self.mouse) {
                        self.drag_scrollbar_to(idx, pos);
                        taken = true;
                    }
                }
                OSEventType::Release
                    if ev.key == OSKey::LeftMouseButton
                        && self.active_scrollbar.is_some_and(|drag| drag.key == key) =>
                {
                    if let Some(pos) = ev.pos.or(self.mouse) {
                        self.drag_scrollbar_to(idx, pos);
                    }
                    self.active_scrollbar = None;
                    taken = true;
                }
                _ => {}
            }

            if taken {
                self.events.remove(ev_idx);
            } else {
                ev_idx += 1;
            }
        }
    }

    fn scrollbar_hit(&self, idx: usize, pos: Option<Point>) -> Option<(Axis, Point)> {
        let pos = pos?;
        for axis in [Axis::Y, Axis::X] {
            if self
                .scrollbar_track_rect(idx, axis)
                .is_some_and(|rect| point_in_rect(&rect, Some(pos)))
            {
                return Some((axis, pos));
            }
        }
        None
    }

    fn begin_scrollbar_drag(&mut self, idx: usize, key: UiKey, axis: Axis, pos: Point) {
        let thumb = self.scrollbar_thumb_rect(idx, axis, SCROLLBAR_HOVER_THICKNESS);
        let thumb_grab_offset = thumb
            .filter(|thumb| point_in_rect(thumb, Some(pos)))
            .map(|thumb| pos.axis(axis) - rect_min_axis(thumb, axis))
            .unwrap_or_else(|| self.scrollbar_thumb_len(idx, axis) * 0.5);

        self.active_scrollbar = Some(ScrollbarDrag {
            key,
            axis,
            thumb_grab_offset,
        });
        self.drag_scrollbar_to(idx, pos);
    }

    fn drag_scrollbar_to(&mut self, idx: usize, pos: Point) {
        let Some(drag) = self.active_scrollbar else {
            return;
        };
        let rect = self.boxes[idx].rect;
        let scroll_max = self.boxes[idx].scroll_max.axis(drag.axis);
        if scroll_max <= 0.0 {
            return;
        }
        let track_min = rect_min_axis(rect, drag.axis);
        let track_len = rect_size_axis(rect, drag.axis).max(1.0);
        let thumb_len = self.scrollbar_thumb_len(idx, drag.axis);
        let movable = (track_len - thumb_len).max(1.0);
        let thumb_min =
            (pos.axis(drag.axis) - drag.thumb_grab_offset - track_min).clamp(0.0, movable);
        let value = scroll_max * (thumb_min / movable);
        self.boxes[idx].scroll.set_axis(drag.axis, value);
        self.boxes[idx].scroll_target.set_axis(drag.axis, value);
        self.request_repaint();
    }

    fn scrollbar_thumb_len(&self, idx: usize, axis: Axis) -> f32 {
        let rect = self.boxes[idx].rect;
        let content = self.boxes[idx].content_size;
        let track_len = rect_size_axis(rect, axis).max(1.0);
        scrollbar_thumb_len(track_len, content.axis(axis))
            .max(12.0)
            .min(track_len)
    }

    fn animate_scroll_offsets(&mut self) {
        let rate = smooth_rate(self.theme.motion.scroll_rate, self.animation_dt);
        let epsilon = 0.5;
        let mut animating = false;
        for idx in self.frame_boxes.clone() {
            let box_ = &mut self.boxes[idx];
            if box_.first_touched_frame == self.build_index {
                box_.scroll = box_.scroll_target;
                continue;
            }
            for axis in [Axis::X, Axis::Y] {
                let current = box_.scroll.axis(axis);
                let target = box_.scroll_target.axis(axis);
                let next = current + (target - current) * rate;
                if (target - next).abs() <= epsilon {
                    box_.scroll.set_axis(axis, target);
                } else {
                    box_.scroll.set_axis(axis, next);
                    animating = true;
                }
            }
        }
        if animating {
            self.request_repaint();
        }
    }

    fn animate_visual_state(&mut self) {
        let hot_rate = smooth_rate(self.theme.motion.hot_rate, self.animation_dt);
        let active_rate = smooth_rate(self.theme.motion.active_rate, self.animation_dt);
        let focus_rate = smooth_rate(self.theme.motion.focus_rate, self.animation_dt);
        let appear_rate = smooth_rate(self.theme.motion.menu_rate, self.animation_dt);
        let color_rate = smooth_rate(30.0, self.animation_dt);
        let epsilon = self.theme.motion.epsilon;
        let mut animating = false;

        for idx in self.frame_boxes.clone() {
            let key = self.boxes[idx].key;
            let is_hot = self.boxes[idx].signal.hovering();
            let is_active = self.active_left_key == Some(key)
                || self.active_right_key == Some(key)
                || self.boxes[idx].signal.pressed();
            let is_focused = self.focus_key == Some(key);
            let is_floating = self.boxes[idx].flags.contains(UIBoxFlags::FLOATING_X)
                || self.boxes[idx].flags.contains(UIBoxFlags::FLOATING_Y);
            let draws_background = self.boxes[idx].flags.contains(UIBoxFlags::DRAW_BACKGROUND);
            let draws_border = self.boxes[idx].flags.contains(UIBoxFlags::DRAW_BORDER);
            let draws_text = self.boxes[idx].flags.contains(UIBoxFlags::DRAW_TEXT);
            let animates_interaction =
                self.boxes[idx].flags.contains(UIBoxFlags::DRAW_HOT_EFFECTS) || draws_border;
            let animates_appearance =
                is_floating && (draws_background || draws_border || draws_text);
            let scrollbar_x_target = (self.scrollbar_available(idx, Axis::X)
                && self.scrollbar_is_hot_or_active(idx, Axis::X))
                as u8 as f32;
            let scrollbar_y_target = (self.scrollbar_available(idx, Axis::Y)
                && self.scrollbar_is_hot_or_active(idx, Axis::Y))
                as u8 as f32;
            let box_ = &mut self.boxes[idx];

            if key.is_zero() {
                box_.hot_t = is_hot as u8 as f32;
                box_.active_t = is_active as u8 as f32;
                box_.focus_t = is_focused as u8 as f32;
                box_.appear_t = 1.0;
                box_.scrollbar_x_t = scrollbar_x_target;
                box_.scrollbar_y_t = scrollbar_y_target;
                box_.bg_color_animated = box_.style.bg_color;
                box_.border_color_animated = box_.style.border_color;
                continue;
            }

            if box_.first_touched_frame == self.build_index {
                box_.hot_t = is_hot as u8 as f32;
                box_.active_t = is_active as u8 as f32;
                box_.focus_t = is_focused as u8 as f32;
                box_.appear_t = if animates_appearance {
                    appear_rate
                } else {
                    1.0
                };
                box_.scrollbar_x_t = scrollbar_x_target;
                box_.scrollbar_y_t = scrollbar_y_target;
                box_.bg_color_animated = box_.style.bg_color;
                box_.border_color_animated = box_.style.border_color;
                if animates_appearance && box_.appear_t < 1.0 - epsilon {
                    animating = true;
                }
                continue;
            }

            box_.hot_t = animate_scalar(box_.hot_t, is_hot as u8 as f32, hot_rate, epsilon);
            box_.active_t =
                animate_scalar(box_.active_t, is_active as u8 as f32, active_rate, epsilon);
            box_.focus_t =
                animate_scalar(box_.focus_t, is_focused as u8 as f32, focus_rate, epsilon);
            box_.appear_t = if animates_appearance {
                animate_scalar(box_.appear_t, 1.0, appear_rate, epsilon)
            } else {
                1.0
            };
            box_.scrollbar_x_t =
                animate_scalar(box_.scrollbar_x_t, scrollbar_x_target, hot_rate, epsilon);
            box_.scrollbar_y_t =
                animate_scalar(box_.scrollbar_y_t, scrollbar_y_target, hot_rate, epsilon);

            let mut target_bg = box_.style.bg_color;
            if box_.flags.contains(UIBoxFlags::DRAW_HOT_EFFECTS) {
                target_bg = color_mix(target_bg, self.theme.surface_hover, box_.hot_t * 0.55);
                target_bg = color_mix(target_bg, self.theme.accent_active, box_.active_t * 0.35);
            }
            let target_border = color_mix(box_.style.border_color, self.theme.accent, box_.focus_t);
            if draws_background {
                box_.bg_color_animated = color_lerp(box_.bg_color_animated, target_bg, color_rate);
            } else {
                box_.bg_color_animated = box_.style.bg_color;
            }
            if draws_border {
                box_.border_color_animated =
                    color_lerp(box_.border_color_animated, target_border, color_rate);
            } else {
                box_.border_color_animated = box_.style.border_color;
            }

            animating = animating
                || (animates_interaction
                    && ((box_.hot_t - is_hot as u8 as f32).abs() > epsilon
                        || (box_.active_t - is_active as u8 as f32).abs() > epsilon
                        || (box_.focus_t - is_focused as u8 as f32).abs() > epsilon))
                || (1.0 - box_.appear_t).abs() > epsilon
                || (box_.scrollbar_x_t - scrollbar_x_target).abs() > epsilon
                || (box_.scrollbar_y_t - scrollbar_y_target).abs() > epsilon
                || (draws_background
                    && color_distance(box_.bg_color_animated, target_bg) > epsilon)
                || (draws_border
                    && color_distance(box_.border_color_animated, target_border) > epsilon);
        }

        if animating {
            self.request_repaint();
        }
    }

    fn signal_from_key_and_flags(
        &mut self,
        key: UiKey,
        flags: UIBoxFlags,
        existing_idx: Option<usize>,
    ) -> UiSignal {
        let mut signal = UiSignal::default();
        let rect = existing_idx
            .map(|idx| expanded_rect(self.boxes[idx].rect, self.boxes[idx].hit_padding))
            .unwrap_or_else(|| RectCoords::from_size(-10000.0, -10000.0, 0.0, 0.0));
        let mouse_over = point_in_rect(&rect, self.mouse);
        let focused = self.focus_key == Some(key);

        if let Some(idx) = existing_idx {
            self.apply_scrollbar_events(idx, key, flags);
        }

        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            let in_bounds = point_in_rect(&rect, ev.pos.or(self.mouse));
            let mut taken = false;

            if flags.is_mouse_clickable() {
                if let Some(button) = mouse_button_from_key(ev.key) {
                    match ev.ty {
                        OSEventType::Press if in_bounds => {
                            self.hot_key = Some(key);
                            self.set_active_key(button, Some(key));
                            self.drag_start_mouse = ev.pos.or(self.mouse);
                            if button == MouseButton::Left {
                                signal.flags |= UiSignal::LEFT_PRESSED;
                                signal.left_press_pos = ev.pos.or(self.mouse);
                            }
                            taken = true;
                        }
                        OSEventType::Release if self.active_key(button) == Some(key) => {
                            self.set_active_key(button, None);
                            if button == MouseButton::Left {
                                signal.flags |= UiSignal::LEFT_RELEASED;
                                if in_bounds {
                                    signal.flags |= UiSignal::LEFT_CLICKED | UiSignal::COMMIT;
                                }
                            }
                            if !in_bounds && self.hot_key == Some(key) {
                                self.hot_key = None;
                            }
                            taken = true;
                        }
                        _ => {}
                    }
                }
            }

            if !taken
                && flags.is_keyboard_clickable()
                && focused
                && ev.ty == OSEventType::Press
                && matches!(
                    ev.key,
                    OSKey::Keyboard(OSKeyCode::KeyEnter) | OSKey::Keyboard(OSKeyCode::KeySpace)
                )
            {
                signal.flags |= UiSignal::COMMIT | UiSignal::LEFT_CLICKED;
                taken = true;
            }

            if !taken && ev.ty == OSEventType::Scroll && in_bounds {
                if flags.scrolls_y() {
                    signal.scroll_y += ev.delta;
                    taken = true;
                }
                if flags.scrolls_x() {
                    signal.scroll_x += ev.delta;
                    taken = true;
                }
            }

            if taken {
                self.events.remove(ev_idx);
            } else {
                ev_idx += 1;
            }
        }

        if mouse_over {
            signal.flags |= UiSignal::MOUSE_OVER;
        }
        if flags.is_mouse_clickable()
            && mouse_over
            && (self.hot_key.is_none() || self.hot_key == Some(key))
            && (self.active_left_key.is_none() || self.active_left_key == Some(key))
            && (self.active_right_key.is_none() || self.active_right_key == Some(key))
        {
            self.hot_key = Some(key);
            signal.flags |= UiSignal::HOVERING;
        }

        if self.active_left_key == Some(key) && self.left_mouse_down {
            signal.flags |= UiSignal::LEFT_DRAGGING;
        }
        signal
    }

    fn layout_root(&mut self, root: usize) {
        for axis in [Axis::X, Axis::Y] {
            self.calc_standalone(root, axis);
            self.calc_upwards(root, axis);
            self.calc_downwards(root, axis);
            self.enforce_constraints(root, axis);
            self.reconcile_overflow(root, axis);
            self.clamp_scroll_offsets(root, axis);
            self.position(root, axis);
        }
    }

    fn clamp_scroll_offsets(&mut self, idx: usize, axis: Axis) {
        let children = self.boxes[idx].children.clone();
        if self.boxes[idx].child_layout_axis == axis {
            let content_size = (self.boxes[idx].computed_size.axis(axis)
                - self.boxes[idx].padding.axis(axis))
            .max(0.0);
            let used_size = self.total_children_size(idx, axis);
            let max_scroll = (used_size - content_size).max(0.0);
            self.boxes[idx].content_size.set_axis(axis, used_size);
            self.boxes[idx].scroll_max.set_axis(axis, max_scroll);
            match axis {
                Axis::X => {
                    self.boxes[idx].scroll.x = self.boxes[idx].scroll.x.clamp(0.0, max_scroll);
                    self.boxes[idx].scroll_target.x =
                        self.boxes[idx].scroll_target.x.clamp(0.0, max_scroll);
                }
                Axis::Y => {
                    self.boxes[idx].scroll.y = self.boxes[idx].scroll.y.clamp(0.0, max_scroll);
                    self.boxes[idx].scroll_target.y =
                        self.boxes[idx].scroll_target.y.clamp(0.0, max_scroll);
                }
            }
        }
        for child in children {
            self.clamp_scroll_offsets(child, axis);
        }
    }

    fn reconcile_overflow(&mut self, idx: usize, axis: Axis) {
        self.reconcile_container_overflow(idx, axis);
        let children = self.boxes[idx].children.clone();
        for child in children {
            self.reconcile_overflow(child, axis);
        }
    }

    fn reconcile_container_overflow(&mut self, parent: usize, axis: Axis) {
        if self.boxes[parent].child_layout_axis != axis {
            return;
        }
        if (axis == Axis::X && self.boxes[parent].flags.scrolls_x())
            || (axis == Axis::Y && self.boxes[parent].flags.scrolls_y())
        {
            // Scroll containers keep child sizes and rely on scrolling instead of shrinking.
            return;
        }
        let children: Vec<usize> = self.boxes[parent]
            .children
            .iter()
            .copied()
            .filter(|child| !self.box_is_out_of_flow(*child))
            .collect();
        if children.is_empty() {
            return;
        }

        let content_size = (self.boxes[parent].computed_size.axis(axis)
            - self.boxes[parent].padding.axis(axis))
        .max(0.0);
        let gaps = self.boxes[parent].child_gap * children.len().saturating_sub(1) as f32;
        let sum_children: f32 = children
            .iter()
            .map(|child| self.boxes[*child].computed_size.axis(axis))
            .sum();
        let mut overflow = (sum_children + gaps - content_size).max(0.0);
        if overflow <= 0.0 {
            return;
        }

        overflow = self.shrink_group_to_fit(&children, axis, overflow, true);
        if overflow > 0.0 {
            overflow = self.shrink_group_to_fit(&children, axis, overflow, false);
        }
        if overflow > 0.0 {
            // Hard fallback guarantee: shrink from tail down to zero.
            for child in children.iter().rev() {
                if overflow <= 0.0 {
                    break;
                }
                let cur = self.boxes[*child].computed_size.axis(axis);
                let take = cur.min(overflow);
                self.boxes[*child].computed_size.set_axis(axis, cur - take);
                overflow -= take;
            }
        }
    }

    fn shrink_group_to_fit(
        &mut self,
        children: &[usize],
        axis: Axis,
        mut overflow: f32,
        fill_only: bool,
    ) -> f32 {
        if overflow <= 0.0 {
            return 0.0;
        }
        let eligible: Vec<usize> = children
            .iter()
            .copied()
            .filter(|idx| {
                let is_fill = self.boxes[*idx].pref_size[axis_idx(axis)] == UISize::Fill;
                if fill_only { is_fill } else { !is_fill }
            })
            .collect();
        if eligible.is_empty() {
            return overflow;
        }

        let capacities: Vec<(usize, f32)> = eligible
            .iter()
            .map(|idx| {
                let cur = self.boxes[*idx].computed_size.axis(axis);
                let min = self.boxes[*idx].min_size.axis(axis);
                (*idx, (cur - min).max(0.0))
            })
            .collect();
        let total_capacity: f32 = capacities.iter().map(|(_, c)| *c).sum();
        if total_capacity <= 0.0 {
            return overflow;
        }

        let target = overflow.min(total_capacity);
        let mut taken_total = 0.0;
        for (idx, cap) in &capacities {
            if *cap <= 0.0 {
                continue;
            }
            let take = (target * (*cap / total_capacity)).min(*cap);
            let cur = self.boxes[*idx].computed_size.axis(axis);
            self.boxes[*idx].computed_size.set_axis(axis, cur - take);
            taken_total += take;
        }

        overflow -= taken_total;
        overflow.max(0.0)
    }

    fn calc_standalone(&mut self, idx: usize, axis: Axis) {
        let children = self.boxes[idx].children.clone();
        for child in children {
            self.calc_standalone(child, axis);
        }
        let pref = self.boxes[idx].pref_size[axis_idx(axis)];
        let value = match pref {
            UISize::Pixels(v) => v,
            UISize::TextContent(padding) => {
                let text = self.boxes[idx].display_string.clone().unwrap_or_default();
                let font_size = self.boxes[idx].style.font_size;
                let margin = self.boxes[idx].style.margin;
                let (w, h) = self.text_size(font_size, &text);
                match axis {
                    Axis::X => w + padding + self.boxes[idx].padding.horizontal() + margin * 2.0,
                    Axis::Y => {
                        h.max(font_size)
                            + padding
                            + self.boxes[idx].padding.vertical()
                            + margin * 2.0
                    }
                }
            }
            UISize::ParentPct(_) | UISize::ChildrenSum | UISize::Fill => {
                self.boxes[idx].min_size.axis(axis)
            }
        };
        self.boxes[idx].computed_size.set_axis(axis, value);
    }

    fn calc_upwards(&mut self, idx: usize, axis: Axis) {
        let children = self.boxes[idx].children.clone();
        for child in children {
            self.calc_upwards(child, axis);
        }
        if self.boxes[idx].pref_size[axis_idx(axis)] != UISize::ChildrenSum {
            return;
        }
        let child_axis = self.boxes[idx].child_layout_axis;
        let mut size: f32 = 0.0;
        if axis == child_axis {
            for child in &self.boxes[idx].children {
                if self.box_is_out_of_flow(*child) {
                    continue;
                }
                size += self.boxes[*child].computed_size.axis(axis);
            }
            let child_count = self.in_flow_child_count(idx);
            if child_count > 1 {
                size += self.boxes[idx].child_gap * (child_count - 1) as f32;
            }
        } else {
            for child in &self.boxes[idx].children {
                if self.box_is_out_of_flow(*child) {
                    continue;
                }
                size = size.max(self.boxes[*child].computed_size.axis(axis));
            }
        }
        size += self.boxes[idx].padding.axis(axis);
        self.boxes[idx].computed_size.set_axis(axis, size);
    }

    fn calc_downwards(&mut self, idx: usize, axis: Axis) {
        let parent_content = if let Some(parent) = self.boxes[idx].parent {
            (self.boxes[parent].computed_size.axis(axis) - self.boxes[parent].padding.axis(axis))
                .max(0.0)
        } else {
            self.boxes[idx].computed_size.axis(axis)
        };
        self.apply_downward_size(idx, axis, parent_content);

        // Resolve direct children on this axis before recursing so descendants
        // observe the final parent size (especially for Fill children).
        let children = self.boxes[idx].children.clone();
        for child in &children {
            if self.box_is_out_of_flow(*child) {
                continue;
            }
            let content = (self.boxes[idx].computed_size.axis(axis)
                - self.boxes[idx].padding.axis(axis))
            .max(0.0);
            self.apply_downward_size(*child, axis, content);
        }
        if self.boxes[idx].child_layout_axis == axis {
            self.distribute_fill_children(idx, axis);
        }

        let children = self.boxes[idx].children.clone();
        for child in children {
            self.calc_downwards(child, axis);
        }
    }

    fn apply_downward_size(&mut self, idx: usize, axis: Axis, parent_content: f32) {
        match self.boxes[idx].pref_size[axis_idx(axis)] {
            UISize::ParentPct(pct) => self.boxes[idx]
                .computed_size
                .set_axis(axis, (parent_content * pct).max(0.0)),
            UISize::Fill => {
                // On the parent's main axis, Fill is resolved by
                // `distribute_fill_children`; don't overwrite that result here.
                if let Some(parent) = self.boxes[idx].parent {
                    if self.boxes[parent].child_layout_axis == axis {
                        return;
                    }
                }
                // On cross-axis, Fill behaves like ParentPct(1.0).
                self.boxes[idx]
                    .computed_size
                    .set_axis(axis, parent_content.max(0.0));
            }
            _ => {}
        }
    }

    fn enforce_constraints(&mut self, idx: usize, axis: Axis) {
        let min = self.boxes[idx].min_size.axis(axis);
        let size = self.boxes[idx].computed_size.axis(axis).max(min);
        self.boxes[idx].computed_size.set_axis(axis, size);
        let children = self.boxes[idx].children.clone();
        for child in children {
            self.enforce_constraints(child, axis);
        }
    }

    fn position(&mut self, idx: usize, axis: Axis) {
        let origin = if self.boxes[idx].flags.contains(match axis {
            Axis::X => UIBoxFlags::FLOATING_X,
            Axis::Y => UIBoxFlags::FLOATING_Y,
        }) {
            self.boxes[idx].fixed_position.axis(axis)
        } else if let Some(parent) = self.boxes[idx].parent {
            let parent_axis = self.boxes[parent].child_layout_axis;
            if axis == parent_axis {
                self.position_on_main_axis(idx, parent, axis)
            } else {
                self.position_on_cross_axis(idx, parent, axis)
            }
        } else {
            0.0
        };
        self.set_rect_axis(idx, axis, origin, self.boxes[idx].computed_size.axis(axis));
        let children = self.boxes[idx].children.clone();
        self.distribute_fill_children(idx, axis);
        for child in children {
            self.position(child, axis);
        }
    }

    fn position_on_main_axis(&self, idx: usize, parent: usize, axis: Axis) -> f32 {
        if self.box_is_out_of_flow(idx) {
            return self.boxes[idx].fixed_position.axis(axis);
        }
        let children: Vec<usize> = self.boxes[parent]
            .children
            .iter()
            .copied()
            .filter(|child| !self.box_is_out_of_flow(*child))
            .collect();
        let child_pos = children.iter().position(|child| *child == idx).unwrap_or(0);
        let padding_start = self.boxes[parent].padding.min_axis(axis);
        let padding_end = self.boxes[parent].padding.max_axis(axis);
        let content_start = self.boxes[parent].rect_axis_min(axis) + padding_start;
        let content_size =
            (self.boxes[parent].computed_size.axis(axis) - padding_start - padding_end).max(0.0);
        let total_children_size = self.total_children_size(parent, axis);
        let extra = (content_size - total_children_size).max(0.0);
        let mut start = content_start;
        let mut gap = self.boxes[parent].child_gap;
        match self.boxes[parent].main_axis_align {
            MainAxisAlign::Start => {}
            MainAxisAlign::Center => start += extra / 2.0,
            MainAxisAlign::End => start += extra,
            MainAxisAlign::SpaceBetween if children.len() > 1 => {
                gap += extra / (children.len() - 1) as f32;
            }
            MainAxisAlign::SpaceAround if !children.is_empty() => {
                gap += extra / children.len() as f32;
                start += gap / 2.0;
            }
            MainAxisAlign::SpaceEvenly if !children.is_empty() => {
                gap += extra / (children.len() + 1) as f32;
                start += gap;
            }
            _ => {}
        }
        let mut pos = start;
        for child in children.iter().take(child_pos) {
            pos += self.boxes[*child].computed_size.axis(axis) + gap;
        }
        if self.boxes[parent].flags.scrolls_x() && axis == Axis::X {
            pos -= self.boxes[parent].scroll.x;
        }
        if self.boxes[parent].flags.scrolls_y() && axis == Axis::Y {
            pos -= self.boxes[parent].scroll.y;
        }
        pos
    }

    fn position_on_cross_axis(&self, idx: usize, parent: usize, axis: Axis) -> f32 {
        let padding_start = self.boxes[parent].padding.min_axis(axis);
        let padding_end = self.boxes[parent].padding.max_axis(axis);
        let base = self.boxes[parent].rect_axis_min(axis) + padding_start;
        let available =
            (self.boxes[parent].computed_size.axis(axis) - padding_start - padding_end).max(0.0);
        let child_size = self.boxes[idx].computed_size.axis(axis);
        let pos = match self.boxes[parent].cross_axis_align {
            CrossAxisAlign::Start | CrossAxisAlign::Stretch => base,
            CrossAxisAlign::Center => base + (available - child_size).max(0.0) / 2.0,
            CrossAxisAlign::End => base + (available - child_size).max(0.0),
        };
        let scroll = if axis == Axis::X && self.boxes[parent].flags.scrolls_x() {
            self.boxes[parent].scroll.x
        } else if axis == Axis::Y && self.boxes[parent].flags.scrolls_y() {
            self.boxes[parent].scroll.y
        } else {
            0.0
        };
        pos - scroll
    }

    fn distribute_fill_children(&mut self, parent: usize, axis: Axis) {
        if self.boxes[parent].child_layout_axis != axis {
            return;
        }
        let fill_children: Vec<usize> = self.boxes[parent]
            .children
            .iter()
            .copied()
            .filter(|idx| {
                !self.box_is_out_of_flow(*idx)
                    && self.boxes[*idx].pref_size[axis_idx(axis)] == UISize::Fill
            })
            .collect();
        if fill_children.is_empty() {
            return;
        }
        let padding = self.boxes[parent].padding.axis(axis);
        let child_count = self.in_flow_child_count(parent);
        let gaps = self.boxes[parent].child_gap * child_count.saturating_sub(1) as f32;
        let fixed: f32 = self.boxes[parent]
            .children
            .iter()
            .filter(|idx| {
                !self.box_is_out_of_flow(**idx)
                    && self.boxes[**idx].pref_size[axis_idx(axis)] != UISize::Fill
            })
            .map(|idx| self.boxes[*idx].computed_size.axis(axis))
            .sum();
        let available =
            (self.boxes[parent].computed_size.axis(axis) - padding - gaps - fixed).max(0.0);
        let each = available / fill_children.len() as f32;
        for child in fill_children {
            self.boxes[child].computed_size.set_axis(axis, each);
        }
    }

    fn total_children_size(&self, parent: usize, axis: Axis) -> f32 {
        let children: Vec<usize> = self.boxes[parent]
            .children
            .iter()
            .copied()
            .filter(|child| !self.box_is_out_of_flow(*child))
            .collect();
        let mut total: f32 = children
            .iter()
            .map(|child| self.boxes[*child].computed_size.axis(axis))
            .sum();
        if children.len() > 1 {
            total += self.boxes[parent].child_gap * (children.len() - 1) as f32;
        }
        total
    }

    fn box_is_out_of_flow(&self, idx: usize) -> bool {
        self.boxes[idx].flags.contains(UIBoxFlags::FLOATING_X)
            || self.boxes[idx].flags.contains(UIBoxFlags::FLOATING_Y)
    }

    fn in_flow_child_count(&self, parent: usize) -> usize {
        self.boxes[parent]
            .children
            .iter()
            .filter(|child| !self.box_is_out_of_flow(**child))
            .count()
    }

    fn set_rect_axis(&mut self, idx: usize, axis: Axis, min: f32, size: f32) {
        match axis {
            Axis::X => {
                self.boxes[idx].rect.x0 = min;
                self.boxes[idx].rect.x1 = min + size;
            }
            Axis::Y => {
                self.boxes[idx].rect.y0 = min;
                self.boxes[idx].rect.y1 = min + size;
            }
        }
    }

    fn draw_ui_all(&mut self) {
        if self.drawer.is_none() {
            return;
        }
        let root_clip = self.boxes[self.root].rect;
        self.draw_ui_root_skipping_clipped(self.root, Some(self.overlay_root), root_clip);
        self.draw_ui_root_clipped(self.overlay_root, root_clip);
    }

    fn draw_ui_root_clipped(&mut self, idx: usize, clip: RectCoords) {
        self.draw_ui_root_skipping_clipped(idx, None, clip);
    }

    fn draw_ui_root_skipping_clipped(
        &mut self,
        idx: usize,
        skip_idx: Option<usize>,
        clip: RectCoords,
    ) {
        if skip_idx == Some(idx) {
            return;
        }
        if !self.boxes[idx].visible {
            return;
        }
        let rect = self.boxes[idx].rect;
        let draw_rect = intersect_rects(rect, clip);
        if draw_rect.width() <= 0.0 || draw_rect.height() <= 0.0 {
            return;
        }
        let flags = self.boxes[idx].flags;
        let style = self.boxes[idx].style;
        let opacity = self.box_opacity(idx);
        let draw_bg = flags.contains(UIBoxFlags::DRAW_BACKGROUND);
        let draw_border = flags.contains(UIBoxFlags::DRAW_BORDER);
        let rounded_with_border = draw_bg && draw_border && style.corner_radius > 0.0;

        if rounded_with_border {
            // Rounded border: draw outer border shape then inset background shape.
            self.drawer.as_mut().unwrap().draw_rect(
                &draw_rect,
                color_mul_alpha(self.boxes[idx].border_color_animated, opacity),
                style.corner_radius,
            );

            let inset = style.border_size.max(0.0);
            let inner_w = (draw_rect.width() - inset * 2.0).max(0.0);
            let inner_h = (draw_rect.height() - inset * 2.0).max(0.0);
            if inner_w > 0.0 && inner_h > 0.0 {
                let inner = RectCoords::from_size(
                    draw_rect.x0 + inset,
                    draw_rect.y0 + inset,
                    inner_w,
                    inner_h,
                );
                let inner_radius = (style.corner_radius - inset).max(0.0);
                self.drawer.as_mut().unwrap().draw_rect(
                    &inner,
                    color_mul_alpha(self.boxes[idx].bg_color_animated, opacity),
                    inner_radius,
                );
            }
        } else if draw_bg {
            self.drawer.as_mut().unwrap().draw_rect(
                &draw_rect,
                color_mul_alpha(self.boxes[idx].bg_color_animated, opacity),
                style.corner_radius,
            );
        }
        if draw_border && !rounded_with_border {
            self.drawer.as_mut().unwrap().draw_empty_rect(
                &draw_rect,
                color_mul_alpha(self.boxes[idx].border_color_animated, opacity),
                style.border_size,
            );
        }
        if flags.contains(UIBoxFlags::DRAW_TEXT)
            && let Some(text) = self.boxes[idx].display_string.clone()
        {
            let padding = self.boxes[idx].padding;
            self.drawer.as_mut().unwrap().draw_text(
                rect.x0 + padding.left + style.margin,
                rect.y0 + padding.top + style.margin,
                style.font_size,
                &text,
                text.len(),
                (rect.x1 - padding.right - style.margin).min(clip.x1),
                (rect.y1 - padding.bottom - style.margin).min(clip.y1),
                color_mul_alpha(style.text_color, opacity),
                false,
                style.font_icon,
            );
        }
        let child_clip = if flags.contains(UIBoxFlags::CLIP) {
            intersect_rects(clip, rect)
        } else {
            clip
        };
        let children = self.boxes[idx].children.clone();
        for child in children {
            self.draw_ui_root_skipping_clipped(child, skip_idx, child_clip);
        }
        self.draw_scrollbars(idx, clip);
        self.draw_text_selection_if_focused(idx);
        self.draw_text_caret_if_focused(idx);
    }

    fn box_opacity(&self, idx: usize) -> f32 {
        let mut opacity = 1.0;
        let mut current = Some(idx);
        while let Some(idx) = current {
            opacity *= self.boxes[idx].appear_t.clamp(0.0, 1.0);
            current = self.boxes[idx].parent;
        }
        opacity
    }

    fn draw_scrollbars(&mut self, idx: usize, clip: RectCoords) {
        if self.drawer.is_none() {
            return;
        }
        let color = color_mul_alpha(self.theme.scrollbar, self.box_opacity(idx));
        if self.scrollbar_available(idx, Axis::Y) {
            let thickness = self.scrollbar_thickness(idx, Axis::Y);
            let Some(bar) = self.scrollbar_thumb_rect(idx, Axis::Y, thickness) else {
                return;
            };
            let bar = intersect_rects(bar, clip);
            if bar.width() > 0.0 && bar.height() > 0.0 {
                self.drawer
                    .as_mut()
                    .unwrap()
                    .draw_rect(&bar, color, thickness * 0.5);
            }
        }
        if self.scrollbar_available(idx, Axis::X) {
            let thickness = self.scrollbar_thickness(idx, Axis::X);
            let Some(bar) = self.scrollbar_thumb_rect(idx, Axis::X, thickness) else {
                return;
            };
            let bar = intersect_rects(bar, clip);
            if bar.width() > 0.0 && bar.height() > 0.0 {
                self.drawer
                    .as_mut()
                    .unwrap()
                    .draw_rect(&bar, color, thickness * 0.5);
            }
        }
    }

    fn draw_text_caret_if_focused(&mut self, idx: usize) {
        if self.drawer.is_none() || self.focus_key != Some(self.boxes[idx].key) {
            return;
        }
        if !self.boxes[idx].flags.accepts_text_input() {
            return;
        }
        let now = self.now_seconds();
        let state = self.text_edit_states.get(&self.boxes[idx].key);
        let last_interaction = state.map(|s| s.last_interaction_time).unwrap_or(now);
        let elapsed = now - last_interaction;
        // Show caret for 0.5s after interaction, then blink at 2 Hz.
        if elapsed > 0.5 && ((elapsed - 0.5) * 2.0) as i64 % 2 != 0 {
            return;
        }

        if self.boxes[idx].flags.contains(UIBoxFlags::LINE_EDIT) {
            self.draw_line_edit_caret(idx);
        } else if self.boxes[idx].flags.contains(UIBoxFlags::TEXTAREA) {
            self.draw_textarea_caret(idx);
        }
    }

    fn draw_text_selection_if_focused(&mut self, idx: usize) {
        if self.drawer.is_none() || self.focus_key != Some(self.boxes[idx].key) {
            return;
        }
        if !self.boxes[idx].flags.accepts_text_input() {
            return;
        }
        let Some(range) = self
            .text_edit_states
            .get(&self.boxes[idx].key)
            .and_then(TextEditState::selection_range)
        else {
            return;
        };
        let mut color = self.theme.color_main;
        color.a = 0.35;
        if self.boxes[idx].flags.contains(UIBoxFlags::LINE_EDIT) {
            self.draw_line_edit_selection(idx, range, color);
        } else if self.boxes[idx].flags.contains(UIBoxFlags::TEXTAREA) {
            self.draw_textarea_selection(idx, range, color);
        }
    }

    fn draw_line_edit_selection(&mut self, idx: usize, range: (usize, usize), color: Color) {
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let style = self.boxes[idx].style;
        let text = self.boxes[idx].display_string.clone().unwrap_or_default();
        let start_text = substring_chars(&text, (0, range.0.min(char_count(&text))));
        let selected_text = substring_chars(&text, (range.0, range.1.min(char_count(&text))));
        let start_w = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, &start_text, start_text.len())
            .0;
        let selected_w = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, &selected_text, selected_text.len())
            .0;
        let x = rect.x0 + padding.left + style.margin + start_w;
        let y = rect.y0 + padding.top + style.margin;
        let h = (rect.y1 - padding.bottom - style.margin - y).max(1.0);
        let sel = RectCoords::from_size(x, y, selected_w, h);
        let sel = intersect_rects(sel, rect);
        if sel.width() > 0.0 && sel.height() > 0.0 {
            self.drawer.as_mut().unwrap().draw_rect(&sel, color, 1.0);
        }
    }

    fn draw_textarea_selection(&mut self, idx: usize, range: (usize, usize), color: Color) {
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let style = self.boxes[idx].style;
        let text = self.boxes[idx].string.clone().unwrap_or_default();
        let line_h = self.theme.size_text + 6.0;
        let content_width =
            (rect.x1 - rect.x0 - padding.horizontal() - style.margin * 2.0).max(0.0);
        let ranges = self.compute_visual_line_ranges(&text, content_width, style.font_size);
        let (start_line, _) = self.visual_line_col_from_cursor_with_ranges(&ranges, range.0);
        let (end_line, _) = self.visual_line_col_from_cursor_with_ranges(&ranges, range.1);
        for line in start_line..=end_line {
            let (line_start, line_end_idx) = ranges[line];
            let start = if line == start_line {
                range.0.max(line_start)
            } else {
                line_start
            };
            let end = if line == end_line {
                range.1.min(line_end_idx)
            } else {
                line_end_idx
            };
            if start >= end {
                continue;
            }
            let before = substring_chars(&text, (line_start, start));
            let selected = substring_chars(&text, (start, end));
            let start_w = self
                .drawer
                .as_ref()
                .unwrap()
                .get_text_size(style.font_size, &before, before.len())
                .0;
            let selected_w = self
                .drawer
                .as_ref()
                .unwrap()
                .get_text_size(style.font_size, &selected, selected.len())
                .0;
            let x = rect.x0 + padding.left + style.margin + start_w;
            let y = rect.y0 + padding.top + style.margin + line as f32 * line_h
                - self.boxes[idx].scroll.y;
            let sel = RectCoords::from_size(x, y, selected_w, line_h);
            let sel = intersect_rects(sel, rect);
            if sel.width() > 0.0 && sel.height() > 0.0 {
                self.drawer.as_mut().unwrap().draw_rect(&sel, color, 1.0);
            }
        }
    }

    fn draw_line_edit_caret(&mut self, idx: usize) {
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let style = self.boxes[idx].style;
        let text = self.boxes[idx].display_string.clone().unwrap_or_default();
        let cursor = self
            .text_edit_states
            .get(&self.boxes[idx].key)
            .map(|state| state.cursor)
            .unwrap_or_else(|| char_count(&text))
            .min(char_count(&text));
        let prefix = substring_chars(&text, (0, cursor));
        let text_width = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, &prefix, prefix.len())
            .0;
        let text_height = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, "M", 1)
            .1;

        let content_x0 = rect.x0 + padding.left + style.margin;
        let content_y0 = rect.y0 + padding.top + style.margin;
        let content_x1 = rect.x1 - padding.right - style.margin;
        let content_y1 = rect.y1 - padding.bottom - style.margin;

        let caret_x = (content_x0 + text_width).min(content_x1 - 1.0);
        let caret_h = text_height.min((content_y1 - content_y0).max(1.0));
        let caret_rect = RectCoords::from_size(caret_x, content_y0, 1.5, caret_h);
        self.drawer
            .as_mut()
            .unwrap()
            .draw_rect(&caret_rect, self.theme.color_text, 0.0);
    }

    fn draw_textarea_caret(&mut self, idx: usize) {
        let style = self.boxes[idx].style;
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let text = self.boxes[idx].string.clone().unwrap_or_default();
        let cursor = self
            .text_edit_states
            .get(&self.boxes[idx].key)
            .map(|state| state.cursor)
            .unwrap_or_else(|| char_count(&text))
            .min(char_count(&text));
        let content_width =
            (rect.x1 - rect.x0 - padding.horizontal() - style.margin * 2.0).max(0.0);
        let ranges = self.compute_visual_line_ranges(&text, content_width, style.font_size);
        let (visual_line, col) = self.visual_line_col_from_cursor_with_ranges(&ranges, cursor);
        let (line_start, _) = ranges[visual_line.min(ranges.len() - 1)];
        let line_prefix = substring_chars(&text, (line_start, line_start + col));
        let content_x0 = rect.x0 + padding.left + style.margin;
        let line_h = self.theme.size_text + 6.0;
        let content_y0 = rect.y0 + padding.top + style.margin + visual_line as f32 * line_h
            - self.boxes[idx].scroll.y;
        let content_x1 = rect.x1 - padding.right - style.margin;
        let content_y1 = (content_y0 + line_h).min(rect.y1 - padding.bottom - style.margin);

        let text_width = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, &line_prefix, line_prefix.len())
            .0;
        let text_height = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, "M", 1)
            .1;

        let caret_x = (content_x0 + text_width).min(content_x1 - 1.0);
        let caret_h = text_height.min((content_y1 - content_y0).max(1.0));
        let caret_rect = RectCoords::from_size(caret_x, content_y0, 1.5, caret_h);
        self.drawer
            .as_mut()
            .unwrap()
            .draw_rect(&caret_rect, self.theme.color_text, 0.0);
    }

    fn text_size(&mut self, font_size: f32, text: &str) -> (f32, f32) {
        if let Some(drawer) = self.drawer.as_ref() {
            drawer.get_text_size(font_size, text, text.len())
        } else {
            (text.chars().count() as f32 * font_size * 0.6, font_size)
        }
    }

    fn active_key(&self, button: MouseButton) -> Option<UiKey> {
        match button {
            MouseButton::Left => self.active_left_key,
            MouseButton::Right => self.active_right_key,
        }
    }

    fn set_active_key(&mut self, button: MouseButton, key: Option<UiKey>) {
        match button {
            MouseButton::Left => self.active_left_key = key,
            MouseButton::Right => self.active_right_key = key,
        }
    }

    fn clipped_rect(&self, idx: usize) -> RectCoords {
        let mut rect = self.boxes[idx].rect;
        let mut parent = self.boxes[idx].parent;
        while let Some(parent_idx) = parent {
            let parent_box = &self.boxes[parent_idx];
            if parent_box.flags.contains(UIBoxFlags::CLIP) {
                rect = intersect_rects(rect, parent_box.rect);
            }
            parent = parent_box.parent;
        }
        rect
    }

    fn refresh_passive_signals(&mut self) {
        self.hot_key = None;
        self.cursor = OSCursor::Arrow;

        let frame_boxes = self.frame_boxes_in_hit_test_order();
        let mut hover_candidate = None;

        for &idx in &frame_boxes {
            self.boxes[idx].signal.flags &=
                !(UiSignal::MOUSE_OVER | UiSignal::HOVERING | UiSignal::LEFT_DRAGGING);
        }

        for &idx in &frame_boxes {
            let rect = expanded_rect(self.clipped_rect(idx), self.boxes[idx].hit_padding);
            if point_in_rect(&rect, self.mouse) {
                self.boxes[idx].signal.flags |= UiSignal::MOUSE_OVER;
                if self.boxes[idx].flags.is_mouse_clickable() {
                    hover_candidate = Some(idx);
                }
            }
            if self.active_left_key == Some(self.boxes[idx].key) && self.left_mouse_down {
                self.boxes[idx].signal.flags |= UiSignal::LEFT_DRAGGING;
            }
        }

        if let Some(idx) = hover_candidate {
            self.hot_key = Some(self.boxes[idx].key);
            self.boxes[idx].signal.flags |= UiSignal::HOVERING;
            self.cursor = if self.boxes[idx].flags.accepts_text_input() {
                OSCursor::IBeam
            } else {
                self.boxes[idx].cursor.unwrap_or(OSCursor::Hand)
            };
        }

        if self.left_mouse_down {
            if let Some(idx) = self.active_left_key.and_then(|key| self.box_from_key(key)) {
                if let Some(cursor) = self.boxes[idx].cursor {
                    self.cursor = cursor;
                }
            }
        }
    }

    fn frame_boxes_in_hit_test_order(&self) -> Vec<usize> {
        let mut normal = Vec::new();
        let mut overlay = Vec::new();
        for &idx in &self.frame_boxes {
            if self.is_overlay_box(idx) {
                overlay.push(idx);
            } else {
                normal.push(idx);
            }
        }
        normal.extend(overlay);
        normal
    }

    fn is_overlay_box(&self, idx: usize) -> bool {
        idx == self.overlay_root || self.box_has_ancestor(idx, self.overlay_root)
    }

    fn box_has_ancestor(&self, idx: usize, ancestor: usize) -> bool {
        let mut parent = self.boxes[idx].parent;
        while let Some(parent_idx) = parent {
            if parent_idx == ancestor {
                return true;
            }
            parent = self.boxes[parent_idx].parent;
        }
        false
    }

    fn release_box(&mut self, idx: usize) {
        self.boxes[idx] = UIBox::new(UiKey::default(), UIBoxFlags::NONE, None, &self.theme);
        self.free_boxes.push(idx);
    }

    fn prune_boxes(&mut self) {
        let frame = self.build_index;
        let stale_keys: Vec<UiKey> = self
            .box_table
            .iter()
            .filter_map(|(key, &idx)| (self.boxes[idx].last_touched_frame < frame).then_some(*key))
            .collect();

        for key in stale_keys {
            if let Some(idx) = self.box_table.remove(&key) {
                self.release_box(idx);
            }
        }

        let transient_boxes: Vec<usize> = self
            .frame_boxes
            .iter()
            .copied()
            .filter(|&idx| self.boxes[idx].key.is_zero())
            .collect();
        for idx in transient_boxes {
            self.release_box(idx);
        }

        if self
            .active_left_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.active_left_key = None;
        }
        if self
            .active_right_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.active_right_key = None;
        }
        if self
            .active_scrollbar
            .is_some_and(|drag| !self.box_table.contains_key(&drag.key))
        {
            self.active_scrollbar = None;
        }
        if self
            .hot_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.hot_key = None;
        }
        if self
            .focus_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.focus_key = None;
        }
        if self
            .next_focus_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.next_focus_key = None;
        }
    }
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

fn smooth_rate(rate: f32, dt: f32) -> f32 {
    (1.0 - 2.0_f32.powf(-rate.max(0.0) * dt.max(0.0))).clamp(0.0, 1.0)
}

fn animate_scalar(current: f32, target: f32, rate: f32, epsilon: f32) -> f32 {
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
    #[cfg(target_os = "macos")]
    {
        has_flag(flags, OSEventFlag::Super)
    }
    #[cfg(not(target_os = "macos"))]
    {
        has_flag(flags, OSEventFlag::Control)
    }
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

fn cursor_left(_text: &str, cursor: usize) -> usize {
    cursor.saturating_sub(1)
}

fn cursor_right(text: &str, cursor: usize) -> usize {
    (cursor + 1).min(char_count(text))
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

fn line_col_from_cursor(text: &str, cursor: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (idx, ch) in text.chars().enumerate() {
        if idx >= cursor {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn cursor_from_line_col(text: &str, target_line: usize, target_col: usize) -> usize {
    let mut line = 0;
    let mut col = 0;
    let mut cursor = 0;
    for ch in text.chars() {
        if line == target_line && col == target_col {
            return cursor;
        }
        if line == target_line && ch == '\n' {
            return cursor;
        }
        cursor += 1;
        if ch == '\n' {
            if line == target_line {
                return cursor - 1;
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    cursor
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
mod tests {
    use super::*;

    fn push_test_event(ui: &mut IMUI, ev: OSEvent) {
        ui.apply_event_side_effects(&ev);
        ui.events.push(ev);
    }

    fn build_vertical_scroll_pane(ui: &mut IMUI) -> UIBoxHandle {
        let pane = ui.named_column("###vertical_scroll_pane", |ui| {
            for idx in 0..10 {
                let label = ui.label(&format!("Row {idx}"));
                ui.height(label, UISize::Pixels(24.0));
            }
        });
        ui.width(pane, UISize::Pixels(120.0));
        ui.height(pane, UISize::Pixels(72.0));
        ui.scroll_y(pane, true);
        pane
    }

    fn build_horizontal_scroll_pane(ui: &mut IMUI) -> UIBoxHandle {
        let pane = ui.named_row("###horizontal_scroll_pane", |ui| {
            for idx in 0..6 {
                let label = ui.label(&format!("Column {idx}"));
                ui.width(label, UISize::Pixels(72.0));
                ui.height(label, UISize::Pixels(24.0));
            }
        });
        ui.width(pane, UISize::Pixels(140.0));
        ui.height(pane, UISize::Pixels(48.0));
        ui.scroll_x(pane, true);
        pane
    }

    #[test]
    fn built_in_themes_expose_distinct_light_and_dark_tokens() {
        let dark = UITheme::dark();
        let light = UITheme::light();

        assert_eq!(dark.kind, ThemeKind::Dark);
        assert_eq!(light.kind, ThemeKind::Light);
        assert!(color_distance(dark.app_bg, light.app_bg) > 0.2);
        assert!(dark.text.a > 0.0);
        assert!(light.text.a > 0.0);
        assert!(dark.accent.a > 0.0);
        assert!(light.accent.a > 0.0);
        assert_eq!(UITheme::for_kind(ThemeKind::Dark).kind, ThemeKind::Dark);
        assert_eq!(UITheme::for_kind(ThemeKind::Light).kind, ThemeKind::Light);
    }

    #[test]
    fn retained_box_hover_state_animates_toward_signal_target() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let first = ui.button("Hover###hover_button", None);
        ui.width(first, UISize::Pixels(120.0));
        ui.height(first, UISize::Pixels(40.0));
        ui.end_frame();
        assert_eq!(ui.boxes[first.idx()].hot_t, 0.0);

        ui.repaint_requested = false;
        ui.mouse = Some(Point::new(20.0, 20.0));
        ui.begin_frame();
        let second = ui.button("Hover###hover_button", None);
        ui.width(second, UISize::Pixels(120.0));
        ui.height(second, UISize::Pixels(40.0));
        ui.end_frame();

        let hot_t = ui.boxes[second.idx()].hot_t;
        assert!(hot_t > 0.0);
        assert!(hot_t < 1.0);
        assert!(ui.repaint_requested);
    }

    #[test]
    fn plain_icon_button_draws_only_clickable_icon_text() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let icon = ui.button_icon_plain("\u{e89c}###plain_icon", None);
        ui.end_frame();

        let icon_box = &ui.boxes[icon.idx()];
        assert!(icon_box.flags.contains(UIBoxFlags::CLICKABLE));
        assert!(icon_box.flags.contains(UIBoxFlags::DRAW_TEXT));
        assert!(!icon_box.flags.contains(UIBoxFlags::DRAW_BACKGROUND));
        assert!(!icon_box.flags.contains(UIBoxFlags::DRAW_BORDER));
        assert!(icon_box.style.font_icon);
        assert!(color_distance(icon_box.style.text_color, ui.theme.text_muted) < 0.001);
    }

    #[test]
    fn plain_icon_button_highlights_with_text_color_on_hover() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let first = ui.button_icon_plain("\u{e89c}###plain_icon_hover", None);
        ui.end_frame();
        assert!(
            color_distance(ui.boxes[first.idx()].style.text_color, ui.theme.text_muted) < 0.001
        );

        ui.mouse = Some(Point::new(8.0, 8.0));
        ui.begin_frame();
        let second = ui.button_icon_plain("\u{e89c}###plain_icon_hover", None);

        assert!(second.hover());
        assert!(
            color_distance(
                ui.boxes[second.idx()].style.text_color,
                ui.theme.accent_hover
            ) < 0.001
        );
        assert!(
            !ui.boxes[second.idx()]
                .flags
                .contains(UIBoxFlags::DRAW_BACKGROUND)
        );
        assert!(
            !ui.boxes[second.idx()]
                .flags
                .contains(UIBoxFlags::DRAW_BORDER)
        );
        ui.end_frame();
    }

    #[test]
    fn plain_icon_button_remains_clickable() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        ui.button_icon_plain("\u{e89c}###plain_icon_click", None);
        ui.end_frame();

        push_test_event(
            &mut ui,
            OSEvent::press(OSKey::LeftMouseButton, Some(Point::new(8.0, 8.0))),
        );
        push_test_event(
            &mut ui,
            OSEvent::release(OSKey::LeftMouseButton, Some(Point::new(8.0, 8.0))),
        );
        ui.begin_frame();
        let clicked = ui.button_icon_plain("\u{e89c}###plain_icon_click", None);

        assert!(clicked.clicked());
        assert!(
            color_distance(
                ui.boxes[clicked.idx()].style.text_color,
                ui.theme.accent_active
            ) < 0.001
        );
        ui.end_frame();
    }

    #[test]
    fn setting_same_theme_does_not_request_repaint() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.repaint_requested = false;
        ui.set_theme(UITheme::dark());
        assert!(!ui.repaint_requested);

        ui.set_theme(UITheme::light());
        assert!(ui.repaint_requested);
    }

    #[test]
    fn static_retained_frame_does_not_keep_lazy_rendering_awake() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let first = ui.button("Idle###idle_button", None);
        ui.width(first, UISize::Pixels(120.0));
        ui.height(first, UISize::Pixels(40.0));
        ui.end_frame();

        ui.repaint_requested = false;
        ui.begin_frame();
        let second = ui.button("Idle###idle_button", None);
        ui.width(second, UISize::Pixels(120.0));
        ui.height(second, UISize::Pixels(40.0));
        ui.end_frame();

        assert!(!ui.repaint_requested);
    }

    #[test]
    fn transient_visual_box_does_not_keep_lazy_rendering_awake() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        ui.floating_pane_at(Point::new(20.0, 20.0), None, |ui| {
            let label = ui.label("Transient");
            ui.padding_all(label, 4.0);
        });
        ui.end_frame();

        ui.repaint_requested = false;
        ui.begin_frame();
        let pane = ui.floating_pane_at(Point::new(20.0, 20.0), None, |ui| {
            let label = ui.label("Transient");
            ui.padding_all(label, 4.0);
        });
        ui.end_frame();

        assert!(pane.key().is_zero());
        assert!(ui.free_boxes.contains(&pane.idx()));
        assert!(!ui.repaint_requested);
    }

    #[test]
    fn key_string_display_and_hash_parts_match_rad_style() {
        assert_eq!(display_part_from_key_string("Save##toolbar"), "Save");
        assert_eq!(hash_part_from_key_string("Save##toolbar"), "Save##toolbar");
        assert_eq!(display_part_from_key_string("Save###stable"), "Save");
        assert_eq!(hash_part_from_key_string("Save###stable"), "###stable");
    }

    #[test]
    fn layout_resolves_children_sum_and_parent_pct() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);
        ui.begin_frame();
        let root_child = ui.column(|ui| {
            let a = ui.label("abc");
            ui.width(a, UISize::Pixels(100.0));
            ui.height(a, UISize::Pixels(20.0));
            let b = ui.label("def");
            ui.width(b, UISize::ParentPct(0.5));
            ui.height(b, UISize::Pixels(20.0));
        });
        ui.width(root_child, UISize::ParentPct(1.0));
        ui.height(root_child, UISize::ParentPct(1.0));
        ui.layout_root(ui.root);
        let children = ui.boxes[root_child.idx].children.clone();
        assert_eq!(ui.boxes[children[0]].computed_size.width, 100.0);
        assert_eq!(ui.boxes[children[1]].computed_size.width, 200.0);
    }

    #[test]
    fn text_content_size_reserves_draw_margin() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);
        ui.begin_frame();
        let label = ui.label("abc");
        ui.layout_root(ui.root);

        let (text_width, text_height) = ui.text_size(ui.boxes[label.idx()].style.font_size, "abc");
        let margin = ui.boxes[label.idx()].style.margin;

        assert!(ui.boxes[label.idx()].computed_size.width >= text_width + margin * 2.0);
        assert!(ui.boxes[label.idx()].computed_size.height >= text_height + margin * 2.0);
    }

    #[test]
    fn floating_boxes_do_not_affect_parent_flow_size_or_gaps() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let row = ui.row(|ui| {
            let a = ui.label("a");
            ui.width(a, UISize::Pixels(40.0));
            ui.height(a, UISize::Pixels(20.0));

            let floating = ui.floating_pane_at(Point::new(100.0, 100.0), Some("###float"), |ui| {
                let child = ui.label("floating");
                ui.width(child, UISize::Pixels(80.0));
                ui.height(child, UISize::Pixels(20.0));
            });
            ui.width(floating, UISize::Pixels(80.0));
            ui.height(floating, UISize::Pixels(20.0));

            let b = ui.label("b");
            ui.width(b, UISize::Pixels(50.0));
            ui.height(b, UISize::Pixels(20.0));
        });
        ui.gap(row, 10.0);
        ui.layout_root(ui.root);

        assert_eq!(ui.boxes[row.idx()].computed_size.width, 100.0);
    }

    #[test]
    fn fill_and_parent_pct_share_remaining_width_without_overflow() {
        let mut ui = IMUI::new_for_test(1000.0, 300.0);
        ui.begin_frame();
        let row = ui.row(|ui| {
            let left = ui.label("left");
            ui.width(left, UISize::Fill);
            ui.height(left, UISize::Pixels(20.0));

            let right = ui.label("right");
            ui.width(right, UISize::ParentPct(0.34));
            ui.height(right, UISize::Pixels(20.0));
        });
        ui.width(row, UISize::ParentPct(1.0));
        ui.height(row, UISize::Pixels(30.0));
        ui.padding_all(row, 10.0);
        ui.gap(row, 12.0);
        ui.layout_root(ui.root);

        let children = ui.boxes[row.idx()].children.clone();
        let left_w = ui.boxes[children[0]].computed_size.width;
        let right_w = ui.boxes[children[1]].computed_size.width;
        let available =
            ui.boxes[row.idx()].computed_size.width - ui.boxes[row.idx()].padding.horizontal();
        let used = left_w + right_w + ui.boxes[row.idx()].child_gap;

        assert!(
            used <= available + 0.01,
            "used={used} available={available}"
        );
        assert!(left_w > 0.0);
    }

    #[test]
    fn keyed_boxes_are_reused_across_consecutive_frames() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let first = ui.named_column("###stable", |_| {});
        ui.end_frame();

        ui.begin_frame();
        let second = ui.named_column("###stable", |_| {});
        ui.end_frame();

        assert_eq!(first.idx(), second.idx());
        assert_eq!(ui.box_table.get(&first.key()), Some(&first.idx()));
    }

    #[test]
    fn keyed_boxes_are_pruned_when_missing_for_a_frame() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let first = ui.named_column("###stable", |_| {});
        ui.end_frame();
        assert!(ui.box_table.contains_key(&first.key()));

        ui.begin_frame();
        ui.end_frame();

        assert!(!ui.box_table.contains_key(&first.key()));
        assert!(ui.free_boxes.contains(&first.idx()));
    }

    #[test]
    fn retained_button_consumes_press_and_release_events() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let first = ui.button("Click###button", None);
        ui.width(first, UISize::Pixels(120.0));
        ui.height(first, UISize::Pixels(40.0));
        ui.end_frame();

        ui.mouse = Some(Point::new(20.0, 20.0));
        ui.events = vec![
            OSEvent {
                ty: OSEventType::Press,
                key: OSKey::LeftMouseButton,
                pos: Some(Point::new(20.0, 20.0)),
                chars: None,
                delta: 0.0,
                flags: None,
            },
            OSEvent {
                ty: OSEventType::Release,
                key: OSKey::LeftMouseButton,
                pos: Some(Point::new(20.0, 20.0)),
                chars: None,
                delta: 0.0,
                flags: None,
            },
        ];

        ui.begin_frame();
        let second = ui.button("Click###button", None);
        ui.width(second, UISize::Pixels(120.0));
        ui.height(second, UISize::Pixels(40.0));

        assert!(second.clicked());
        assert!(ui.events.is_empty());
    }

    #[test]
    fn focused_line_edit_consumes_text_events() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);
        let mut buffer = String::new();

        ui.begin_frame();
        let edit = ui.line_edit("Edit###edit", &mut buffer, false);
        ui.width(edit, UISize::Pixels(120.0));
        ui.height(edit, UISize::Pixels(32.0));
        ui.end_frame();

        ui.focus_key = Some(edit.key());
        ui.events = vec![OSEvent {
            ty: OSEventType::Press,
            key: OSKey::Keyboard(OSKeyCode::KeyA),
            pos: None,
            chars: Some('a'),
            delta: 0.0,
            flags: None,
        }];

        ui.begin_frame();
        ui.line_edit("Edit###edit", &mut buffer, false);

        assert_eq!(buffer, "a");
        assert!(ui.events.is_empty());
    }

    #[test]
    fn line_edit_selects_text_with_mouse_drag() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);
        let mut buffer = "abcdef".to_string();

        ui.begin_frame();
        let edit = ui.line_edit("Edit###edit", &mut buffer, false);
        ui.width(edit, UISize::Pixels(220.0));
        ui.height(edit, UISize::Pixels(32.0));
        ui.end_frame();

        let rect = ui.boxes[edit.idx()].rect;
        let padding = ui.boxes[edit.idx()].padding;
        let style = ui.boxes[edit.idx()].style;
        let char_w = style.font_size * 0.6;
        let content_x = rect.x0 + padding.left + style.margin;
        let y = rect.y0 + rect.height() * 0.5;
        let start = Point::new(content_x + char_w * 1.2, y);
        let end = Point::new(content_x + char_w * 4.2, y);

        push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(start)));
        ui.begin_frame();
        let edit = ui.line_edit("Edit###edit", &mut buffer, false);
        ui.width(edit, UISize::Pixels(220.0));
        ui.height(edit, UISize::Pixels(32.0));
        ui.end_frame();

        let state = ui.text_edit_states.get(&edit.key()).unwrap();
        assert_eq!(state.cursor, 1);
        assert_eq!(state.selection_range(), None);
        assert_eq!(ui.focus_key, Some(edit.key()));

        push_test_event(&mut ui, OSEvent::mouse_move(end));
        ui.begin_frame();
        let edit = ui.line_edit("Edit###edit", &mut buffer, false);
        ui.width(edit, UISize::Pixels(220.0));
        ui.height(edit, UISize::Pixels(32.0));
        ui.end_frame();

        let state = ui.text_edit_states.get(&edit.key()).unwrap();
        assert_eq!(state.cursor, 4);
        assert_eq!(state.selection_range(), Some((1, 4)));

        push_test_event(&mut ui, OSEvent::release(OSKey::LeftMouseButton, Some(end)));
        ui.begin_frame();
        let edit = ui.line_edit("Edit###edit", &mut buffer, false);
        ui.width(edit, UISize::Pixels(220.0));
        ui.height(edit, UISize::Pixels(32.0));
        ui.end_frame();

        let state = ui.text_edit_states.get(&edit.key()).unwrap();
        assert_eq!(state.selection_range(), Some((1, 4)));
    }

    #[test]
    fn textarea_selects_text_with_mouse_drag_across_lines() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);
        let mut buffer = "abc\ndef".to_string();

        ui.begin_frame();
        let edit = ui.textarea("Text###text", &mut buffer);
        ui.width(edit, UISize::Pixels(220.0));
        ui.height(edit, UISize::Pixels(120.0));
        ui.end_frame();

        let rect = ui.boxes[edit.idx()].rect;
        let padding = ui.boxes[edit.idx()].padding;
        let style = ui.boxes[edit.idx()].style;
        let char_w = style.font_size * 0.6;
        let line_h = ui.theme.size_text + 6.0;
        let content_x = rect.x0 + padding.left + style.margin;
        let content_y = rect.y0 + padding.top + style.margin;
        let start = Point::new(content_x + char_w * 1.2, content_y + line_h * 0.5);
        let end = Point::new(content_x + char_w * 2.2, content_y + line_h * 1.5);

        push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(start)));
        ui.begin_frame();
        let edit = ui.textarea("Text###text", &mut buffer);
        ui.width(edit, UISize::Pixels(220.0));
        ui.height(edit, UISize::Pixels(120.0));
        ui.end_frame();

        let state = ui.text_edit_states.get(&edit.key()).unwrap();
        assert_eq!(state.cursor, 1);
        assert_eq!(state.selection_range(), None);
        assert_eq!(ui.focus_key, Some(edit.key()));

        push_test_event(&mut ui, OSEvent::mouse_move(end));
        ui.begin_frame();
        let edit = ui.textarea("Text###text", &mut buffer);
        ui.width(edit, UISize::Pixels(220.0));
        ui.height(edit, UISize::Pixels(120.0));
        ui.end_frame();

        let state = ui.text_edit_states.get(&edit.key()).unwrap();
        assert_eq!(state.cursor, 6);
        assert_eq!(state.selection_range(), Some((1, 6)));

        push_test_event(&mut ui, OSEvent::release(OSKey::LeftMouseButton, Some(end)));
        ui.begin_frame();
        let edit = ui.textarea("Text###text", &mut buffer);
        ui.width(edit, UISize::Pixels(220.0));
        ui.height(edit, UISize::Pixels(120.0));
        ui.end_frame();

        let state = ui.text_edit_states.get(&edit.key()).unwrap();
        assert_eq!(state.selection_range(), Some((1, 6)));
    }

    #[test]
    fn vertical_scrollbar_width_animates_on_hover() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let pane = build_vertical_scroll_pane(&mut ui);
        ui.end_frame();

        assert_eq!(
            ui.scrollbar_thickness(pane.idx(), Axis::Y),
            SCROLLBAR_THICKNESS
        );
        let thumb = ui
            .scrollbar_thumb_rect(pane.idx(), Axis::Y, SCROLLBAR_THICKNESS)
            .unwrap();
        ui.mouse = Some(Point::new(
            thumb.x0 + thumb.width() * 0.5,
            thumb.y0 + thumb.height() * 0.5,
        ));
        ui.repaint_requested = false;

        ui.begin_frame();
        let pane = build_vertical_scroll_pane(&mut ui);
        ui.end_frame();

        let thickness = ui.scrollbar_thickness(pane.idx(), Axis::Y);
        assert!(thickness > SCROLLBAR_THICKNESS);
        assert!(thickness < SCROLLBAR_HOVER_THICKNESS);
        assert!(ui.repaint_requested);

        for _ in 0..120 {
            ui.repaint_requested = false;
            ui.begin_frame();
            build_vertical_scroll_pane(&mut ui);
            ui.end_frame();
            if !ui.repaint_requested {
                break;
            }
        }
        assert_eq!(
            ui.scrollbar_thickness(pane.idx(), Axis::Y),
            SCROLLBAR_HOVER_THICKNESS
        );

        ui.mouse = Some(Point::new(300.0, 180.0));
        ui.repaint_requested = false;
        ui.begin_frame();
        let pane = build_vertical_scroll_pane(&mut ui);
        ui.end_frame();

        let thickness = ui.scrollbar_thickness(pane.idx(), Axis::Y);
        assert!(thickness > SCROLLBAR_THICKNESS);
        assert!(thickness < SCROLLBAR_HOVER_THICKNESS);
        assert!(ui.repaint_requested);
    }

    #[test]
    fn horizontal_scrollbar_height_animates_on_hover() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let pane = build_horizontal_scroll_pane(&mut ui);
        ui.end_frame();

        assert_eq!(
            ui.scrollbar_thickness(pane.idx(), Axis::X),
            SCROLLBAR_THICKNESS
        );
        let thumb = ui
            .scrollbar_thumb_rect(pane.idx(), Axis::X, SCROLLBAR_THICKNESS)
            .unwrap();
        ui.mouse = Some(Point::new(
            thumb.x0 + thumb.width() * 0.5,
            thumb.y0 + thumb.height() * 0.5,
        ));
        ui.repaint_requested = false;

        ui.begin_frame();
        let pane = build_horizontal_scroll_pane(&mut ui);
        ui.end_frame();

        let thickness = ui.scrollbar_thickness(pane.idx(), Axis::X);
        assert!(thickness > SCROLLBAR_THICKNESS);
        assert!(thickness < SCROLLBAR_HOVER_THICKNESS);
        assert!(ui.repaint_requested);

        for _ in 0..120 {
            ui.repaint_requested = false;
            ui.begin_frame();
            build_horizontal_scroll_pane(&mut ui);
            ui.end_frame();
            if !ui.repaint_requested {
                break;
            }
        }
        assert_eq!(
            ui.scrollbar_thickness(pane.idx(), Axis::X),
            SCROLLBAR_HOVER_THICKNESS
        );

        ui.mouse = Some(Point::new(300.0, 180.0));
        ui.repaint_requested = false;
        ui.begin_frame();
        let pane = build_horizontal_scroll_pane(&mut ui);
        ui.end_frame();

        let thickness = ui.scrollbar_thickness(pane.idx(), Axis::X);
        assert!(thickness > SCROLLBAR_THICKNESS);
        assert!(thickness < SCROLLBAR_HOVER_THICKNESS);
        assert!(ui.repaint_requested);
    }

    #[test]
    fn vertical_scrollbar_click_and_drag_updates_scroll() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let pane = build_vertical_scroll_pane(&mut ui);
        ui.end_frame();

        let thumb = ui
            .scrollbar_thumb_rect(pane.idx(), Axis::Y, SCROLLBAR_HOVER_THICKNESS)
            .unwrap();
        let start = Point::new(
            thumb.x0 + thumb.width() * 0.5,
            thumb.y0 + thumb.height() * 0.5,
        );
        let end = Point::new(start.x(), start.y() + 24.0);

        push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(start)));
        ui.begin_frame();
        let _pane = build_vertical_scroll_pane(&mut ui);
        ui.end_frame();
        assert!(ui.active_scrollbar.is_some());
        assert!(ui.events.is_empty());

        push_test_event(&mut ui, OSEvent::mouse_move(end));
        ui.begin_frame();
        let pane = build_vertical_scroll_pane(&mut ui);
        ui.end_frame();
        assert!(ui.boxes[pane.idx()].scroll.y > 0.0);
        assert_eq!(
            ui.boxes[pane.idx()].scroll.y,
            ui.boxes[pane.idx()].scroll_target.y
        );

        push_test_event(&mut ui, OSEvent::release(OSKey::LeftMouseButton, Some(end)));
        ui.begin_frame();
        build_vertical_scroll_pane(&mut ui);
        ui.end_frame();
        assert!(ui.active_scrollbar.is_none());
        assert!(ui.events.is_empty());
    }

    #[test]
    fn vertical_scrollbar_track_click_updates_scroll() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);

        ui.begin_frame();
        let pane = build_vertical_scroll_pane(&mut ui);
        ui.end_frame();

        let thumb = ui
            .scrollbar_thumb_rect(pane.idx(), Axis::Y, SCROLLBAR_HOVER_THICKNESS)
            .unwrap();
        let rect = ui.boxes[pane.idx()].rect;
        let click = Point::new(
            thumb.x0 + thumb.width() * 0.5,
            thumb.y1 + (rect.y1 - thumb.y1) * 0.5,
        );

        push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(click)));
        ui.begin_frame();
        let pane = build_vertical_scroll_pane(&mut ui);
        ui.end_frame();

        assert!(ui.boxes[pane.idx()].scroll.y > 0.0);
        assert_eq!(
            ui.boxes[pane.idx()].scroll.y,
            ui.boxes[pane.idx()].scroll_target.y
        );
        assert!(ui.events.is_empty());
    }

    #[test]
    fn topmost_box_wins_hovering_after_layout() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);
        ui.mouse = Some(Point::new(20.0, 20.0));

        ui.begin_frame();
        let base = ui.button("Base###base", None);
        ui.width(base, UISize::Pixels(120.0));
        ui.height(base, UISize::Pixels(40.0));

        let mut overlay_button = None;
        let overlay = ui.floating_pane_at(Point::new(0.0, 0.0), Some("###overlay"), |ui| {
            let button = ui.button("Overlay###overlay_button", None);
            ui.width(button, UISize::Pixels(120.0));
            ui.height(button, UISize::Pixels(40.0));
            overlay_button = Some(button);
        });
        ui.padding_all(overlay, 0.0);
        ui.gap(overlay, 0.0);

        ui.end_frame();

        let overlay_button = overlay_button.unwrap();
        assert!(ui.boxes[base.idx()].signal.mouse_over());
        assert!(!ui.boxes[base.idx()].signal.hovering());
        assert!(ui.boxes[overlay_button.idx()].signal.hovering());
    }

    #[test]
    fn overlay_box_wins_hovering_even_when_declared_before_normal_box() {
        let mut ui = IMUI::new_for_test(400.0, 200.0);
        ui.mouse = Some(Point::new(20.0, 20.0));

        ui.begin_frame();
        let mut overlay_button = None;
        let overlay = ui.floating_pane_at(Point::new(0.0, 0.0), Some("###overlay"), |ui| {
            let button = ui.button("Overlay###overlay_button", None);
            ui.width(button, UISize::Pixels(120.0));
            ui.height(button, UISize::Pixels(40.0));
            overlay_button = Some(button);
        });
        ui.padding_all(overlay, 0.0);
        ui.gap(overlay, 0.0);

        let base = ui.button("Base###base", None);
        ui.width(base, UISize::Pixels(120.0));
        ui.height(base, UISize::Pixels(40.0));
        ui.end_frame();

        let overlay_button = overlay_button.unwrap();
        assert!(ui.boxes[base.idx()].signal.mouse_over());
        assert!(!ui.boxes[base.idx()].signal.hovering());
        assert!(ui.boxes[overlay_button.idx()].signal.hovering());
    }
}
