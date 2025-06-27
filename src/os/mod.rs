#[cfg(target_os = "macos")]
include!("window_macos.rs");
#[cfg(target_os = "linux")]
include!("window_linux.rs");

// QUESTION(xarkes): is it a good way to do it? one thing which is annoying is that there is no "interface" declaration telling what the window_xxx.rs should implement

#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
compile_error!("Support for target OS is not implemented!");

#[derive(PartialEq, Eq)]
pub enum OSEventType {
    MouseMove,
    Press,
    Release,
    Unknown,
}
#[derive(PartialEq, Eq)]
pub enum OSKey {
    LeftMouseButton,
    Unknown,
}
pub struct OSEvent {
    pub ty: OSEventType,
    pub key: OSKey,
    pub pos: (f32, f32),
}

#[cfg(target_os = "macos")]
pub struct Window {
    // app: Retained<NSApplication>,
    // window: OnceCell<Retained<NSWindow>>,
    pub view: OnceCell<Retained<NSView>>,
}
#[cfg(target_os = "linux")]
pub struct Window {
    pub display: *mut x11::xlib::Display,
    pub win: u64
}
