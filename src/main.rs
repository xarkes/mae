mod draw;
mod imui;
mod os;
mod render;
mod widgets;

use imui::{IMUI, UISize};

fn main() {
    let mut ui = IMUI::new(1024, 768);
    let mut count = 0;
    ui.eventloop(|ui| {
        // top bar
        // let param = LayoutParam::default();
        let blue = ui.color_rgb(61, 78, 219);
        {
            ui.layout(0);
            ui.parent(ui.root.clone());
            ui.size(UISize::pct(1.), UISize::px(40.));
            ui.widget();

            // pattern a envisager
            // param.reset().size(bidule);
            // ui.widget(param);
        }

        // main content
        let content = {
            ui.size(UISize::pct(1.), UISize::pct(0.8));
            ui.color_rgb(230, 230, 230);
            ui.widget()
        };

        // white box
        let mid = {
            ui.parent(content);

            // stupid spacer
            ui.size(UISize::pct(0.), UISize::px(100.));
            ui.widget();

            ui.size(UISize::pct(0.5), UISize::pct(0.5));
            ui.color_rgb(255, 255, 255);
            ui.layout(1);
            ui.widget()
        };

        // button
        {
            ui.parent(mid);
            ui.color(blue);
            ui.size(UISize::pct(0.6), UISize::px(30.));
            if ui.button(Some("Click here!")).borrow().clicked() {
                count += 1;
            }
            ui.color_rgb(0, 0, 0);
            ui.label(format!("You clicked... {} times!", count).as_str());
        }
    });
}
