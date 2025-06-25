mod draw;
mod os;
mod render;
mod widgets;

use std::{cell::RefCell, rc::Rc};

// TODO(xarkes):
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
    loop {
        // xarkes: Handle events
        let w: f32;
        let h: f32;
        {
            let mut renderer = renderer.borrow_mut();
            renderer.win.get_events();
            (w, h) = renderer.win.get_size();
            renderer.resize(w, h);
        }

        // xarkes: Draw fps counter
        {
            let fps = 1f64 / time * 1000f64;
            let text = format!("{:.2}ms - {}fps", time, fps as u64);
            let font_size = 12u32;
            let x = (w - (text.len() as f32 * font_size as f32 / 1.6)) as u32;
            drawer.draw_text(x, 0, font_size, text.as_str(), text.len());
        }

        // xarkes: Draw interface
        widgets::textarea(&drawer, 200, 0, w as u32, h as u32, long_text);

        // xarkes: Render
        {
            let mut renderer = renderer.borrow_mut();
            renderer.update();
        }

        let end = os::timer_value();
        time = (end - start) as f64 * 1_000_000.0 / freq;
        start = end;
    }
}
