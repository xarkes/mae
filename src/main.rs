mod draw;
mod os;
mod render;
mod ui;
mod widgets;

use std::{cell::RefCell, rc::Rc};
use ui::{UISize, UIState};

// TODO(xarkes):
// - [ ] XXX: Urgent: take a decision regarding the APIs. Should we work with u32 (pixels) or floats? Currently it is a bit a mix of everything and we have to decide which one to use and stick to it.
// - [ ] Add proper logging
// - [ ] Draw the interface as you'd like it
// - [ ] Handle events (mouse over, mouse click, keyboard inputs, ...)
// - [ ] Port to Linux
fn main() {
    let window = os::Window::new(1024, 768);
    let renderer = Rc::new(RefCell::new(render::Renderer::new(window)));
    let drawer = draw::Drawer::new(Rc::downgrade(&renderer));
    let freq = os::timer_init();
    let mut start = os::timer_value();
    let mut time = 0f64;
    let long_text = include_str!("./main.rs");

    let mut ui = Box::new(UIState::new(drawer));
    let mut val = 1234;
    loop {
        // xarkes: handle events
        let w: f32;
        let h: f32;
        {
            let mut renderer = renderer.borrow_mut();
            ui.get_events(&renderer.win);
            (w, h) = renderer.win.get_size();
            renderer.resize(w, h);
            ui.resize(w, h);
        }

        // xarkes: draw interface
        {
            ui.size(UISize::pct(0.2), UISize::px(20.));
            let but = ui.button(Some("Click me"));
            if but.clicked() {
                val += 1;
            }
            let color = draw::color::WHITE;
            let txt = format!("Yooo {}", val);
            ui.drawer
                .draw_text(100., 120., 12, txt.as_str(), txt.len(), color);
        }

        // xarkes: draw fps counter
        {
            let fps = 1f64 / time * 1000f64;
            let text = format!("{:.2}ms - {}fps", time, fps as u64);
            let font_size = 12u32;
            let x = w - (text.len() as f32 * font_size as f32 / 1.6);
            ui.drawer.draw_text(
                x,
                0.0,
                font_size,
                text.as_str(),
                text.len(),
                draw::color::WHITE,
            );
        }

        // xarkes: render
        {
            let mut renderer = renderer.borrow_mut();
            renderer.update();
        }

        let end = os::timer_value();
        time = (end - start) as f64 * 1_000_000.0 / freq;
        start = end;
    }
}
