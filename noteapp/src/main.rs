use std::cell::RefCell;
use std::rc::Rc;

use mae::imui::IMUI;
use mae::imui::UISize;

fn main() {
    let file_buffer = Rc::new(RefCell::new(
        std::fs::read_to_string("../mae/src/imui.rs").unwrap(),
    ));

    let mut ui = IMUI::new(1024, 768);
    ui.eventloop(|ui| {
        ui.params().size(UISize::Percents(1.), UISize::Percents(1.));
        ui.textarea(file_buffer.clone(), "maintextarea");
    });
}
