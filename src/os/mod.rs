use crate::imui;

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "windows"),
    not(target_os = "android")
))]
compile_error!("Support for targeted OS is not implemented!",);

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as os_impl;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as os_impl;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as os_impl;

#[cfg(target_os = "android")]
mod android {
    use super::OSEvent;
    pub struct Window {}
    impl Window {
        pub fn new(width: u32, height: u32) -> Self {
            Window {}
        }
        pub fn get_size(&self) -> (f32, f32) {
            (1024., 768.)
        }
        pub fn get_events(&self) -> Vec<OSEvent> {
            let events = Vec::new();
            events
        }
    }
    pub fn timer_init() -> f64 {
        1.
    }
    pub fn timer_value() -> u64 {
        1
    }
}
#[cfg(target_os = "android")]
use android as os_impl;

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

pub type Window = os_impl::Window;

pub fn timer_init() -> f64 {
    os_impl::timer_init()
}
pub fn timer_value() -> u64 {
    os_impl::timer_value()
}
