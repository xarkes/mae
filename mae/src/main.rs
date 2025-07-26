mod draw;
mod imui;
mod os;
mod render;

use imui::uibox::UIBoxStyle;
use imui::{IMUI, Point, Size, UISize};
use std::{cell::RefCell, rc::Rc};

fn main() {
    let mut ui = IMUI::new(1024, 768);
    let buffer = Rc::new(RefCell::new("Write here...".to_string()));
    let buffer2 = Rc::new(RefCell::new("Some multiline text\nTry me!\n:p".to_string()));
    ui.eventloop(|ui| {
        let style = UIBoxStyle::default();
        ui.label("This is my label", style);
        ui.button((UISize::TextContent, UISize::TextContent), "Button", style);
        ui.line_edit(buffer.clone(), "#textedit");
        ui.textarea(buffer2.clone(), "#textarea");
        ui.floating_pane(
            Point::new(300., 300.),
            Size::from((400., 400.)),
            "Demo box",
            |ui| {
                ui.label("Label here", style);
                ui.button((UISize::TextContent, UISize::TextContent), "Button", style);
                ui.line_edit(buffer.clone(), "#textedit");
                ui.textarea(buffer2.clone(), "#textarea");
            },
        );
    });
}
