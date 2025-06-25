mod draw;
mod os;
mod render;
mod widgets;

use std::{cell::RefCell, rc::Rc};

use os::{WindowEvent, WindowEventType};
use render::V2f32;

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
    let long_text = include_str!("/tmp/file.txt");

    let mut mouse_coords: V2f32 = V2f32 { x: -1.0, y: -1.0 };
    loop {
        // xarkes: handle events
        let w: f32;
        let h: f32;
        {
            let mut renderer = renderer.borrow_mut();
            let events = renderer.win.get_events();
            for ev in events {
                match ev.ty {
                    WindowEventType::MouseMove => {
                        mouse_coords.x = ev.data0;
                        mouse_coords.y = ev.data1;
                    }
                    _ => {}
                }
            }

            (w, h) = renderer.win.get_size();
            renderer.resize(w, h);
        }

        // xarkes: draw interface
        {
            widgets::treeview(&drawer, 0.0, 0.0, 200.0, h, &mouse_coords);
            widgets::textarea(&drawer, 200.0, 0.0, 200.0 + w, h, long_text);
        }

        // xarkes: draw fps counter
        {
            let fps = 1f64 / time * 1000f64;
            let text = format!("{:.2}ms - {}fps", time, fps as u64);
            let font_size = 12u32;
            let x = w - (text.len() as f32 * font_size as f32 / 1.6);
            drawer.draw_text(
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
