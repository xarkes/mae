mod draw;
mod os;
mod render;

use std::{cell::RefCell, rc::Rc};

fn main() {
    let window = os::Window::new(600, 600);
    let renderer = Rc::new(RefCell::new(render::Renderer::new(window)));
    let drawer = draw::Drawer::new(Rc::downgrade(&renderer));
    loop {
        {
            let mut renderer = renderer.borrow_mut();
            renderer.win.get_events();
            let (w, h) = renderer.win.get_size();
            renderer.resize(w, h);
        }

        drawer.draw_text(0, 0, 12, "This is my text");
        drawer.draw_text(100, 150, 12, "And another text! :p");

        {
            let mut renderer = renderer.borrow_mut();
            renderer.update();
        }
    }
}
