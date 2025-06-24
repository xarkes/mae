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
    let long_text = include_str!("/tmp/file.txt");
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
        // drawer.draw_text(0, 0, font_size, "This is my text");
        // drawer.draw_text(100, 150, font_size, "And another text! :p");

        fn text_widget(x: u32, y: u32, winx: u32, winy: u32, content: &str, drawer: &draw::Drawer) {
            // xarkes: iterate lines and draw them
            let mut yoff = 0;
            let width = winx - x;
            let nchars = width / (12 / 2);
            for line in content.split('\n') {
                // TODO(xarkes): This sucks due to reallocation
                let mut line = line.to_string();
                line.truncate(nchars as usize);
                drawer.draw_text(x, y + yoff, 12, line.as_str());
                yoff += 14;

                // xarkes: Don't draw not visible lines
                if y + yoff > winy {
                    break;
                }
            }
        }
        text_widget(0, 0, w as u32, h as u32, long_text, &drawer);

        {
            let mut renderer = renderer.borrow_mut();
            renderer.update();
        }

        let end = os::timer_value();
        time = (end - start) as f64 * 1_000_000.0 / freq;
        start = end;
    }
}
