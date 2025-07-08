use crate::os::Window;
use log;
extern crate khronos_egl;
// Android implementation of OpenGL using EGL.
// Currently we expect the MainActivity to inherit from GameActivity
// android_activity crate will compile and bundle the game-activity AOSP C++ library
// in a similar fashion you would link your OpenGL application to game-activity lib in Android Studio

// TODO(xarkes): Make ogl_* functions impl of GLContextHandle
pub struct GLContextHandle {
    pub egl: khronos_egl::DynamicInstance<khronos_egl::EGL1_4>,
    pub display: khronos_egl::Display,
    pub surface: khronos_egl::Surface,
}
pub type GLStringPtr = *const u8;
pub fn ogl_create_context(win: &Window) -> GLContextHandle {
    // TODO: dlopen in favor of libloading maybe?
    let lib = unsafe { libloading::Library::new("libEGL.so").expect("unable to find libEGL.so") };

    let egl = unsafe {
        khronos_egl::DynamicInstance::<khronos_egl::EGL1_4>::load_required_from(lib)
            .expect("unable to load libEGL.so")
    };
    log::debug!("{:?}", egl);

    let display = unsafe { egl.get_display(khronos_egl::DEFAULT_DISPLAY).unwrap() };
    log::debug!("Display: {:?}", display);
    egl.initialize(display);

    let attributes = [
        khronos_egl::RENDERABLE_TYPE,
        khronos_egl::OPENGL_ES3_BIT,
        khronos_egl::SURFACE_TYPE,
        khronos_egl::WINDOW_BIT,
        khronos_egl::BLUE_SIZE,
        8,
        khronos_egl::GREEN_SIZE,
        8,
        khronos_egl::RED_SIZE,
        8,
        khronos_egl::DEPTH_SIZE,
        24,
        khronos_egl::NONE,
    ];

    // get the number of matching configurations.
    let count = egl.matching_config_count(display, &attributes).unwrap();

    // get the matching configurations
    let mut configs = Vec::with_capacity(count);
    egl.choose_config(display, &attributes, &mut configs)
        .unwrap();
    log::debug!("Got {} configs", configs.len());
    let mut config = None;
    for cfg in configs {
        if egl
            .get_config_attrib(display, cfg, khronos_egl::RED_SIZE)
            .unwrap()
            == 8
            && egl
                .get_config_attrib(display, cfg, khronos_egl::GREEN_SIZE)
                .unwrap()
                == 8
            && egl
                .get_config_attrib(display, cfg, khronos_egl::BLUE_SIZE)
                .unwrap()
                == 8
            && egl
                .get_config_attrib(display, cfg, khronos_egl::DEPTH_SIZE)
                .unwrap()
                == 24
        {
            log::debug!("Found config");
            config = Some(cfg);
            break;
        }
    }

    let config = match config {
        Some(cfg) => cfg,
        None => egl
            .choose_first_config(display, &attributes)
            .unwrap()
            .expect("unable to find an appropriate ELG configuration"),
    };

    let fmt = egl.get_config_attrib(display, config, khronos_egl::NATIVE_VISUAL_ID);
    log::debug!("Format: {:?}", fmt);

    let surface = unsafe {
        egl.create_window_surface(
            display,
            config,
            win.app.native_window().unwrap().ptr().as_ptr() as _,
            None,
        )
        .unwrap()
    };
    log::debug!("Surface: {:?}", surface);

    let context_attributes = [khronos_egl::CONTEXT_MAJOR_VERSION, 3, khronos_egl::NONE];
    let context = egl
        .create_context(display, config, None, &context_attributes)
        .unwrap();
    log::debug!("Window: {:?}", win.app.native_window());

    let asd = egl.make_current(display, Some(surface), Some(surface), Some(context));
    log::debug!("Make current: {:?}", asd);
    // .expect("Failed to create EGL context");
    gl::load_with(|symbol| unsafe {
        match egl.get_proc_address(symbol) {
            Some(addr) => {
                // log::debug!("{} -> {:?}", symbol, addr);
                addr as *const std::ffi::c_void
            }
            None => std::ptr::null(),
        }
    });

    GLContextHandle {
        egl,
        display,
        surface,
    }
}
pub fn ogl_toggle_vsync(ctx: &GLContextHandle, toggle: bool) {}
pub fn ogl_resize(ctx: &GLContextHandle) -> (f32, f32) {
    let width = ctx
        .egl
        .query_surface(ctx.display, ctx.surface, khronos_egl::WIDTH)
        .unwrap();
    let height = ctx
        .egl
        .query_surface(ctx.display, ctx.surface, khronos_egl::HEIGHT)
        .unwrap();
    log::debug!("{}x{}", width, height);
    (width as f32, height as f32)
}
pub fn ogl_swapbuffers(ctx: &GLContextHandle) {
    ctx.egl.swap_buffers(ctx.display, ctx.surface);
}
