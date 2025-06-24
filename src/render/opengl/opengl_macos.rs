extern crate objc2;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
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

type GLContextHandle = *mut AnyObject;

pub fn ogl_os_create_context(win: &Window) -> *mut AnyObject {
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
        let view = win.view.get().unwrap().clone();
        let _: () = msg_send![context, setView: Retained::into_raw(view)];
        let _: () = msg_send![context, makeCurrentContext];
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

    // SAFETY(xarkes): The pointers come back from OpenGL library.
    // We also assume the GetString function was resolved earlier, if not it will simply result in a null deref.
    unsafe {
        let vendor = gl::GetString(gl::VENDOR) as *mut i8;
        let version = gl::GetString(gl::VERSION) as *mut i8;
        if vendor != std::ptr::null_mut() && version != std::ptr::null_mut() {
            let vendorstr = std::ffi::CStr::from_ptr(vendor).to_str().expect("<err>");
            let versionstr = std::ffi::CStr::from_ptr(version).to_str().expect("<err>");
            println!("OpenGL vendor: {} - version: {}", vendorstr, versionstr);
        } else {
            println!("Could not retrieve OpenGL vendor and version!");
        }
    }

    ctx
}

pub fn ogl_os_resize(ctx: *mut AnyObject) {
    unsafe {
        let _: () = msg_send![ctx, update];
    }
}

pub fn ogl_os_swapbuffers(ctx: *mut AnyObject) {
    unsafe {
        let _: () = msg_send![ctx, flushBuffer];
    }
}

pub fn ogl_os_toggle_vsync(ctx: *mut AnyObject, enable: bool) {
    unsafe {
        #[allow(non_snake_case)]
        let NSOpenGLContextParameterSwapInterval = 222i64;
        let val = match enable {
            true => 0i32,
            false => 1i32,
        };
        let _: () =
            msg_send![ctx, setValues: &val, forParameter:NSOpenGLContextParameterSwapInterval];
    }
}
