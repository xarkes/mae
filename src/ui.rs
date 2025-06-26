use std::{cell::RefCell, rc::Rc};

use crate::{
    draw::Drawer,
    os::{OSEvent, OSEventType, Window},
    render::RectCoords,
};

pub struct UIBox {
    parent: Option<Rc<RefCell<UIBox>>>,
}
pub struct UIState {
    root: UIBox,
    events: Vec<OSEvent>,
    pub drawer: Drawer,
    mouse: (f32, f32),
    cursor: (f32, f32),
}
impl UIState {
    pub fn new(drawer: Drawer) -> Self {
        UIState {
            root: UIBox { parent: None },
            events: Vec::new(),
            drawer,
            mouse: (-1.0, -1.0),
            cursor: (-1.0, -1.0),
        }
    }

    /////////////////////////////////
    //// Events related functions
    pub fn get_events(&mut self, win: &Window) {
        self.events = win.get_events();
        self.consume_events();
    }
    pub fn consume_events(&mut self) {
        // TODO(xarkes): This likely sucks, but hey it is a work in progress!
        for ev in &self.events {
            if ev.ty == OSEventType::MouseMove {
                self.mouse = ev.pos;
            } else if ev.ty == OSEventType::MouseClick {
                self.cursor = ev.pos;
            }
        }
    }
    pub fn hover(&self, coords: &RectCoords) -> bool {
        point_in_rect(coords, self.mouse)
    }
    pub fn cursor(&self) -> (f32, f32) {
        self.cursor
    }
}

//// Utility functions
fn point_in_rect(loc: &RectCoords, point: (f32, f32)) -> bool {
    point.0 >= loc.x0 && point.0 <= loc.x1 && point.1 >= loc.y0 && point.1 <= loc.y1
}
