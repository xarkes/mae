mod draw;
mod imui;
mod os;
mod render;

use imui::{IMUI, Point, Size};
use std::{cell::RefCell, rc::Rc};

fn main() {
    let mut ui = IMUI::new(1024, 768);
    let buffer = Rc::new(RefCell::new(String::from("Write here...")));
    let buffer2 = Rc::new(RefCell::new(String::from(
        "Some multiline text\nTry me!\n:p",
    )));
    ui.eventloop(|ui| {
        ui.label("Label here");
        ui.button("suce");
        ui.floating_pane(
            Point::new(300., 300.),
            Size::from((400., 400.)),
            "Demo box",
            |ui| {
                ui.label("Label here");
                ui.button("Button");
                ui.line_edit(buffer.clone(), "#textedit");
                ui.textarea(buffer2.clone(), "#textarea");
            },
        );
    });
}
