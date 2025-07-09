mod draw;
mod imui;
mod os;
mod render;

use imui::{IMUI, UISize, UITextAlign};

fn main() {
    let mut ui = IMUI::new(1024, 768);
    let mut count = 0;
    let mut buffer = String::from("Bonjour");
    ui.eventloop(|ui| {
        let root = ui.root.clone();
        let blue = imui::color_rgb(61, 78, 219);
        let white = imui::color_rgb(255, 255, 255);
        let black = imui::color_rgb(0, 0, 0);

        // top bar
        {
            ui.params()
                .parent(root.clone())
                .size(UISize::Percents(1.), UISize::Pixels(40.))
                .color(blue);
            ui.widget();
        }

        // main content
        ui.params()
            .parent(root.clone())
            .size(UISize::Percents(1.), UISize::Percents(1.))
            .color(imui::color_rgb(230, 230, 230));
        let content = ui.widget();

        ui.params()
            .size(UISize::Percents(1.), UISize::Pixels(14.))
            .parent(content.clone())
            .text_align(UITextAlign::Center)
            .color(black);
        ui.label("Your vault is locked.");
        ui.rparams()
            .size(UISize::Percents(1.), UISize::Pixels(100.));
        ui.label("someone@somewhere.com");

        // white box
        ui.params()
            .parent(content.clone())
            .size(UISize::Percents(1.), UISize::Pixels(100.))
            .color(imui::color::NONE);
        ui.widget();
        ui.params()
            .parent(content.clone())
            .size(UISize::Percents(0.5), UISize::Percents(0.5))
            .color(white);
        let mid = ui.widget();

        ui.params()
            .parent(mid.clone())
            .size(UISize::Percents(0.6), UISize::Pixels(30.))
            .color(blue);
        ui.line_edit(&buffer, None);
        if ui.button(Some("Click here!")).borrow().clicked() {
            count += 1;
        }
        ui.label(format!("Count: {}", count).as_str());
    });
}
