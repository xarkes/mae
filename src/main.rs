mod os;
mod render;

fn main() {
    let window = os::Window::new();
    let renderer = render::Renderer::new(window);
    loop {
        renderer.win.get_events();
        renderer.update();
    }
}
