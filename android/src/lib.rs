use android_activity::{
    input::{InputEvent, KeyAction, KeyEvent, KeyMapChar, MotionAction},
    AndroidApp, InputStatus, MainEvent, PollEvent,
};
use log::info;

use mae::imui::color_rgb;
use mae::imui::UISize;
use mae::imui::UITextAlign;
use mae::imui::IMUI;
use mae::os;

#[no_mangle]
fn android_main(app: AndroidApp) {
    // TODO: Rename library name from 'main' to something catchable in the logs
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Trace)
            .with_tag("xark.es.mae"),
    );
    log::debug!("android_main");

    let mut quit = false;
    let mut redraw_pending = true;
    let mut native_window: Option<ndk::native_window::NativeWindow> = None;

    // let mut combining_accent = None;

    let mut ui = None;
    while ui.is_none() {
        app.poll_events(
            Some(std::time::Duration::from_secs(1)),
            |event| match event {
                PollEvent::Main(main_event) => match main_event {
                    MainEvent::InitWindow { .. } => {
                        native_window = app.native_window();
                        ui = Some(IMUI::mobile(mae::os::Window { app: app.clone() }));
                    }
                    _ => {}
                },
                _ => {}
            },
        );
    }

    log::debug!("UI initialized?");
    let mut ui = ui.unwrap();

    let mut count = 0;
    let mut buffer = String::from("Bonjour");
    ui.eventloop(|ui| {
        let root = ui.root.clone();
        let blue = color_rgb(61, 78, 219);
        let white = color_rgb(255, 255, 255);
        let black = color_rgb(0, 0, 0);

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
            .color(color_rgb(230, 230, 230));
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
            .color(mae::imui::color::NONE);
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
