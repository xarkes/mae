fn main() {
    // XXX: In the future should we write our own X11 library? This way we do not require any system dependency
    // (although libX11 is likely present on every linux running Xorg)
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=X11");

    // XXX: This is needed because we use x11::glx bindings...
    // Default opengl bindings dynamically load the functions
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=GLX");
}
