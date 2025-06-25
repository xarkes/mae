#[cfg(target_os = "macos")]
include!("window_macos.rs");

#[cfg(all(not(target_os = "macos")))]
compile_error!("Support for target OS is not implemented!");

pub enum WindowEventType {
    MouseMove,
    Unknown,
}
pub struct WindowEvent {
    pub ty: WindowEventType,
    pub data0: f32,
    pub data1: f32,
}

pub struct Window {
    #[cfg(target_os = "macos")]
    app: Retained<NSApplication>,
    #[cfg(target_os = "macos")]
    window: OnceCell<Retained<NSWindow>>,
    #[cfg(target_os = "macos")]
    pub view: OnceCell<Retained<NSView>>,
}
