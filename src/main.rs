mod os;
mod render;

fn main() {
    let window = os::Window::new(600, 600);
    let mut renderer = render::Renderer::new(window);
    loop {
        renderer.win.get_events();
        renderer.update();
    }
}
