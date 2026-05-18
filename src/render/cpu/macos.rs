#![allow(deprecated)]

use crate::os::Window;

use objc2::{AnyThread, MainThreadOnly, msg_send, rc::Retained};
use objc2_app_kit::{
    NSAlphaNonpremultipliedBitmapFormat, NSAutoresizingMaskOptions, NSBitmapImageRep,
    NSDeviceRGBColorSpace, NSImage, NSImageScaling, NSImageView, NSView, NSWindow,
};
use objc2_foundation::{MainThreadMarker, NSInteger, NSPoint, NSRect, NSSize};

pub struct CPUContextHandle {
    window: Retained<NSWindow>,
    view: Retained<NSView>,
    image_view: Option<Retained<NSImageView>>,
    image: Option<Retained<NSImage>>,
    bitmap: Option<Retained<NSBitmapImageRep>>,
    width: usize,
    height: usize,
}

impl Drop for CPUContextHandle {
    fn drop(&mut self) {
        if let Some(image_view) = self.image_view.take() {
            image_view.removeFromSuperview();
        }
    }
}

pub fn cpu_create_context(win: &Window) -> CPUContextHandle {
    CPUContextHandle {
        window: win.window.get().unwrap().clone(),
        view: win.view.get().unwrap().clone(),
        image_view: None,
        image: None,
        bitmap: None,
        width: 0,
        height: 0,
    }
}

pub fn cpu_swapbuffers(
    ctx: &mut CPUContextHandle,
    framebuffer: &[u32],
    width: usize,
    height: usize,
) {
    if width == 0 || height == 0 || framebuffer.len() < width * height {
        return;
    }

    ensure_bitmap(ctx, width, height);
    let Some(bitmap) = ctx.bitmap.as_ref() else {
        return;
    };

    unsafe {
        copy_argb_to_rgba_ptr(framebuffer, bitmap.bitmapData(), width * height);
    }

    if let (Some(image), Some(image_view)) = (ctx.image.as_ref(), ctx.image_view.as_ref()) {
        image.recache();
        image_view.setFrame(ctx.view.bounds());
        unsafe {
            let _: () = msg_send![Retained::as_ptr(image_view), setNeedsDisplay: true];
            let _: () = msg_send![Retained::as_ptr(image_view), displayIfNeeded];
        }
        ctx.window.flushWindowIfNeeded();
    }
}

fn ensure_bitmap(ctx: &mut CPUContextHandle, width: usize, height: usize) {
    ensure_image_view(ctx);

    if ctx.bitmap.is_some() && ctx.width == width && ctx.height == height {
        return;
    }

    let mut planes: [*mut u8; 5] = [std::ptr::null_mut(); 5];
    let bitmap = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            planes.as_mut_ptr(),
            width as NSInteger,
            height as NSInteger,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            NSAlphaNonpremultipliedBitmapFormat,
            (width * 4) as NSInteger,
            32,
        )
    };

    ctx.bitmap = bitmap;
    ctx.width = width;
    ctx.height = height;

    let logical_bounds = ctx.view.bounds();
    let pixel_bounds = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(width as f64, height as f64),
    );
    if let Some(bitmap) = ctx.bitmap.as_ref() {
        bitmap.setSize(logical_bounds.size);
        bitmap.setPixelsWide(pixel_bounds.size.width as NSInteger);
        bitmap.setPixelsHigh(pixel_bounds.size.height as NSInteger);
    }

    if let Some(bitmap) = ctx.bitmap.as_ref() {
        let image = NSImage::initWithSize(NSImage::alloc(), logical_bounds.size);
        unsafe {
            let _: () = msg_send![
                Retained::as_ptr(&image),
                addRepresentation: Retained::as_ptr(bitmap)
            ];
        }
        if let Some(image_view) = ctx.image_view.as_ref() {
            image_view.setImage(Some(&image));
        }
        ctx.image = Some(image);
    }
}

fn ensure_image_view(ctx: &mut CPUContextHandle) {
    if ctx.image_view.is_some() {
        return;
    }

    let mtm = MainThreadMarker::new().expect("macOS CPU renderer must run on the main thread");
    let image_view = NSImageView::initWithFrame(NSImageView::alloc(mtm), ctx.view.bounds());
    image_view.setImageScaling(NSImageScaling::ScaleAxesIndependently);
    image_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    ctx.view.addSubview(&image_view);
    ctx.image_view = Some(image_view);
}

unsafe fn copy_argb_to_rgba_ptr(src: &[u32], dst: *mut u8, pixel_count: usize) {
    debug_assert!(src.len() >= pixel_count);
    debug_assert!(!dst.is_null());

    for (idx, argb) in src.iter().take(pixel_count).enumerate() {
        let base = idx * 4;
        unsafe {
            *dst.add(base) = ((argb >> 16) & 0xFF) as u8;
            *dst.add(base + 1) = ((argb >> 8) & 0xFF) as u8;
            *dst.add(base + 2) = (argb & 0xFF) as u8;
            *dst.add(base + 3) = ((argb >> 24) & 0xFF) as u8;
        }
    }
}
