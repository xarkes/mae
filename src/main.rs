mod draw;
mod os;
mod render;

use std::{cell::RefCell, rc::Rc};

fn main() {
    let window = os::Window::new(600, 600);
    let renderer = Rc::new(RefCell::new(render::Renderer::new(window)));
    let drawer = draw::Drawer::new(Rc::downgrade(&renderer));

    // TODO: Add a UI module which will handle the drawing of the scene
    // as well as layouting, widgets, etc.
    // TODO: Do I want to try to maintain both immediate and retained mode?

    let freq = os::timer_init();
    println!("Freq: {}", freq);
    let mut start = os::timer_value();
    let mut time = 0f64;
    loop {
        let w: f32;
        let h: f32;
        {
            let mut renderer = renderer.borrow_mut();
            renderer.win.get_events();
            (w, h) = renderer.win.get_size();
            renderer.resize(w, h);
        }

        let ms = format!("{:.3}", time);
        let font_size = 12u32;
        drawer.draw_text(
            w as u32 - font_size * ms.len() as u32,
            0,
            font_size,
            ms.as_str(),
        );
        drawer.draw_text(0, 0, font_size, "This is my text");
        drawer.draw_text(100, 150, font_size, "And another text! :p");

        {
            let mut renderer = renderer.borrow_mut();
            renderer.update();
        }

        let end = os::timer_value();
        time = (end - start) as f64 * 1_000_000.0 / freq;
        start = end;
    }
}
