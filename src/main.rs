mod draw;
mod os;
mod render;
mod rmgui;
mod widgets;

use std::{cell::RefCell, rc::Rc};

fn main() {
    let mut window = GUIWindow::create_window(600, 600);
    let long_text = include_str!("/tmp/file.txt");
    let textarea = widgets::TextArea::new(String::from(long_text));
    window.gui.add(textarea);
    window.event_loop()
}

struct GUIWindow {
    renderer: Rc<RefCell<render::Renderer>>,
    gui: rmgui::RMGUI,
}

impl GUIWindow {
    pub fn create_window(width: u32, height: u32) -> Self {
        let window = os::Window::new(width, height);
        let renderer = Rc::new(RefCell::new(render::Renderer::new(window)));
        let drawer = draw::Drawer::new(renderer.clone());
        // TODO: Do I want to try to maintain both immediate and retained mode?
        GUIWindow {
            renderer,
            gui: rmgui::RMGUI::new(drawer),
        }
    }

    pub fn event_loop(&mut self) {
        let freq = os::timer_init();
        println!("Freq: {}", freq);
        let mut start = os::timer_value();
        let mut time = 0f64;

        // xarkes: Add FPS counter
        let fps_counter = widgets::Label::new(String::new());
        self.gui.add(fps_counter.clone());

        loop {
            let w: f32;
            let h: f32;
            {
                let mut renderer = self.renderer.borrow_mut();
                renderer.win.get_events();
                (w, h) = renderer.win.get_size();
                renderer.resize(w, h);
            }

            let ms = format!("{:.3}", time);
            fps_counter.borrow_mut().text = ms;
            self.gui.draw();

            {
                let mut renderer = self.renderer.borrow_mut();
                renderer.update();
            }

            let end = os::timer_value();
            time = (end - start) as f64 * 1_000_000.0 / freq;
            start = end;
        }
    }
}
