use crate::imui;

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

pub type Window = os_impl::Window;

pub fn timer_init() -> f64 {
    os_impl::timer_init()
}
pub fn timer_value() -> u64 {
    os_impl::timer_value()
}
