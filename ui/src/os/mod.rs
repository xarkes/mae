use crate::imui;

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "windows"),
    not(target_os = "android")
))]
compile_error!("Support for targeted OS is not implemented!",);

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod os_impl;

#[cfg(target_os = "android")]
mod android;
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
