use std::{cell::RefCell, rc::Rc};

use crate::{
    draw::{self, Drawer},
    os::{self, OSEvent, OSEventType, OSKey},
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

pub enum UISize {
    Pixels(f32),
    Percents(f32),
}

pub struct UIWidget {
    bounds: RectCoords,
    last_child: Option<UIWidgetRef>,

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
    parent: Option<UIWidgetRef>,
    width: UISize,
    height: UISize,
    color: V4f32,
    layout: u32,
    text_align: UITextAlign,
}
impl UIWidgetParams {
    pub fn default() -> Self {
        UIWidgetParams {
            parent: None,
            width: UISize::Percents(1.),
            height: UISize::Percents(1.),
            color: V4f32 {
                r: 1.,
                g: 1.,
                b: 1.,
                a: 1.,
            },
            layout: 0,
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
    pub fn parent(&mut self, parent: Option<UIWidgetRef>) -> &mut Self {
        self.parent = parent;
        self
    }
    pub fn color(&mut self, color: V4f32) -> &mut Self {
        self.color = color;
        self
    }
    pub fn layout(&mut self, layout: u32) -> &mut Self {
        self.layout = layout;
        self
    }
    pub fn text_align(&mut self, mode: UITextAlign) -> &mut Self {
        self.text_align = mode;
        self
    }
    pub fn reset(&mut self) {
        // TODO(xarkes): This sucks
        self.parent = UIWidgetParams::default().parent;
        self.width = UIWidgetParams::default().width;
        self.height = UIWidgetParams::default().height;
        self.color = UIWidgetParams::default().color;
        self.layout = UIWidgetParams::default().layout;
        self.text_align = UIWidgetParams::default().text_align;
    }
}

pub struct IMUI {
    pub root: UIWidgetRef,
    pub drawer: Drawer,
    debug: bool,
    size: (f32, f32),
    events: Vec<OSEvent>,
    params: UIWidgetParams,

    //// input events cache
    mouse: Option<Point>,
    click: Option<Point>,
    release: Option<Point>,
}
impl IMUI {
    pub fn new(w: u32, h: u32) -> Self {
        let window = os::Window::new(w, h);
        let renderer = render::Renderer::new(window);
        let drawer = draw::Drawer::new(renderer);

        let root = Rc::new(RefCell::new(UIWidget {
            bounds: RectCoords::from_size(0., 0., 1024., 768.),
            last_child: None,
            flags: 0,
            events: 0,
        }));
        IMUI {
            root: root.clone(),
            drawer,
            debug: false,
            size: (0., 0.),
            events: Vec::new(),
            params: UIWidgetParams::default(),
            mouse: None,
            click: None,
            release: None,
        }
    }
    pub fn debug(&mut self) {
        self.debug = true;
    }
    pub fn eventloop(&mut self, mut drawfunction: impl FnMut(&mut IMUI)) {
        let display_fps = self.debug;
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

            // xarkes: draw and update FPS counter
            if display_fps {
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

            #[cfg(debug_assertions)]
            self.draw_debug_pane();

            // xarkes: render
            {
                self.drawer.renderer.render_frame();
            }
        }
    }

    #[cfg(debug_assertions)]
    fn draw_debug_pane(&mut self) {
        // TODO()
        self.params.reset();
        self.params
            .layout(2)
            .parent(Some(self.root.clone()))
            .size(UISize::Pixels(200.), UISize::Pixels(40.))
            .color(color_rgb(40, 60, 140));
        let uibox = self.widget();
        self.params.reset();
        self.params.layout(1).parent(Some(uibox.clone()));
        let osef = false;
        self.checkbox(&osef);
    }

    fn create_ui_widget(&mut self, flags: u64) -> UIWidgetRef {
        // xarkes: apply layout properties and compute bounds
        let bounds = self.compute_layout_bounds();
        let mut w = UIWidget {
            bounds,
            last_child: None,
            flags,
            events: 0,
        };

        // xarkes: apply events flags
        if point_in_rect(&bounds, self.mouse) {
            w.events |= UIWidgetEvent::MouseOver as u64;
        }
        if point_in_rect(&bounds, self.click) && w.clickable() {
            w.events |= UIWidgetEvent::MouseClicked as u64;
        } else if point_in_rect(&bounds, self.release) && w.clickable() {
            w.events |= UIWidgetEvent::MouseReleased as u64;
            // xarkes: consume the release so clicked() triggers only once
            self.release = None;
        }

        // xarkes: update parent childs
        let childref = Rc::new(RefCell::new(w));
        self.params.parent.as_mut().unwrap().borrow_mut().last_child = Some(childref.clone());
        childref
    }

    /////////////////////////////////
    //// Styling and layout
    fn compute_layout_bounds(&self) -> RectCoords {
        // TODO(xarkes): Stop procrastinating and rewrite this shit omg
        // TODO(xarkes): think of an actual layout algorithm and fix node relationships
        // in this immediate context we can only rely on previously added nodes
        let params = &self.params;
        let parent = params.parent.as_ref().unwrap().borrow();
        let prev = parent.last_child.clone();
        let prev_bounds = match prev {
            None => RectCoords {
                x0: 0.,
                y0: 0.,
                x1: 0.,
                y1: 0.,
            },
            Some(prev) => prev.borrow().bounds,
        };

        fn uisize_as_px(uisize: &UISize, parent_val: f32) -> f32 {
            match uisize {
                UISize::Pixels(val) => *val,
                UISize::Percents(val) => val * parent_val,
            }
        }

        let parent_width = parent.bounds.x1 - parent.bounds.x0;
        let parent_height = parent.bounds.y1 - parent.bounds.y0;
        let mut rect = match params.layout {
            0 => {
                // default
                let w = uisize_as_px(&params.width, parent_width);
                let h = uisize_as_px(&params.height, parent_height);
                RectCoords::from_size(prev_bounds.x0, prev_bounds.y1, w, h)
            }
            1 => {
                // centered
                let w = uisize_as_px(&params.width, parent_width);
                let h = uisize_as_px(&params.height, parent_height);
                let x = parent.bounds.x0 + (parent_width - w) / 2.0;
                let y = parent.bounds.y0 + prev_bounds.y1;
                RectCoords::from_size(x, y, w, h)
            }
            2 => {
                // float
                let w = uisize_as_px(&params.width, parent_width);
                let h = uisize_as_px(&params.height, parent_height);
                let x = parent.bounds.x1 - w;
                let y = parent.bounds.y0;
                RectCoords::from_size(x, y, w, h)
            }
            _ => {
                panic!("not handled");
            }
        };

        // xarkes: Adjust rectangle to be in bounds of screen
        // TODO(xarkes): Replace this into asserts and make the layout algorithm solid if possible
        // I guess the challenge here is that you cannot know before drawing the required space
        // or whatever (since it's immediate mode)
        if rect.x0 < 0. {
            rect.x0 = 0.;
        }
        if rect.x1 > self.size.0 {
            rect.x1 = self.size.0;
        }
        if rect.y0 < 0. {
            rect.y0 = 0.;
        }
        if rect.y1 > self.size.1 {
            rect.y1 = self.size.1;
        }
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
    fn draw_bounds(&mut self, widget: &UIWidget) {
        if self.debug {
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
    pub fn widget(&mut self) -> UIWidgetRef {
        let widget = self.create_ui_widget(0);
        self.drawer
            .draw_rect(&widget.borrow().bounds, self.params.color);
        self.draw_bounds(&widget.borrow());
        widget
    }
    pub fn checkbox(&mut self, state: &bool) -> UIWidgetRef {
        let checkbox = self.create_ui_widget(UIWidgetFlag::MouseClickable as u64);
        let box_bounds = RectCoords::from_size(
            checkbox.borrow().bounds.x0,
            checkbox.borrow().bounds.y0,
            30.,
            30.,
        );
        self.drawer.draw_rect(&box_bounds, color_rgb(255, 255, 255));
        self.drawer
            .draw_empty_rect(&box_bounds, color_rgb(0, 0, 0), 1., false);
        if checkbox.borrow().clicked() {
            let box_bounds =
                RectCoords::from_size(box_bounds.x0 + 2., box_bounds.y0 + 2., 26., 26.);
            self.drawer.draw_rect(&box_bounds, color_rgb(255, 255, 255));
        }
        checkbox
    }
    pub fn line_edit(&mut self, text_buffer: &String, hint: Option<&str>) -> UIWidgetRef {
        // TODO(xarkes): finish drawing and all -> I hope it can be cool
        let le = self.create_ui_widget(UIWidgetFlag::MouseClickable as u64);
        let bg_color = color_rgb(200, 200, 200);
        self.drawer.draw_rect(&le.borrow().bounds, bg_color);
        self.drawer.draw_text(
            le.borrow().bounds.x0,
            le.borrow().bounds.y0,
            12,
            text_buffer.as_str(),
            text_buffer.len(),
            color_rgb(0, 0, 0),
        );
        self.draw_bounds(&le.borrow());
        le
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
        let widget = self.create_ui_widget(0);
        let label_width = self.drawer.get_text_size(12, label, label.len()).0;
        self.drawer.draw_text(
            // TODO(xarkes): Text align center requires text api to precompute the text length
            match self.params.text_align {
                UITextAlign::Left => widget.borrow().bounds.x0,
                UITextAlign::Center => {
                    (widget.borrow().bounds.x1 - widget.borrow().bounds.x0) / 2. - label_width / 2.
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
        for ev in &self.events {
            if ev.ty == OSEventType::MouseMove {
                self.mouse = Some(ev.pos);
            } else if ev.ty == OSEventType::Press && ev.key == OSKey::LeftMouseButton {
                self.click = Some(ev.pos);
            } else if ev.ty == OSEventType::Release && ev.key == OSKey::LeftMouseButton {
                self.click = None;
                self.release = Some(ev.pos);
            }
        }
    }
    pub fn get_events(&mut self) {
        self.events = self.drawer.renderer.win.get_events();
        self.consume_events();
    }
    pub fn resize(&mut self) -> Point {
        self.size = self.drawer.renderer.win.get_size();
        self.drawer.renderer.resize(self.size.0, self.size.1);
        let root = Rc::new(RefCell::new(UIWidget {
            bounds: RectCoords::from_size(0., 0., self.size.0, self.size.1),
            last_child: None,
            flags: 0,
            events: 0,
        }));
        self.root = root;
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
