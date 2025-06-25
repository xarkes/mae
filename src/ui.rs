use crate::{
    draw::Drawer,
    os::{OSEvent, OSEventType, Window},
    render::RectCoords,
};

pub struct UIState {
    events: Vec<OSEvent>,
    pub drawer: Drawer,
    mouse: (f32, f32),
}
impl UIState {
    pub fn new(drawer: Drawer) -> Self {
        UIState {
            events: Vec::new(),
            drawer,
            mouse: (-1.0, -1.0),
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
            let ev_is_mouse = ev.ty == OSEventType::MouseMove;

            if ev_is_mouse {
                self.mouse = ev.pos;
            }
        }
    }
    pub fn hover(&self, coords: &RectCoords) -> bool {
        point_in_rect(coords, self.mouse)
    }
}

//// Utility functions
fn point_in_rect(loc: &RectCoords, point: (f32, f32)) -> bool {
    point.0 >= loc.x0 && point.0 <= loc.x1 && point.1 >= loc.y0 && point.1 <= loc.y1
}
