use std::{cell::RefCell, rc::Rc};

#[cfg(target_os = "android")]
use android_activity::AndroidApp;
use gl::COLOR;

use crate::{
    draw::{self, Drawer},
    os::{self, OSEvent, OSEventType, OSKey, OSKeyCode},
    render::{self, RectCoords, V4f32},
};

pub mod color {
    pub const NONE: crate::render::V4f32 = crate::render::V4f32 {
        r: 0.,
        g: 0.,
        b: 0.,
        a: 0.,
    };
}

pub type Color = V4f32;
impl Color {
    pub fn from_text(text: &str) -> Self {
        if text.len() < 4 {
            Color {
                r: 1.,
                g: 1.,
                b: 1.,
                a: 1.,
            }
        } else if text.len() == 4 && text.as_bytes()[0] == b'#' {
            let bytes = text.as_bytes();
            let mut vals: [f32; 3] = [0., 0., 0.];
            for i in 0..3 {
                let b = bytes[1 + i];
                let mut val = 0;
                if b >= b'0' && b <= b'9' {
                    val = b - b'0';
                } else if b >= b'a' && b <= b'f' {
                    val = b - b'a' + 10;
                } else if b >= b'A' && b <= b'F' {
                    val = b - b'A' + 10;
                }
                vals[i] = val as f32 / 16.;
            }
            Color {
                r: vals[0],
                g: vals[1],
                b: vals[2],
                a: 1.,
            }
        } else {
            Color {
                r: 1.,
                g: 1.,
                b: 1.,
                a: 1.,
            }
        }
    }
}

pub(crate) type Point = (f32, f32);
type UIWidgetRef = Rc<RefCell<UIWidget>>;

#[repr(u64)]
enum UIWidgetFlag {
    MouseClickable = 1u64,
}

#[repr(u64)]
enum UIWidgetEvent {
    MouseOver = 1u64,
    MouseClicked = 2u64,
    MouseReleased = 4u64,
}

pub enum UITextAlign {
    Left,
    Center,
}

#[derive(Clone, Copy)]
pub enum UISize {
    Pixels(f32),
    Percents(f32),
}
impl UISize {
    pub fn from_str(input: &str) -> Self {
        let val = match i32::from_str_radix(input, 10) {
            Ok(r) => r as f32,
            Err(_) => 0.,
        };
        UISize::Pixels(val)
    }
    pub fn pixels(&self, parent_val: f32) -> f32 {
        match self {
            UISize::Pixels(val) => *val,
            UISize::Percents(val) => val * parent_val,
        }
    }
}

pub enum UIPosition {
    Relative(UISize, UISize),
    Fixed(UISize, UISize),
}

pub enum UILayout {
    Default,
}

pub struct UIWidget {
    bounds: RectCoords,
    parent: Option<UIWidgetRef>,
    children: Vec<UIWidgetRef>,

    // event flags
    flags: u64,
    events: u64,
}
impl UIWidget {
    pub fn hover(&self) -> bool {
        (self.events & UIWidgetEvent::MouseOver as u64) > 0
    }
    pub fn click(&self) -> bool {
        (self.events & UIWidgetEvent::MouseClicked as u64) > 0
    }
    pub fn clicked(&self) -> bool {
        (self.events & UIWidgetEvent::MouseReleased as u64) > 0
    }

    fn clickable(&self) -> bool {
        (self.flags & UIWidgetFlag::MouseClickable as u64) > 0
    }
}

pub struct UIWidgetParams {
    default_parent: UIWidgetRef,
    parent: UIWidgetRef,
    width: UISize,
    height: UISize,
    color: V4f32,
    position: UIPosition,
    layout: UILayout,
    text_align: UITextAlign,
}
impl UIWidgetParams {
    pub fn new(parent: UIWidgetRef) -> Self {
        UIWidgetParams {
            default_parent: parent.clone(),
            parent,
            width: UISize::Percents(1.),
            height: UISize::Percents(1.),
            color: V4f32 {
                r: 1.,
                g: 1.,
                b: 1.,
                a: 1.,
            },
            position: UIPosition::Relative(UISize::Pixels(0.), UISize::Pixels(0.)),
            layout: UILayout::Default,
            text_align: UITextAlign::Left,
        }
    }
    pub fn size(&mut self, w: UISize, h: UISize) -> &mut Self {
        self.width(w);
        self.height(h);
        self
    }
    pub fn width(&mut self, w: UISize) -> &mut Self {
        self.width = w;
        self
    }
    pub fn height(&mut self, h: UISize) -> &mut Self {
        self.height = h;
        self
    }
    pub fn parent(&mut self, parent: UIWidgetRef) -> &mut Self {
        self.parent = parent;
        self
    }
    pub fn color(&mut self, color: V4f32) -> &mut Self {
        self.color = color;
        self
    }
    pub fn position(&mut self, pos: UIPosition) -> &mut Self {
        self.position = pos;
        self
    }
    pub fn layout(&mut self, layout: UILayout) -> &mut Self {
        self.layout = layout;
        self
    }
    pub fn text_align(&mut self, mode: UITextAlign) -> &mut Self {
        self.text_align = mode;
        self
    }
    pub fn reset(&mut self) {
        self.parent = self.default_parent.clone();
        self.width = UISize::Percents(1.);
        self.height = UISize::Percents(1.);
        self.color = V4f32 {
            r: 1.,
            g: 1.,
            b: 1.,
            a: 1.,
        };
        self.position = UIPosition::Relative(UISize::Pixels(0.), UISize::Pixels(0.));
        self.layout = UILayout::Default;
        self.text_align = UITextAlign::Left;
    }
}

#[derive(Default)]
struct IMUIEvents {
    events: Vec<OSEvent>,
    //// input events cache
    mouse: Option<Point>,
    click: Option<Point>,
    release: Option<Point>,
}

#[cfg(debug_assertions)]
struct IMUIDebug {
    fps: bool,
    hints: bool,
    target: Option<UIWidgetRef>,
}
#[cfg(debug_assertions)]
impl IMUIDebug {
    pub fn default() -> Self {
        IMUIDebug {
            fps: true,
            hints: false,
            target: None,
        }
    }
}

struct IMUITextInputState {
    focus: String,
    buffer: Rc<RefCell<String>>,
    idx: usize,
}

pub struct IMUI {
    pub root: UIWidgetRef,
    pub drawer: Drawer,
    #[cfg(debug_assertions)]
    debug: IMUIDebug,
    size: (f32, f32),
    params: UIWidgetParams,
    event: IMUIEvents,
    text_input_state: Option<IMUITextInputState>,
}
impl IMUI {
    #[cfg(not(target_os = "android"))]
    pub fn new(w: u32, h: u32) -> Self {
        let window = os::Window::new(w, h);
        IMUI::new_body(window)
    }
    #[cfg(target_os = "android")]
    pub fn android(app: AndroidApp) -> Self {
        let win = os::Window::new(app);

        // xarkes: wait for InitWindow to initialize the renderer
        win.wait_for_native_window();

        IMUI::new_body(win)
    }
    fn new_body(window: os::Window) -> Self {
        let renderer = render::Renderer::new(window);
        let drawer = draw::Drawer::new(renderer);

        let root = Rc::new(RefCell::new(UIWidget {
            bounds: RectCoords::from_size(0., 0., 1024., 768.),
            parent: None,
            children: Vec::new(),
            flags: 0,
            events: 0,
        }));
        IMUI {
            root: root.clone(),
            drawer,
            #[cfg(debug_assertions)]
            debug: IMUIDebug::default(),
            size: (0., 0.),
            params: UIWidgetParams::new(root),
            event: IMUIEvents::default(),
            text_input_state: None,
        }
    }
    pub fn eventloop(&mut self, mut drawfunction: impl FnMut(&mut IMUI)) {
        let freq = os::timer_init();
        let mut time = 0f64;
        let mut start = os::timer_value();
        loop {
            // xarkes: handle events
            let w: f32;
            {
                self.get_events();
                (w, _) = self.resize();
            }

            // xarkes: draw interface
            {
                drawfunction(self);
            }

            #[cfg(debug_assertions)]
            self.draw_debug_pane();

            // xarkes: draw and update FPS counter
            #[cfg(debug_assertions)]
            if self.debug.fps {
                let fps = 1f64 / time * 1000f64;
                let text = format!("{:.2}ms - {}fps", time, fps as u64);
                let font_size = 12;
                self.drawer.draw_text(
                    w - (text.len() as f32 * font_size as f32 / 1.6),
                    0.0,
                    font_size,
                    text.as_str(),
                    text.len(),
                    draw::color::FPS,
                );
                let end = os::timer_value();
                time = (end - start) as f64 * 1_000_000.0 / freq;
                start = end;
            }

            // xarkes: render
            {
                self.drawer.renderer.render_frame();
            }
        }
    }

    #[cfg(debug_assertions)]
    fn draw_debug_pane(&mut self) {
        self.params.reset();
        let box_width = 200.;
        // TODO(xarkes): Problem: How do we define the height...
        // a) do a "pre-render" that will compute all layout
        // b) ..?
        self.params
            .position(UIPosition::Fixed(
                UISize::Pixels(self.size.0 - box_width),
                UISize::Pixels(40.),
            ))
            .parent(self.root.clone())
            .size(UISize::Pixels(box_width), UISize::Pixels(80.))
            .color(color_rgb(40, 60, 140));
        let uibox = self.widget();
        self.params.reset();
        self.params
            .parent(uibox.clone())
            .text_align(UITextAlign::Center);
        self.label("Debugging panel");

        // Debug checkbox
        let mut box_checked = self.debug.hints;
        self.checkbox(&mut box_checked);
        if box_checked != self.debug.hints {
            self.debug.hints = box_checked;
        }
        // TODO(xarkes): This sucks, you have to define better alternatives
        self.params
            .position(UIPosition::Relative(
                UISize::Pixels(25.),
                UISize::Pixels(-20.),
            ))
            .text_align(UITextAlign::Left);
        self.label("Show debug hints");

        // FPS checkbox
        let mut box_checked = self.debug.fps;
        self.params
            .parent(uibox.clone())
            .position(UIPosition::Relative(UISize::Pixels(0.), UISize::Pixels(0.)));
        self.checkbox(&mut box_checked);
        if box_checked != self.debug.fps {
            self.debug.fps = box_checked;
        }
        // TODO(xarkes): This sucks, you have to define better alternatives
        self.params
            .position(UIPosition::Relative(
                UISize::Pixels(25.),
                UISize::Pixels(-20.),
            ))
            .text_align(UITextAlign::Left);
        self.label("Show FPS");

        // Target element
        self.params
            .position(UIPosition::Relative(UISize::Pixels(0.), UISize::Pixels(4.)));
        if let Some(target) = &self.debug.target {
            // XXX(xarkes): this is stupid, you wont get any update on the element as it is recreated each time...
            let txt = format!(
                "{}x{}",
                target.borrow().bounds.width(),
                target.borrow().bounds.height()
            );
            self.label(txt.as_str());
        } else {
            self.label("<No element selected>");
        }
    }

    fn create_ui_widget(&mut self, flags: u64) -> UIWidgetRef {
        // xarkes: apply layout properties and compute bounds
        let bounds = self.compute_layout_bounds();
        let mut w = UIWidget {
            parent: Some(self.params.parent.clone()),
            bounds,
            children: Vec::new(),
            flags,
            events: 0,
        };

        // xarkes: apply events flags
        if point_in_rect(&w.bounds, self.event.mouse) {
            w.events |= UIWidgetEvent::MouseOver as u64;
        }
        if point_in_rect(&w.bounds, self.event.click) && w.clickable() {
            w.events |= UIWidgetEvent::MouseClicked as u64;
        } else if point_in_rect(&w.bounds, self.event.release) && w.clickable() {
            w.events |= UIWidgetEvent::MouseReleased as u64;
            // xarkes: consume the release so clicked() triggers only once
            self.event.release = None;
        }

        // xarkes: update child and parent relationships
        let childref = Rc::new(RefCell::new(w));
        self.params
            .parent
            .borrow_mut()
            .children
            .push(childref.clone());

        #[cfg(debug_assertions)]
        self.draw_bounds(&childref.borrow());
        #[cfg(debug_assertions)]
        if point_in_rect(&childref.borrow().bounds, self.event.click) {
            self.debug.target = Some(childref.clone());
        }
        childref
    }

    /////////////////////////////////
    //// Styling and layout
    fn compute_layout_bounds(&self) -> RectCoords {
        let parent = self.params.parent.borrow();
        let previous = parent.children.last();
        let (x, y) = match &self.params.position {
            UIPosition::Relative(x, y) => {
                let (mx, my) = match previous {
                    Some(previous) => {
                        let prev_child_bounds = previous.borrow().bounds;
                        // layout LTR_TopToBottom
                        (
                            x.pixels(parent.bounds.x0),
                            y.pixels(parent.bounds.y0) + (prev_child_bounds.y1 - parent.bounds.y0),
                        )
                    }
                    None => (x.pixels(parent.bounds.x0), y.pixels(parent.bounds.y0)),
                };
                (parent.bounds.x0 + mx, parent.bounds.y0 + my)
            }
            UIPosition::Fixed(x, y) => {
                (x.pixels(parent.bounds.x0), y.pixels(parent.bounds.height()))
            }
        };

        let w = self.params.width.pixels(parent.bounds.width());
        let h = self.params.height.pixels(parent.bounds.height());
        let rect = RectCoords::from_size(x, y, w, h);

        // xarkes: assert that bounds are properly computed
        // debug_assert!(rect.x0 >= 0.);
        // debug_assert!(rect.y0 >= 0.);
        // debug_assert!(rect.x0 <= self.size.0);
        // debug_assert!(rect.y0 <= self.size.1);
        // debug_assert!(rect.x1 >= 0.);
        // debug_assert!(rect.y1 >= 0.);
        // debug_assert!(rect.x1 <= self.size.0);
        // debug_assert!(rect.y1 <= self.size.1);
        rect
    }
    pub fn params<'a>(&'a mut self) -> &'a mut UIWidgetParams {
        self.params.reset();
        &mut self.params
    }
    pub fn rparams(&mut self) -> &mut UIWidgetParams {
        &mut self.params
    }

    /////////////////////////////////
    //// UI widgets
    #[cfg(debug_assertions)]
    fn draw_bounds(&mut self, widget: &UIWidget) {
        if self.debug.hints {
            let color = match widget.hover() {
                true => color_rgb(0, 255, 0),
                false => color_rgb(255, 0, 0),
            };
            let border_width = match widget.hover() {
                true => 3.,
                false => 1.,
            };
            self.drawer
                .draw_empty_rect(&widget.bounds, color, border_width, true);
            if widget.hover() {
                let txt = format!("{:.2}px", widget.bounds.x1 - widget.bounds.x0);
                let len = self.drawer.get_text_size(12, txt.as_str(), txt.len());
                let font_size = 12.;
                let y = match widget.bounds.y0 < font_size {
                    true => widget.bounds.y0 + font_size,
                    false => widget.bounds.y0 - font_size,
                };
                self.drawer.draw_text(
                    widget.bounds.x0 + (widget.bounds.x1 - widget.bounds.x0 - len.0) / 2.,
                    y,
                    12 as u32,
                    txt.as_str(),
                    txt.len(),
                    color,
                );
                let txt = format!("{:.2}px", widget.bounds.y1 - widget.bounds.y0);
                let len = self.drawer.get_text_size(12, txt.as_str(), txt.len());
                let x = match widget.bounds.x0 < len.0 {
                    true => widget.bounds.x0 + border_width,
                    false => widget.bounds.x0 - len.0,
                };
                self.drawer.draw_text(
                    x,
                    widget.bounds.y0 + (widget.bounds.y1 - widget.bounds.y0) / 2. - len.1,
                    12 as u32,
                    txt.as_str(),
                    txt.len(),
                    color,
                );
            }
        }
    }
    pub fn widget(&mut self) -> UIWidgetRef {
        let widget = self.create_ui_widget(0);
        self.drawer
            .draw_rect(&widget.borrow().bounds, self.params.color);
        self.draw_bounds(&widget.borrow());
        widget
    }
    pub fn checkbox(&mut self, state: &mut bool) -> UIWidgetRef {
        // XXX(xarkes): Just a hack for now, think better about this
        let oldwidth = self.params.width;
        let oldheight = self.params.height;
        self.params.width = UISize::Pixels(20.);
        self.params.height = UISize::Pixels(20.);
        let checkbox = self.create_ui_widget(UIWidgetFlag::MouseClickable as u64);
        self.params.width = oldwidth;
        self.params.height = oldheight;

        self.drawer
            .draw_rect(&checkbox.borrow().bounds, color_rgb(255, 255, 255));
        self.drawer
            .draw_empty_rect(&checkbox.borrow().bounds, color_rgb(0, 0, 0), 1., false);
        if checkbox.borrow().clicked() {
            *state = !*state;
        }
        if *state {
            self.drawer.draw_rect(
                &RectCoords::from_size(
                    &checkbox.borrow().bounds.x0 + 2.,
                    &checkbox.borrow().bounds.y0 + 2.,
                    16.,
                    16.,
                ),
                color_rgb(0, 0, 80),
            );
        }
        if checkbox.borrow().hover() {
            self.drawer.draw_empty_rect(
                &RectCoords::from_size(
                    &checkbox.borrow().bounds.x0 + 1.,
                    &checkbox.borrow().bounds.y0 + 1.,
                    18.,
                    18.,
                ),
                // color_rgb(30, 30, 140),
                color_rgb(255, 30, 140),
                2.,
                false,
            );
        }
        self.draw_bounds(&checkbox.borrow());
        checkbox
    }
    pub fn line_edit(&mut self, text_buffer: Rc<RefCell<String>>, id: &str) -> UIWidgetRef {
        let widget = self.create_ui_widget(UIWidgetFlag::MouseClickable as u64);
        let font_size = 12.;
        // XXX: Only support monospace font atm, should add compute method
        let char_width = 7.;
        if widget.borrow().clicked() {
            // xarkes: update the text input global state
            let idx =
                ((self.event.mouse.unwrap().0 - widget.borrow().bounds.x0) / (char_width)) as usize;
            let idx = std::cmp::min(idx, text_buffer.borrow().len());
            self.text_input_state = Some(IMUITextInputState {
                focus: String::from(id),
                buffer: text_buffer.clone(),
                idx,
            });
        }

        // background
        let bg_color = color_rgb(200, 200, 200);
        self.drawer.draw_rect(&widget.borrow().bounds, bg_color);

        // text
        self.drawer.draw_text(
            widget.borrow().bounds.x0,
            widget.borrow().bounds.y0,
            12,
            text_buffer.borrow().as_str(),
            text_buffer.borrow().len(),
            color_rgb(0, 0, 0),
        );

        // cursor
        let show_cursor = match &self.text_input_state {
            Some(state) => state.focus.eq(id),
            None => false,
        };
        if show_cursor {
            // XXX: We assume here monospace font
            let cursorx = widget.borrow().bounds.x0
                + char_width * self.text_input_state.as_ref().unwrap().idx as f32;
            let cursory = widget.borrow().bounds.y0;
            self.drawer.draw_rect(
                &RectCoords::from_size(cursorx, cursory, 2., font_size + 4.),
                Color::from_text("#111"),
            );
        }

        widget
    }
    pub fn button(&mut self, label: Option<&str>) -> UIWidgetRef {
        let button = self.create_ui_widget(UIWidgetFlag::MouseClickable as u64);
        {
            let uibox = button.borrow();
            let bg_color = match uibox.hover() {
                false => self.params.color,
                true => V4f32 {
                    r: self.params.color.r * 1.1,
                    g: self.params.color.g * 1.1,
                    b: self.params.color.b * 1.1,
                    a: self.params.color.a,
                },
            };
            let draw_off = match uibox.click() {
                false => 0.,
                true => 1.,
            };
            self.drawer.draw_rect(
                &RectCoords {
                    x0: uibox.bounds.x0 + draw_off,
                    y0: uibox.bounds.y0 + draw_off,
                    x1: uibox.bounds.x1 + draw_off,
                    y1: uibox.bounds.y1 + draw_off,
                },
                bg_color,
            );
            if let Some(label) = label {
                self.drawer.draw_text(
                    uibox.bounds.x0 + draw_off,
                    uibox.bounds.y0 + draw_off,
                    12,
                    label,
                    label.len(),
                    draw::color::WHITE,
                );
            }
        }
        self.draw_bounds(&button.borrow());
        button
    }
    pub fn label(&mut self, label: &str) -> UIWidgetRef {
        let label_size = self.drawer.get_text_size(12, label, label.len());

        // XXX(xarkes): Just a hack for now, think better about this
        let oldwidth = self.params.width;
        let oldheight = self.params.height;
        self.params.width = UISize::Pixels(label_size.0);
        self.params.height = UISize::Pixels(self.drawer.renderer.font_cache.line_height(12.));
        let widget = self.create_ui_widget(0);
        self.params.width = oldwidth;
        self.params.height = oldheight;

        let widget_copy = widget.clone();
        let widget_ref = widget_copy.borrow();
        let parent_ref = widget_ref.parent.as_ref().unwrap();
        let parent_bounds = parent_ref.borrow().bounds;
        // TODO(xarkes): I think the text alignment should be handled by create_ui_widget
        self.drawer.draw_text(
            match self.params.text_align {
                UITextAlign::Left => widget.borrow().bounds.x0,
                UITextAlign::Center => {
                    widget.borrow().bounds.x0 + parent_bounds.width() / 2. - label_size.0 / 2.
                }
            },
            widget.borrow().bounds.y0,
            12,
            label,
            label.len(),
            self.params.color,
        );
        self.draw_bounds(&widget.borrow());
        widget
    }

    /////////////////////////////////
    //// Events related functions
    fn consume_events(&mut self) {
        for ev in &self.event.events {
            if ev.ty == OSEventType::MouseMove {
                self.event.mouse = ev.pos;
            } else if ev.ty == OSEventType::Press && ev.key == OSKey::LeftMouseButton {
                self.event.click = ev.pos;
                // xarkes: when there is a click anywhere, reset the text input global state
                // if the click happens to be on something clickable, then the widget will handle it,
                // if not this resets the current state
                self.text_input_state = None;
            } else if ev.ty == OSEventType::Release && ev.key == OSKey::LeftMouseButton {
                self.event.click = None;
                self.event.release = ev.pos;
            }

            // xarkes: consume global keyboard events
            if ev.ty == OSEventType::Press {
                // if ev.key >= OSKey::KeyA && ev.key <= OSKey::KeyZ {}
                if let Some(textinput) = self.text_input_state.as_mut() {
                    match &ev.key {
                        OSKey::Keyboard(keycode) => {
                            println!("Keypress: {:?}", keycode);
                            match keycode {
                                OSKeyCode::KeyBackspace => {
                                    if textinput.idx > 0 {
                                        textinput.idx -= 1;
                                        textinput.buffer.borrow_mut().remove(textinput.idx);
                                    }
                                }
                                OSKeyCode::KeyLeftArrow => {
                                    if textinput.idx > 0 {
                                        textinput.idx -= 1;
                                    }
                                }
                                OSKeyCode::KeyRightArrow => {
                                    if textinput.idx < textinput.buffer.borrow().len() {
                                        textinput.idx += 1;
                                    }
                                }
                                OSKeyCode::KeyEnter => {}
                                _ => {
                                    textinput.buffer.borrow_mut().insert_str(
                                        textinput.idx,
                                        ev.chars.as_ref().unwrap().as_str(),
                                    );
                                    textinput.idx += 1;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    pub fn get_events(&mut self) {
        self.event.events = self.drawer.renderer.win.get_events();
        self.consume_events();
    }
    pub fn resize(&mut self) -> Point {
        self.size = self.drawer.renderer.win.get_size();
        self.drawer.renderer.resize(self.size.0, self.size.1);
        // let root = Rc::new(RefCell::new(UIWidget {
        //     bounds: RectCoords::from_size(0., 0., self.size.0, self.size.1),
        //     parent: None,
        //     children: Vec::new(),
        //     flags: 0,
        //     events: 0,
        // }));
        // self.root = root;
        self.root.borrow_mut().bounds.x1 = self.size.0;
        self.root.borrow_mut().bounds.y1 = self.size.1;
        self.root.borrow_mut().children = Vec::new();
        self.size
    }
}

//// Utility functions
fn point_in_rect(loc: &RectCoords, point: Option<Point>) -> bool {
    if let Some(point) = point {
        point.0 >= loc.x0 && point.0 <= loc.x1 && point.1 >= loc.y0 && point.1 <= loc.y1
    } else {
        false
    }
}
pub fn color_rgb(r: u8, g: u8, b: u8) -> V4f32 {
    V4f32 {
        r: r as f32 / 256.,
        g: g as f32 / 256.,
        b: b as f32 / 256.,
        a: 1.,
    }
}
