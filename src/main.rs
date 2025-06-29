mod draw;
mod imui;
mod os;
mod render;

use imui::{IMUI, UISize, UITextAlign};

fn main() {
    let mut ui = IMUI::new(1024, 768);
    ui.debug();
    let mut count = 0;
    ui.eventloop(|ui| {
        let root = ui.root.clone();
        let blue = imui::color_rgb(61, 78, 219);
        let white = imui::color_rgb(255, 255, 255);
        let black = imui::color_rgb(0, 0, 0);

        // top bar
        {
            ui.params()
                .layout(0)
                .parent(Some(root.clone()))
                .size(UISize::Percents(1.), UISize::Pixels(40.))
                .color(blue);
            ui.widget();
        }

        // main content
        ui.params()
            .parent(Some(root.clone()))
            .size(UISize::Percents(1.), UISize::Percents(1.))
            .color(imui::color_rgb(230, 230, 230));
        let content = ui.widget();

        ui.params()
            .size(UISize::Percents(1.), UISize::Pixels(14.))
            .parent(Some(content.clone()))
            .text_align(UITextAlign::Center)
            .layout(1)
            .color(black);
        ui.label("Your vault is locked.");
        ui.rparams()
            .size(UISize::Percents(1.), UISize::Pixels(100.));
        ui.label("someone@somewhere.com");

        // white box
        ui.params()
            .parent(Some(content.clone()))
            .size(UISize::Percents(0.), UISize::Pixels(100.));
        ui.widget();
        ui.params()
            .parent(Some(content.clone()))
            .size(UISize::Percents(0.5), UISize::Percents(0.5))
            .color(white)
            .layout(1);
        let mid = ui.widget();

        ui.params()
            .parent(Some(mid.clone()))
            .size(UISize::Percents(0.6), UISize::Pixels(30.))
            .layout(1)
            .color(blue);
        ui.line_edit(None);
        if ui.button(Some("Click here!")).borrow().clicked() {
            count += 1;
        }
        ui.label(format!("Count: {}", count).as_str());
    });
}
