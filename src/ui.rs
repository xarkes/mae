use std::{cell::RefCell, rc::Rc};

use crate::{
    draw::{self, Drawer},
    os::{OSEvent, OSEventType, OSKey, Window},
    render::RectCoords,
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

#[repr(u32)]
#[derive(Clone, Copy)]
enum UISizeKind {
    Pixels,
    Percents,
}

#[derive(Clone, Copy)]
pub struct UISize {
    kind: UISizeKind,
    value: f32,
}
impl UISize {
    pub fn pct(val: f32) -> Self {
        UISize {
            kind: UISizeKind::Percents,
            value: val,
        }
    }
    pub fn px(val: f32) -> Self {
        UISize {
            kind: UISizeKind::Pixels,
            value: val,
        }
    }
}

pub struct UIWidget {
    bounds: RectCoords,
    parent: Option<Rc<RefCell<UIWidget>>>,

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

pub struct UIState {
    pub root: Rc<RefCell<UIWidget>>,
    pub drawer: Drawer,
    events: Vec<OSEvent>,

    //// style and layout related
    parent: Rc<RefCell<UIWidget>>,
    width: UISize,
    height: UISize,

    //// input related
    mouse: (f32, f32),
    click: (f32, f32),
    release: (f32, f32),
}
impl UIState {
    pub fn new(drawer: Drawer) -> Self {
        let root = Rc::new(RefCell::new(UIWidget {
            bounds: RectCoords::from_size(0., 0., 1024., 768.),
            parent: None,
            flags: 0,
            events: 0,
        }));
        UIState {
            root: root.clone(),
            drawer,
            events: Vec::new(),
            parent: root.clone(),
            width: UISize {
                kind: UISizeKind::Percents,
                value: 1.,
            },
            height: UISize {
                kind: UISizeKind::Percents,
                value: 1.,
            },
            mouse: (-1., -1.),
            click: (-1., -1.),
            release: (-1., -1.),
        }
    }

    fn create_ui_widget(&mut self, bounds: RectCoords, flags: u64) -> UIWidget {
        let mut w = UIWidget {
            bounds,
            parent: Some(self.parent.clone()),
            flags,
            events: 0,
        };
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
        w
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
    fn compute_layout_bounds(&self) -> RectCoords {
        let x = 0.;
        let y = 0.;
        let w = match self.width.kind {
            UISizeKind::Pixels => self.width.value,
            UISizeKind::Percents => {
                self.width.value * (self.parent.borrow().bounds.x1 - self.parent.borrow().bounds.x0)
            }
        };
        let h = match self.height.kind {
            UISizeKind::Pixels => self.height.value,
            UISizeKind::Percents => {
                self.height.value
                    * (self.parent.borrow().bounds.y1 - self.parent.borrow().bounds.y0)
            }
        };
        RectCoords::from_size(x, y, x + w, x + h)
    }

    /////////////////////////////////
    //// UI widgets
    pub fn button(&mut self, label: Option<&str>) -> UIWidget {
        let bounds = self.compute_layout_bounds();
        let uibox = self.create_ui_widget(bounds, UIWidgetFlag::MouseClickable as u64);
        let mut bg_color = draw::color::TMP;
        let mut draw_off = 0.;
        if uibox.hover() {
            bg_color = draw::color::TMP2;
        }
        if uibox.click() {
            draw_off = 1.;
        }
        self.drawer.draw_rect(
            &RectCoords {
                x0: bounds.x0 + draw_off,
                y0: bounds.y0 + draw_off,
                x1: bounds.x1 + draw_off,
                y1: bounds.y1 + draw_off,
            },
            bg_color,
        );
        if let Some(label) = label {
            self.drawer.draw_text(
                bounds.x0 + draw_off,
                bounds.y0 + draw_off,
                12,
                label,
                label.len(),
                draw::color::WHITE,
            );
        }
        uibox
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
    pub fn get_events(&mut self, win: &Window) {
        self.events = win.get_events();
        self.consume_events();
    }
    pub fn resize(&mut self, w: f32, h: f32) {
        self.root.borrow_mut().bounds.x1 = w;
        self.root.borrow_mut().bounds.y1 = h;
    }
}

//// Utility functions
fn point_in_rect(loc: &RectCoords, point: (f32, f32)) -> bool {
    point.0 >= loc.x0 && point.0 <= loc.x1 && point.1 >= loc.y0 && point.1 <= loc.y1
}
