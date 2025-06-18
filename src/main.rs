mod os;
mod render;

fn main() {
    let window = os::Window::new(600, 600);
    let mut renderer = render::Renderer::new(window);
    loop {
        renderer.win.get_events();
        let (w, h) = renderer.win.get_size();
        renderer.resize(w, h);
        renderer.update();
    }
}
