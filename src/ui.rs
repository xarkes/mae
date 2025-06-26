use std::rc::Rc;

use crate::{
    draw::{self, Drawer},
    os::{OSEvent, OSEventType, OSKey, Window},
    render::RectCoords,
    widgets,
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

pub struct UIWidget {
    bounds: RectCoords,
    parent: Option<Rc<UIWidget>>,
    next: Option<Rc<UIWidget>>,

    // Computed flags
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
    pub root: Rc<UIWidget>,
    pub drawer: Drawer,
    events: Vec<OSEvent>,

    //// input related
    mouse: (f32, f32),
    click: (f32, f32),
    release: (f32, f32),
}
impl UIState {
    pub fn new(drawer: Drawer) -> Self {
        UIState {
            root: Rc::new(UIWidget {
                bounds: RectCoords::from_size(0., 0., 1024., 768.),
                parent: None,
                next: None,
                flags: 0,
                events: 0,
            }),
            drawer,
            events: Vec::new(),
            mouse: (-1., -1.),
            click: (-1., -1.),
            release: (-1., -1.),
        }
    }

    fn create_ui_widget(
        &mut self,
        bounds: RectCoords,
        parent: Rc<UIWidget>,
        flags: u64,
    ) -> UIWidget {
        let mut w = UIWidget {
            bounds,
            parent: Some(parent),
            next: None,
            flags,
            events: 0,
        };
        // self.consume_events_for_widget(&mut w);
        if point_in_rect(&bounds, self.mouse) {
            w.events |= UIWidgetEvent::MouseOver as u64;
        }
        if point_in_rect(&bounds, self.click) {
            w.events |= UIWidgetEvent::MouseClicked as u64;
        } else if point_in_rect(&bounds, self.release) {
            w.events |= UIWidgetEvent::MouseReleased as u64;
            // consume the release
            self.release = (-1., -1.);
        }
        w
    }

    /////////////////////////////////
    //// UI widgets
    pub fn button(&mut self, parent: Rc<UIWidget>, label: Option<&str>) -> UIWidget {
        let bounds = RectCoords::from_size(100., 100., 100., 20.);
        let uibox = self.create_ui_widget(bounds, parent, UIWidgetFlag::MouseClickable as u64);
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
    // fn consume_events_for_widget(&mut self, w: &mut UIWidget) {
    //     let mut idx = 0;
    //     while idx < self.events.len() {
    //         let ev = &self.events[idx];
    //         let mouse_in_bounds = point_in_rect(&w.bounds, ev.pos);
    //         let ev_is_mouse = ev.key == (OSKey::LeftMouseButton);
    //         if ev.ty == OSEventType::MouseMove && mouse_in_bounds {
    //             w.events |= UIWidgetEvent::MouseOver as u64;
    //             self.events.remove(idx);
    //             continue;
    //         }
    //         if ev_is_mouse && ev.ty == OSEventType::Press && mouse_in_bounds && w.clickable() {
    //             w.flags |= UIWidgetEvent::MouseClicked as u64;
    //             self.events.remove(idx);
    //             continue;
    //         }
    //         idx = idx + 1;
    //     }
    // }
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
}

//// Utility functions
fn point_in_rect(loc: &RectCoords, point: (f32, f32)) -> bool {
    point.0 >= loc.x0 && point.0 <= loc.x1 && point.1 >= loc.y0 && point.1 <= loc.y1
}
