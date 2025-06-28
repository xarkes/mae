use std::{cell::RefCell, rc::Rc};

use crate::{
    draw::{self, Drawer},
    os::{self, OSEvent, OSEventType, OSKey},
    render::{self, RectCoords, V4f32},
};

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

pub enum UISize {
    Pixels(f32),
    Percents(f32),
}
impl UISize {
    pub fn pct(val: f32) -> Self {
        UISize::Percents(val)
    }
    pub fn px(val: f32) -> Self {
        UISize::Pixels(val)
    }
}

pub struct UIWidget {
    bounds: RectCoords,
    #[allow(dead_code)]
    parent: Option<Rc<RefCell<UIWidget>>>,
    last_child: Option<Rc<RefCell<UIWidget>>>,

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

pub struct IMUIState {
    pub root: Rc<RefCell<UIWidget>>,
    pub drawer: Drawer,
    events: Vec<OSEvent>,

    //// style and layout related options, used for creation
    parent: Rc<RefCell<UIWidget>>,
    width: UISize,
    height: UISize,
    color: V4f32,
    layout: u32,

    //// input events cache
    mouse: (f32, f32),
    click: (f32, f32),
    release: (f32, f32),
}
impl IMUIState {
    pub fn new(w: u32, h: u32) -> Self {
        let window = os::Window::new(w, h);
        let renderer = render::Renderer::new(window);
        let drawer = draw::Drawer::new(renderer);

        let root = Rc::new(RefCell::new(UIWidget {
            bounds: RectCoords::from_size(0., 0., 1024., 768.),
            parent: None,
            last_child: None,
            flags: 0,
            events: 0,
        }));
        IMUIState {
            root: root.clone(),
            drawer,
            events: Vec::new(),
            parent: root.clone(),
            width: UISize::pct(1.),
            height: UISize::pct(1.),
            color: draw::color::WHITE,

            // TODO(xarkes): Make this optional
            layout: 0,
            mouse: (-1., -1.),
            click: (-1., -1.),
            release: (-1., -1.),
        }
    }
    pub fn eventloop(&mut self, mut drawfunction: impl FnMut(&mut IMUIState)) {
        let display_fps = true;
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

            // xarkes: render
            {
                self.drawer.renderer.update();
            }
        }
    }

    fn create_ui_widget(&mut self, flags: u64) -> Rc<RefCell<UIWidget>> {
        // xarkes: apply layout properties and compute bounds
        let bounds = self.compute_layout_bounds();
        let mut w = UIWidget {
            bounds,
            parent: Some(self.parent.clone()),
            last_child: None,
            flags,
            events: 0,
        };

        // xarkes: apply events flags
        if point_in_rect(&bounds, self.mouse) && w.clickable() {
            w.events |= UIWidgetEvent::MouseOver as u64;
        }
        if point_in_rect(&bounds, self.click) && w.clickable() {
            w.events |= UIWidgetEvent::MouseClicked as u64;
        } else if point_in_rect(&bounds, self.release) && w.clickable() {
            w.events |= UIWidgetEvent::MouseReleased as u64;
            // xarkes: consume the release so clicked() triggers only once
            self.release = (-1., -1.);
        }

        // xarkes: update parent childs
        let childref = Rc::new(RefCell::new(w));
        self.parent.borrow_mut().last_child = Some(childref.clone());
        childref
    }

    /////////////////////////////////
    //// Styling and layout
    pub fn size(&mut self, w: UISize, h: UISize) {
        self.width(w);
        self.height(h);
    }
    pub fn width(&mut self, w: UISize) {
        self.width = w;
    }
    pub fn height(&mut self, h: UISize) {
        self.height = h;
    }
    pub fn parent(&mut self, parent: Rc<RefCell<UIWidget>>) {
        self.parent = parent;
    }
    pub fn color(&mut self, color: V4f32) {
        self.color = color;
    }
    pub fn color_rgb(&mut self, r: u8, g: u8, b: u8) -> V4f32 {
        self.color = V4f32 {
            r: r as f32 / 256.,
            g: g as f32 / 256.,
            b: b as f32 / 256.,
            a: 1.,
        };
        self.color
    }
    pub fn layout(&mut self, layout: u32) {
        self.layout = layout;
    }
    fn compute_layout_bounds(&self) -> RectCoords {
        // TODO(xarkes): think of an actual layout algorithm and fix node relationships
        // in this immediate context we can only rely on previously added nodes

        let prev = self.parent.borrow().last_child.clone();
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

        let parent_width = self.parent.borrow().bounds.x1 - self.parent.borrow().bounds.x0;
        let parent_height = self.parent.borrow().bounds.y1 - self.parent.borrow().bounds.y0;
        match self.layout {
            0 => {
                // default
                let w = uisize_as_px(&self.width, parent_width);
                let h = uisize_as_px(&self.height, parent_height);
                RectCoords::from_size(prev_bounds.x0, prev_bounds.y1, w, h)
            }
            1 => {
                // centered
                let w = uisize_as_px(&self.width, parent_width);
                let h = uisize_as_px(&self.height, parent_height);
                let x = self.parent.borrow().bounds.x0 + (parent_width - w) / 2.0;
                let y = self.parent.borrow().bounds.y0 + prev_bounds.y1;
                RectCoords::from_size(x, y, w, h)
            }
            _ => {
                panic!("not handled");
            }
        }
    }

    /////////////////////////////////
    //// UI widgets
    pub fn widget(&mut self) -> Rc<RefCell<UIWidget>> {
        let widget = self.create_ui_widget(0);
        self.drawer.draw_rect(&widget.borrow().bounds, self.color);
        widget
    }
    pub fn button(&mut self, label: Option<&str>) -> Rc<RefCell<UIWidget>> {
        let button = self.create_ui_widget(UIWidgetFlag::MouseClickable as u64);
        {
            let uibox = button.borrow();
            let mut bg_color = self.color;
            let mut draw_off = 0.;
            if uibox.hover() {
                bg_color = V4f32 {
                    r: self.color.r * 1.1,
                    g: self.color.g * 1.1,
                    b: self.color.b * 1.1,
                    a: self.color.a,
                };
            }
            if uibox.click() {
                draw_off = 1.;
            }
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
        button
    }
    pub fn label(&mut self, label: &str) -> Rc<RefCell<UIWidget>> {
        let widget = self.create_ui_widget(0);
        self.drawer.draw_text(
            widget.borrow().bounds.x0,
            widget.borrow().bounds.y0,
            12,
            label,
            label.len(),
            self.color,
        );
        widget
    }

    /////////////////////////////////
    //// Events related functions
    fn consume_events(&mut self) {
        for ev in &self.events {
            if ev.ty == OSEventType::MouseMove {
                self.mouse = ev.pos;
            } else if ev.ty == OSEventType::Press && ev.key == OSKey::LeftMouseButton {
                self.click = ev.pos;
            } else if ev.ty == OSEventType::Release && ev.key == OSKey::LeftMouseButton {
                self.click = (-1., -1.);
                self.release = ev.pos;
            }
        }
    }
    pub fn get_events(&mut self) {
        self.events = self.drawer.renderer.win.get_events();
        self.consume_events();
    }
    pub fn resize(&mut self) -> (f32, f32) {
        let (w, h) = self.drawer.renderer.win.get_size();
        self.drawer.renderer.resize(w, h);
        let root = Rc::new(RefCell::new(UIWidget {
            bounds: RectCoords::from_size(0., 0., w, h),
            parent: None,
            last_child: None,
            flags: 0,
            events: 0,
        }));
        self.root = root;
        (w, h)
    }
}

//// Utility functions
fn point_in_rect(loc: &RectCoords, point: (f32, f32)) -> bool {
    point.0 >= loc.x0 && point.0 <= loc.x1 && point.1 >= loc.y0 && point.1 <= loc.y1
}
pub fn create_window(w: u32, h: u32) -> Box<IMUIState> {
    Box::new(IMUIState::new(w, h))
}
