// mod draw;
// mod imui;
// mod os;
// mod render;

use android_activity::AndroidApp;
use android_logger::Config;
use log::LevelFilter;
use std::panic::catch_unwind;

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Trace),
    );
    log::info!("This is android_main()");
}
