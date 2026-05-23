extern crate objc2;
use crate::os::Window;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
#[allow(deprecated)]
use objc2_app_kit::NSOpenGLContextParameter;
use std::ffi::CStr;

enum NSOpenGLPFA {
    // AllRenderers = 1,
    // TripleBuffer = 3,
    DoubleBuffer = 5,
    // AuxBuffers = 7,
    ColorSize = 8,
    AlphaSize = 11,
    DepthSize = 12,
    StencilSize = 13,
    // AccumSize = 14,
    // MinimumPolicy = 51,
    // MaximumPolicy = 52,
    SampleBuffers = 55,
    Samples = 56,
    // AuxDepthStencil = 57,
    // ColorFloat = 58,
    // Multisample = 59,
    // Supersample = 60,
    // SampleAlpha = 61,
    // RendererID = 70,
    // NoRecovery = 72,
    Accelerated = 73,
    ClosestPolicy = 74,
    // BackingStore = 76,
    // ScreenMask = 84,
    // AllowOfflineRenderers = 96,
    // AcceleratedCompute = 97,
    OpenGLProfile = 99,
    // VirtualScreenCount = 128,

    // Stereo = 6,
    // OffScreen = 53,
    // FullScreen = 54,
    // SingleRenderer = 71,
    // Robust = 75,
    // MPSafe = 78,
    // Window = 80,
    // MultiScreen = 81,
    // Compliant = 83,
    // PixelBuffer = 90,
    // RemotePixelBuffer = 91,
}

enum NSOpenGLProfile {
    // VersionLegacy = 0x1000,
    Version3_2Core = 0x3200,
    // Version4_1Core = 0x4100,
}

pub type GLContextHandle = *mut AnyObject;
pub type GLStringPtr = *const i8;

pub fn ogl_create_context(win: &Window) -> *mut AnyObject {
    let class_name = CStr::from_bytes_with_nul(b"NSOpenGLPixelFormat\0").unwrap();
    let class_name_ctx = CStr::from_bytes_with_nul(b"NSOpenGLContext\0").unwrap();
    let attrs = [
        NSOpenGLPFA::Accelerated as u32,
        NSOpenGLPFA::ClosestPolicy as u32,
        NSOpenGLPFA::OpenGLProfile as u32,
        NSOpenGLProfile::Version3_2Core as u32,
        NSOpenGLPFA::ColorSize as u32,
        24,
        NSOpenGLPFA::AlphaSize as u32,
        8,
        NSOpenGLPFA::DepthSize as u32,
        24,
        NSOpenGLPFA::StencilSize as u32,
        8,
        NSOpenGLPFA::DoubleBuffer as u32,
        NSOpenGLPFA::SampleBuffers as u32,
        1,
        NSOpenGLPFA::Samples as u32,
        4,
        0,
    ];

    let ctx: *mut AnyObject;
    // SAFETY(xarkes): This is just basic Objective C calls, if there is any problem, the Objective C runtime will handle it.
    unsafe {
        let pixel_format_class = AnyClass::get(&class_name).unwrap();
        let pixel_format: *mut AnyObject = msg_send![pixel_format_class, alloc];
        let pixel_format: *mut AnyObject =
            msg_send![pixel_format, initWithAttributes: attrs.as_ptr()];

        let context_class = AnyClass::get(&class_name_ctx).unwrap();
        let context: *mut AnyObject = msg_send![context_class, alloc];
        let context: *mut AnyObject = msg_send![context, initWithFormat: pixel_format, shareContext: std::ptr::null_mut() as *mut AnyObject];
        println!("Context: {:?}", context);

        // xarkes: Attach OpenGL to the window's NSView, and make it current context
        let view = win.view.get().unwrap();
        let _: () = msg_send![context, setView: Retained::as_ptr(view)];
        let _: () = msg_send![context, makeCurrentContext];
        let surface_opacity = 0i32;
        #[allow(deprecated)]
        let _: () = msg_send![
            context,
            setValues: &surface_opacity,
            forParameter: NSOpenGLContextParameter::SurfaceOpacity
        ];
        ctx = context;
    }

    // NOTE(xarkes): This is a bit hacky, but objc2 doesn't expose the NSOpenGL stuff, hence the code above.
    // OpenGL is deprecated on MacOS, so we should probably not support it, but using it eases the cross platform development.
    unsafe extern "C" {
        fn dlsym(handle: *mut std::ffi::c_void, symbol: *const i8) -> *mut std::ffi::c_void;
        fn dlopen(path: *const i8, mode: u32) -> *mut std::ffi::c_void;
    }
    let lib_ptr = unsafe {
        dlopen(
            std::ffi::CString::new(
                "/System/Library/Frameworks/OpenGL.framework/Versions/A/Libraries/libGL.dylib",
            )
            .unwrap()
            .as_ptr(),
            2,
        )
    };
    if lib_ptr == std::ptr::null_mut() {
        panic!("Could not load libGL, renderer cannot be used.");
    }

    // SAFETY(xarkes): We trust the OS to give us valid pointers with dlopen and dlsym
    gl::load_with(|symbol| unsafe {
        let symbol = std::ffi::CString::new(symbol).unwrap();
        let addr = dlsym(lib_ptr, symbol.as_ptr());
        addr as *const std::ffi::c_void
    });

    ctx
}

pub fn ogl_resize(ctx: &GLContextHandle) {
    unsafe {
        let _: () = msg_send![*ctx, update];
    }
}

pub fn ogl_swapbuffers(ctx: &GLContextHandle) {
    unsafe {
        let _: () = msg_send![*ctx, flushBuffer];
    }
}

pub fn ogl_toggle_vsync(ctx: &GLContextHandle, enable: bool) {
    unsafe {
        let val = match enable {
            true => 1i32,
            false => 0i32,
        };
        #[allow(deprecated)]
        let _: () =
            msg_send![*ctx, setValues: &val, forParameter:NSOpenGLContextParameter::SwapInterval];
    }
}

pub fn ogl_destroy_context(ctx: &mut GLContextHandle) {
    if ctx.is_null() {
        return;
    }

    unsafe {
        let class_name_ctx = CStr::from_bytes_with_nul(b"NSOpenGLContext\0").unwrap();
        let context_class = AnyClass::get(&class_name_ctx).unwrap();
        let _: () = msg_send![*ctx, clearDrawable];
        let _: () = msg_send![*ctx, setView: std::ptr::null_mut::<AnyObject>()];
        let _: () = msg_send![context_class, clearCurrentContext];
        drop(Retained::from_raw(*ctx));
        *ctx = std::ptr::null_mut();
    }
}
