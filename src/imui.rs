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
        Color, Padding, UIBox, UIBoxFlags as UIBoxFlag, UIBoxHandle, UIBoxParams, UIBoxStyle,
        UiSignal as UIBoxSignal, u64_hash_from_string,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseButton {
    Left,
    Right,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct UiSignal {
    pub flags: u32,
    pub scroll_x: i16,
    pub scroll_y: i16,
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
    string: Option<String>,
    display_string: Option<String>,
    flags: UIBoxFlags,
    pref_size: [UISize; 2],
    min_size: Size,
    fixed_position: Point,
    computed_size: Size,
    rect: RectCoords,
    child_layout_axis: Axis,
    padding: Padding,
    child_gap: f32,
    main_axis_align: MainAxisAlign,
    cross_axis_align: CrossAxisAlign,
    style: UIBoxStyle,
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
            string: string.clone(),
            display_string: string,
            flags,
            pref_size: [UISize::ChildrenSum, UISize::ChildrenSum],
            min_size: Size::default(),
            fixed_position: Point::default(),
            computed_size: Size::default(),
            rect: RectCoords::from_size(0.0, 0.0, 0.0, 0.0),
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
        let first_touched_frame = self.first_touched_frame;
        let last_touched_frame = self.last_touched_frame;

        *self = Self::new(key, flags, string, theme);
        self.rect = rect;
        self.computed_size = computed_size;
        self.first_touched_frame = first_touched_frame;
        self.last_touched_frame = last_touched_frame;
    }
}

pub struct UITheme {
    pub color_bg: Color,
    pub color_bg_popup: Color,
    pub color_main: Color,
    pub color_text: Color,
    pub size_text: f32,
}

impl Default for UITheme {
    fn default() -> Self {
        Self {
            color_bg: Color::new("#20242a"),
            color_bg_popup: Color::new("#2b3038"),
            color_main: Color::new("#2f8f83"),
            color_text: Color::new("#f0f3f5"),
            size_text: 14.0,
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
    focus_key: Option<UiKey>,
    next_focus_key: Option<UiKey>,
    cursor: OSCursor,

    boxes: Vec<UIBox>,
    box_table: HashMap<UiKey, usize>,
    free_boxes: Vec<usize>,
    frame_boxes: Vec<usize>,
    root: usize,
    parent_stack: Vec<usize>,
    build_index: u64,
    render_continuously: bool,
    vsync_enabled: bool,
    repaint_requested: bool,
    timer_frequency: f64,
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

    #[cfg(test)]
    fn new_for_test(w: f32, h: f32) -> Self {
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
            focus_key: None,
            next_focus_key: None,
            cursor: OSCursor::Arrow,
            boxes: Vec::new(),
            box_table: HashMap::new(),
            free_boxes: Vec::new(),
            frame_boxes: Vec::new(),
            root: 0,
            parent_stack: Vec::new(),
            build_index: 0,
            render_continuously: true,
            vsync_enabled: true,
            repaint_requested: true,
            timer_frequency: os::timer_init(),
            fps_window_start: 0.0,
            fps_frame_count: 0,
            fps: 0.0,
            theme,
        };
        if let Some(drawer) = ui.drawer.as_mut() {
            drawer.renderer.vsync(ui.vsync_enabled);
        }
        ui.fps_window_start = ui.now_seconds();
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

    fn begin_frame(&mut self) {
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
    }

    fn end_frame(&mut self) {
        self.layout_root(self.root);
        self.refresh_passive_signals();
        self.draw_ui_all();

        if let Some(drawer) = self.drawer.as_mut() {
            drawer.renderer.render_frame();
            drawer.renderer.win.set_cursor(self.cursor);
        }

        self.prune_boxes();
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

        for ev in &self.events {
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

    pub fn line_edit(&mut self, id: &str, buffer: &mut String, masked: bool) -> UIBoxHandle {
        let handle = self.alloc_box(Some(id), UIBoxFlags::LINE_EDIT);
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::Pixels(32.0)];
        self.boxes[handle.idx].padding = Padding::all(7.0);
        self.boxes[handle.idx].style.bg_color = Color::new("#15191f");
        self.boxes[handle.idx].style.border_color = Color::new("#3c4652");
        self.apply_click_to_focus(handle);
        if self.box_is_focused(handle) {
            self.apply_text_input(buffer, false);
            self.boxes[handle.idx].style.border_color = self.theme.color_main;
        }
        self.set_edit_display_text(handle, buffer, masked);
        handle
    }

    pub fn textarea(&mut self, id: &str, buffer: &mut String) -> UIBoxHandle {
        let handle = self.alloc_box(Some(id), UIBoxFlags::TEXTAREA);
        self.boxes[handle.idx].child_layout_axis = Axis::Y;
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::ParentPct(1.0)];
        self.boxes[handle.idx].padding = Padding::all(10.0);
        self.boxes[handle.idx].style.bg_color = Color::new("#15191f");
        self.boxes[handle.idx].style.border_color = Color::new("#303946");
        self.boxes[handle.idx].child_gap = 2.0;
        self.apply_click_to_focus(handle);
        if self.box_is_focused(handle) {
            self.apply_text_input(buffer, true);
            self.boxes[handle.idx].style.border_color = self.theme.color_main;
        }

        self.parent_stack.push(handle.idx);
        if buffer.is_empty() {
            let empty = self.label("");
            self.height(empty, UISize::Pixels(self.theme.size_text + 4.0));
        } else {
            let lines: Vec<String> = buffer.lines().map(str::to_string).collect();
            for (idx, line) in lines.iter().enumerate() {
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

    pub fn floating_pane_at(
        &mut self,
        pos: Point,
        id: Option<&str>,
        children: impl FnOnce(&mut IMUI),
    ) -> UIBoxHandle {
        let handle = self.alloc_box(id, UIBoxFlags::DRAW_BACKGROUND | UIBoxFlags::DRAW_BORDER);
        self.boxes[handle.idx].fixed_position = pos;
        self.boxes[handle.idx].flags |= UIBoxFlags::FLOATING_X | UIBoxFlags::FLOATING_Y;
        self.boxes[handle.idx].child_layout_axis = Axis::Y;
        self.boxes[handle.idx].pref_size = [UISize::ChildrenSum, UISize::ChildrenSum];
        self.parent_stack.push(handle.idx);
        children(self);
        self.parent_stack.pop();
        handle
    }

    pub fn width(&mut self, handle: UIBoxHandle, width: UISize) -> &mut Self {
        self.boxes[handle.idx].pref_size[axis_idx(Axis::X)] = width;
        self
    }

    pub fn height(&mut self, handle: UIBoxHandle, height: UISize) -> &mut Self {
        self.boxes[handle.idx].pref_size[axis_idx(Axis::Y)] = height;
        self
    }

    pub fn min_width(&mut self, handle: UIBoxHandle, width: f32) -> &mut Self {
        self.boxes[handle.idx].min_size.width = width;
        self
    }

    pub fn min_height(&mut self, handle: UIBoxHandle, height: f32) -> &mut Self {
        self.boxes[handle.idx].min_size.height = height;
        self
    }

    pub fn background(&mut self, handle: UIBoxHandle, color: Color) -> &mut Self {
        self.boxes[handle.idx].flags |= UIBoxFlags::DRAW_BACKGROUND;
        self.boxes[handle.idx].style.bg_color = color;
        self
    }

    pub fn text_color(&mut self, handle: UIBoxHandle, color: Color) -> &mut Self {
        self.boxes[handle.idx].style.text_color = color;
        self
    }

    pub fn border_color(&mut self, handle: UIBoxHandle, color: Color) -> &mut Self {
        self.boxes[handle.idx].flags |= UIBoxFlags::DRAW_BORDER;
        self.boxes[handle.idx].style.border_color = color;
        self
    }

    pub fn padding_all(&mut self, handle: UIBoxHandle, value: f32) -> &mut Self {
        self.boxes[handle.idx].padding = Padding::all(value);
        self
    }

    pub fn gap(&mut self, handle: UIBoxHandle, value: f32) -> &mut Self {
        self.boxes[handle.idx].child_gap = value;
        self
    }

    pub fn align(
        &mut self,
        handle: UIBoxHandle,
        main: MainAxisAlign,
        cross: CrossAxisAlign,
    ) -> &mut Self {
        self.boxes[handle.idx].main_axis_align = main;
        self.boxes[handle.idx].cross_axis_align = cross;
        self
    }

    pub fn align_main(&mut self, handle: UIBoxHandle, main: MainAxisAlign) -> &mut Self {
        self.boxes[handle.idx].main_axis_align = main;
        self
    }

    pub fn align_cross(&mut self, handle: UIBoxHandle, cross: CrossAxisAlign) -> &mut Self {
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
        self.boxes[handle.idx].style.bg_color = Color::new("#323842");
        self.boxes[handle.idx].style.border_color = Color::new("#48515d");
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
        self.background(tooltip, Color::new("#111418f0"));
        self.padding_all(tooltip, 4.0);
    }

    fn apply_click_to_focus(&mut self, handle: UIBoxHandle) {
        if self.boxes[handle.idx].flags.click_to_focus() && handle.clicked() {
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

    fn apply_text_input(&mut self, buffer: &mut String, multiline: bool) {
        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            if ev.ty != OSEventType::Press {
                ev_idx += 1;
                continue;
            }
            let mut taken = true;
            match ev.key {
                OSKey::Keyboard(OSKeyCode::KeyBackspace) => {
                    buffer.pop();
                }
                OSKey::Keyboard(OSKeyCode::KeyEnter) if multiline => {
                    buffer.push('\n');
                }
                OSKey::Keyboard(OSKeyCode::KeyEscape) => {
                    self.focus_key = None;
                }
                _ => {
                    if let Some(c) = ev.chars {
                        if !c.is_ascii_control() {
                            buffer.push(c);
                        } else {
                            taken = false;
                        }
                    } else {
                        taken = false;
                    }
                }
            }
            if taken {
                self.events.remove(ev_idx);
            } else {
                ev_idx += 1;
            }
        }
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

        if let Some(parent_idx) = parent_idx {
            self.boxes[parent_idx].children.push(idx);
        }
        self.frame_boxes.push(idx);
        UIBoxHandle { idx, key, signal }
    }

    fn signal_from_key_and_flags(
        &mut self,
        key: UiKey,
        flags: UIBoxFlags,
        existing_idx: Option<usize>,
    ) -> UiSignal {
        let mut signal = UiSignal::default();
        let rect = existing_idx
            .map(|idx| self.boxes[idx].rect)
            .unwrap_or_else(|| RectCoords::from_size(-10000.0, -10000.0, 0.0, 0.0));
        let mouse_over = point_in_rect(&rect, self.mouse);
        let focused = self.focus_key == Some(key);

        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            let in_bounds = point_in_rect(&rect, ev.pos);
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
                    signal.scroll_y += ev.delta as i16;
                    taken = true;
                }
                if flags.scrolls_x() {
                    signal.scroll_x += ev.delta as i16;
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
            self.position(root, axis);
        }
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
                let (w, h) = self.text_size(font_size, &text);
                match axis {
                    Axis::X => w + padding + self.boxes[idx].padding.horizontal(),
                    Axis::Y => h.max(font_size) + padding + self.boxes[idx].padding.vertical(),
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
                size += self.boxes[*child].computed_size.axis(axis);
            }
            if self.boxes[idx].children.len() > 1 {
                size += self.boxes[idx].child_gap * (self.boxes[idx].children.len() - 1) as f32;
            }
        } else {
            for child in &self.boxes[idx].children {
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
        match self.boxes[idx].pref_size[axis_idx(axis)] {
            UISize::ParentPct(pct) => self.boxes[idx]
                .computed_size
                .set_axis(axis, (parent_content * pct).max(0.0)),
            UISize::Fill => self.boxes[idx]
                .computed_size
                .set_axis(axis, parent_content.max(0.0)),
            _ => {}
        }
        let children = self.boxes[idx].children.clone();
        for child in children {
            self.calc_downwards(child, axis);
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
        let children = &self.boxes[parent].children;
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
        pos
    }

    fn position_on_cross_axis(&self, idx: usize, parent: usize, axis: Axis) -> f32 {
        let padding_start = self.boxes[parent].padding.min_axis(axis);
        let padding_end = self.boxes[parent].padding.max_axis(axis);
        let base = self.boxes[parent].rect_axis_min(axis) + padding_start;
        let available =
            (self.boxes[parent].computed_size.axis(axis) - padding_start - padding_end).max(0.0);
        let child_size = self.boxes[idx].computed_size.axis(axis);
        match self.boxes[parent].cross_axis_align {
            CrossAxisAlign::Start | CrossAxisAlign::Stretch => base,
            CrossAxisAlign::Center => base + (available - child_size).max(0.0) / 2.0,
            CrossAxisAlign::End => base + (available - child_size).max(0.0),
        }
    }

    fn distribute_fill_children(&mut self, parent: usize, axis: Axis) {
        if self.boxes[parent].child_layout_axis != axis {
            return;
        }
        let fill_children: Vec<usize> = self.boxes[parent]
            .children
            .iter()
            .copied()
            .filter(|idx| self.boxes[*idx].pref_size[axis_idx(axis)] == UISize::Fill)
            .collect();
        if fill_children.is_empty() {
            return;
        }
        let padding = self.boxes[parent].padding.axis(axis);
        let gaps = self.boxes[parent].child_gap
            * self.boxes[parent].children.len().saturating_sub(1) as f32;
        let fixed: f32 = self.boxes[parent]
            .children
            .iter()
            .filter(|idx| self.boxes[**idx].pref_size[axis_idx(axis)] != UISize::Fill)
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
        let children = &self.boxes[parent].children;
        let mut total: f32 = children
            .iter()
            .map(|child| self.boxes[*child].computed_size.axis(axis))
            .sum();
        if children.len() > 1 {
            total += self.boxes[parent].child_gap * (children.len() - 1) as f32;
        }
        total
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
        self.draw_ui_root(self.root);
    }

    fn draw_ui_root(&mut self, idx: usize) {
        if !self.boxes[idx].visible {
            return;
        }
        let rect = self.boxes[idx].rect;
        let flags = self.boxes[idx].flags;
        let signal = self.boxes[idx].signal;
        let style = self.boxes[idx].style;
        if flags.contains(UIBoxFlags::DRAW_BACKGROUND) {
            let mut color = style.bg_color;
            if flags.contains(UIBoxFlags::DRAW_HOT_EFFECTS) && signal.hovering() {
                color.r = (color.r + 0.08).min(1.0);
                color.g = (color.g + 0.08).min(1.0);
                color.b = (color.b + 0.08).min(1.0);
            }
            self.drawer.as_mut().unwrap().draw_rect(&rect, color);
        }
        if flags.contains(UIBoxFlags::DRAW_BORDER) {
            self.drawer.as_mut().unwrap().draw_empty_rect(
                &rect,
                style.border_color,
                style.border_size,
            );
        }
        if flags.contains(UIBoxFlags::DRAW_TEXT) {
            if let Some(text) = self.boxes[idx].display_string.clone() {
                let padding = self.boxes[idx].padding;
                self.drawer.as_mut().unwrap().draw_text(
                    rect.x0 + padding.left + style.margin,
                    rect.y0 + padding.top + style.margin,
                    style.font_size,
                    &text,
                    text.len(),
                    rect.x1 - padding.right,
                    rect.y1 - padding.bottom,
                    style.text_color,
                    false,
                    style.font_icon,
                );
            }
        }
        let children = self.boxes[idx].children.clone();
        for child in children {
            self.draw_ui_root(child);
        }
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

        let frame_boxes = self.frame_boxes.clone();
        let mut hover_candidate = None;

        for &idx in &frame_boxes {
            self.boxes[idx].signal.flags &=
                !(UiSignal::MOUSE_OVER | UiSignal::HOVERING | UiSignal::LEFT_DRAGGING);
        }

        for &idx in &frame_boxes {
            let rect = self.clipped_rect(idx);
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
                OSCursor::Hand
            };
        }
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
}
