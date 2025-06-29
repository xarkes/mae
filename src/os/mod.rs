use crate::imui;

#[cfg(target_os = "macos")]
include!("macos.rs");
#[cfg(target_os = "linux")]
include!("linux.rs");
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows::*;

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "windows")
))]
compile_error!("Support for target OS is not implemented!");

#[derive(PartialEq, Eq)]
pub enum OSEventType {
    MouseMove,
    Press,
    Release,
}
#[derive(PartialEq, Eq)]
pub enum OSKey {
    LeftMouseButton,
}
pub struct OSEvent {
    pub ty: OSEventType,
    pub key: OSKey,
    pub pos: imui::Point,
}

// TODO: declare trait to retrieve view/display depending on OS
// + declare window base as base structure for shared properties (homemade inheritance)
#[cfg(target_os = "macos")]
pub struct Window {
    // app: Retained<NSApplication>,
    // window: OnceCell<Retained<NSWindow>>,
    pub view: OnceCell<Retained<NSView>>,
}
#[cfg(target_os = "linux")]
pub struct Window {
    size: (f32, f32),
    pub display: *mut x11::xlib::Display,
    pub win: u64,
}

#[cfg(target_os = "windows")]
pub type Window = windows::Window;

#[cfg(target_os = "windows")]
pub fn timer_init() -> f64 {
    #[cfg(target_os = "windows")]
    windows::timer_init()
}
#[cfg(target_os = "windows")]
pub fn timer_value() -> f64 {
    #[cfg(target_os = "windows")]
    windows::timer_value()
}
