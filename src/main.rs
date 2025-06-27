mod draw;
mod imui;
mod os;
mod render;
mod widgets;

use imui::UISize;

// TODO(xarkes):
// - [ ] XXX: Urgent: take a decision regarding the APIs. Should we work with u32 (pixels) or floats? Currently it is a bit a mix of everything and we have to decide which one to use and stick to it.
// - [ ] Add proper logging
// - [ ] Draw the interface as you'd like it
// - [ ] Handle events (mouse over, mouse click, keyboard inputs, ...)
// - [ ] Port to Linux

fn main() {
    let mut ui = imui::create_window(1024, 768);
    let mut val = 1234;
    ui.eventloop(|ui| {
        ui.parent(ui.root.clone());
        ui.size(UISize::pct(0.2), UISize::px(20.));
        let but = ui.button(Some("Click me"));
        if but.borrow().clicked() {
            val += 1;
        }
        ui.size(UISize::pct(0.2), UISize::px(20.));
        ui.label(format!("Yooo {}", val).as_str());
        ui.label(format!("Yooo {}", val).as_str());
        ui.label(format!("Yooo {}", val).as_str());
        ui.label(format!("Yooo {}", val).as_str());
        ui.label(format!("Yooo {}", val).as_str());
        ui.label(format!("Yooo {}", val).as_str());
        ui.label(format!("Yooo {}", val).as_str());
        ui.label(format!("Yooo {}", val).as_str());
    });
}
