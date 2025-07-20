use android_activity::AndroidApp;
use log::info;

use mae::imui::color_rgb;
use mae::imui::UISize;
use mae::imui::UITextAlign;
use mae::imui::IMUI;
use mae::os;

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Trace)
            .with_tag("xark.es.mae")
            .with_filter(
                android_logger::FilterBuilder::new()
                    .parse("debug,android_activity::activity_impl=error")
                    .build(),
            ),
    );
    log::debug!("android_main");
    let mut ui = IMUI::android(app);
    log::debug!("UI initialized");

    let mut count = 0;
    let mut buffer = String::from("Bonjour");
    // Notes
    // What we see from this experiment is that the UISize Pixel is not something good for cross platform development as it would make everything too small on Android devices
    // I guess there is a good ratio to be found, and we should also provide some way to allow people to zoom in or zoom out, in particular by using the UI scale System feature on Android
    // Or maybe "em" like
    // I need to fix the font size as well
    let ui_scale = 4.;
    ui.eventloop(|ui| {
        let root = ui.root.clone();
        let blue = color_rgb(61, 78, 219);
        let white = color_rgb(255, 255, 255);
        let black = color_rgb(0, 0, 0);

        // top bar
        {
            ui.params()
                .parent(root.clone())
                .size(UISize::Percents(1.), UISize::Pixels(40. * ui_scale))
                .color(blue);
            ui.widget();
        }

        // main content
        ui.params()
            .parent(root.clone())
            .size(UISize::Percents(1.), UISize::Percents(1.))
            .color(color_rgb(230, 230, 230));
        let content = ui.widget();

        ui.params()
            .size(UISize::Percents(1.), UISize::Pixels(14. * ui_scale))
            .parent(content.clone())
            .text_align(UITextAlign::Center)
            .color(black);
        ui.label("Your vault is locked.");
        ui.rparams()
            .size(UISize::Percents(1.), UISize::Pixels(100. * ui_scale));
        ui.label("someone@somewhere.com");

        // white box
        ui.params()
            .parent(content.clone())
            .size(UISize::Percents(1.), UISize::Pixels(100. * ui_scale))
            .color(Color {
                r: 0.,
                g: 0.,
                b: 0.,
                a: 0.,
            });
        ui.widget();
        ui.params()
            .parent(content.clone())
            .size(UISize::Percents(0.5), UISize::Percents(0.5))
            .color(white);
        let mid = ui.widget();

        ui.params()
            .parent(mid.clone())
            .size(UISize::Percents(0.6), UISize::Pixels(30. * ui_scale))
            .color(blue);
        ui.line_edit(&buffer, None);
        if ui.button(Some("Click here!")).borrow().clicked() {
            count += 1;
        }
        ui.label(format!("Count: {}", count).as_str());
    });
}
