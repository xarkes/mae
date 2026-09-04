use super::*;

impl IMUI {
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    pub fn new(w: u32, h: u32, title: &str) -> Self {
        let window = os::Window::new(w, h, title);
        Self::new_body(window)
    }

    #[cfg(target_os = "android")]
    pub fn android(app: AndroidApp) -> Self {
        let win = os::Window::new(app);
        win.wait_for_native_window();
        Self::new_body(win)
    }

    pub(super) fn new_body(window: os::Window) -> Self {
        let renderer = render::Renderer::new(window);
        let drawer = Drawer::new(renderer);
        Self::with_drawer(Some(drawer), Size::default())
    }

    pub(super) fn os_window(&self) -> Option<&os::Window> {
        self.drawer.as_ref().map(|d| &d.renderer.win)
    }

    pub(super) fn os_window_mut(&mut self) -> Option<&mut os::Window> {
        self.drawer.as_mut().map(|d| &mut d.renderer.win)
    }

    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn new_for_test(w: f32, h: f32) -> Self {
        Self::with_drawer(
            None,
            Size {
                width: w,
                height: h,
            },
        )
    }

    pub(super) fn with_drawer(drawer: Option<Drawer>, size: Size) -> Self {
        let theme = UITheme::default();
        let mut ui = Self {
            drawer,
            size,
            events: Vec::new(),
            frame_presses: Vec::new(),
            mouse: None,
            left_mouse_down: false,
            right_mouse_down: false,
            hot_key: None,
            prev_hot_key: None,
            pointer_blacklist_rects: Vec::new(),
            scrollbar_hit_areas: Vec::new(),
            active_left_key: None,
            active_right_key: None,
            drag_start_mouse: None,
            text_click_streak: TextClickStreak::default(),
            active_scrollbar: None,
            focus_key: None,
            next_focus_key: None,
            ime_preedit: None,
            cursor: OSCursor::Arrow,
            text_edit_states: HashMap::new(),
            editor_layouts: HashMap::new(),
            line_scratch: LineScratch::default(),
            string_pool: StringPool::default(),
            undo_states: HashMap::new(),
            markdown_mode: MarkdownMode::Source,
            clipboard: String::new(),
            images: HashMap::new(),
            requested_images: std::collections::HashSet::new(),
            images_rev: 0,
            pasted_image: None,
            images_to_free: Vec::new(),
            image_drag: None,
            image_resize_out: None,
            image_unsynced: std::collections::HashSet::new(),
            external_repaint: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            remote_carets: HashMap::new(),
            boxes: Vec::new(),
            box_table: HashMap::new(),
            free_boxes: Vec::new(),
            frame_boxes: Vec::new(),
            canvas_paints: Vec::new(),
            root: 0,
            overlay_root: 0,
            parent_stack: Vec::new(),
            build_index: 0,
            render_continuously: false,
            pending_resize_redraws: 0,
            vsync_enabled: true,
            cap_fps_to_refresh_rate: true,
            refresh_rate_hz: 60.0,
            repaint_requested: true,
            quit_requested: false,
            timer_frequency: os::timer_init(),
            last_frame_time: 0.0,
            animation_dt: 1.0 / 60.0,
            fps_window_start: 0.0,
            fps_frame_count: 0,
            fps: 0.0,
            toasts: Vec::new(),
            next_toast_id: 0,
            theme,
            #[cfg(target_arch = "wasm32")]
            tick_scheduler: std::rc::Rc::new(std::cell::RefCell::new(None)),
        };
        if let Some(drawer) = ui.drawer.as_mut() {
            drawer.renderer.vsync(ui.vsync_enabled);
            ui.refresh_rate_hz = drawer.renderer.win.refresh_rate_hz();
        }
        let now = ui.now_seconds();
        ui.last_frame_time = now;
        ui.fps_window_start = now;
        ui.begin_frame();
        ui
    }

    pub fn eventloop(&mut self, mut build_ui_func: impl FnMut(&mut IMUI)) {
        self.eventloop_with_shutdown(|ui| build_ui_func(ui), |_| {});
    }

    pub fn eventloop_with_shutdown(
        &mut self,
        mut build_ui_func: impl FnMut(&mut IMUI),
        mut shutdown_func: impl FnMut(&mut IMUI),
    ) {
        loop {
            self.pull_consume_events();
            if self
                .external_repaint
                .swap(false, std::sync::atomic::Ordering::Acquire)
            {
                self.repaint_requested = true;
            }
            let had_events = !self.events.is_empty();
            let mut resized = false;
            if let Some(maybe_new_size) = self.os_window_mut().map(|w| w.get_size()) {
                if maybe_new_size.0 != self.size.width || maybe_new_size.1 != self.size.height {
                    self.resize();
                    resized = true;
                }
            }

            if had_events || resized {
                self.repaint_requested = true;
            }
            if resized {
                // The first frame after a resize lands in a stale-sized back
                // buffer; queue extra redraws so a correctly-sized frame is
                // presented even if we'd otherwise go idle. See field docs.
                self.pending_resize_redraws = 2;
            }

            if self.quit_requested {
                break;
            }

            let force_redraw = self.pending_resize_redraws > 0;
            if !self.render_continuously && !self.repaint_requested && !force_redraw {
                std::thread::sleep(core::time::Duration::from_millis(16));
                continue;
            }

            self.pending_resize_redraws = self.pending_resize_redraws.saturating_sub(1);
            self.repaint_requested = false;
            self.sleep_for_fps_cap();
            self.begin_frame();
            build_ui_func(self);
            self.end_frame();
            self.update_fps();

            if self.quit_requested {
                break;
            }
        }
        shutdown_func(self);
    }

    /// Browser equivalent of `eventloop`: `requestAnimationFrame`-driven
    /// instead of a blocking `loop { sleep(..) }` (which can't run on the JS
    /// main thread — there is no way to block it without freezing the page).
    /// Consumes `self`, since the frame-to-frame state now lives in an
    /// `Rc<RefCell<_>>` shared with the recursively-rescheduled callback.
    ///
    pub fn request_quit(&mut self) {
        self.quit_requested = true;
        self.request_repaint();
    }

    /// A clonable, thread-safe handle that wakes the event loop for a rebuild.
    /// Hand it to background threads (sync engine, timers) so the UI reacts to
    /// their events without rendering continuously.
    pub fn repaint_waker(&self) -> RepaintWaker {
        RepaintWaker {
            flag: self.external_repaint.clone(),
            #[cfg(target_arch = "wasm32")]
            scheduler: self.tick_scheduler.clone(),
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        let now = self.now_seconds();
        self.animation_dt = (now - self.last_frame_time).clamp(1.0 / 240.0, 1.0 / 15.0) as f32;
        self.last_frame_time = now;
        self.capture_pointer_blacklist_rects();
        self.capture_scrollbar_hit_areas();
        self.build_index += 1;
        self.remote_carets.clear();
        self.image_unsynced.clear();
        self.frame_boxes.clear();
        self.canvas_paints.clear();
        self.parent_stack.clear();
        self.cursor = OSCursor::Arrow;
        self.hot_key = None;
        self.focus_key = self.next_focus_key.take().or(self.focus_key);

        let root = self.alloc_box(Some("#root"), UIBoxFlags::NONE);
        self.root = root.idx;
        self.parent_stack.push(root.idx);
        self.boxes[root.idx].pref_size = [
            UISize::Pixels(self.size.width),
            UISize::Pixels(self.size.height),
        ];
        self.boxes[root.idx].computed_size = self.size;
        self.boxes[root.idx].rect =
            RectCoords::from_size(0.0, 0.0, self.size.width, self.size.height);
        self.boxes[root.idx].child_layout_axis = Axis::X;

        let overlay_root = self.alloc_box(Some("###overlay_root"), UIBoxFlags::NONE);
        self.overlay_root = overlay_root.idx;
        self.boxes[overlay_root.idx].flags |= UIBoxFlags::FLOATING_X | UIBoxFlags::FLOATING_Y;
        self.boxes[overlay_root.idx].fixed_position = Point::new(0.0, 0.0);
        self.boxes[overlay_root.idx].pref_size = [
            UISize::Pixels(self.size.width),
            UISize::Pixels(self.size.height),
        ];
        self.boxes[overlay_root.idx].computed_size = self.size;
        self.boxes[overlay_root.idx].rect =
            RectCoords::from_size(0.0, 0.0, self.size.width, self.size.height);
        self.boxes[overlay_root.idx].child_layout_axis = Axis::Y;
    }

    pub(crate) fn end_frame(&mut self) {
        self.render_toasts();
        self.animate_scroll_offsets();
        self.layout_root(self.root);
        self.apply_textarea_mouse_selections();
        self.update_focused_textarea_scroll();
        self.resolve_cursor();
        self.animate_visual_state();
        self.draw_ui_all();
        self.update_previous_clip_rects();

        if let Some(drawer) = self.drawer.as_mut() {
            drawer.renderer.render_frame();
        }
        let cursor = self.cursor;
        if let Some(win) = self.os_window_mut() {
            win.set_cursor(cursor);
        }

        self.frame_presses.clear();
        self.prune_boxes();
    }

    #[cfg(feature = "testkit")]
    pub(crate) fn end_test_frame(&mut self) -> crate::testkit::UiSnapshot {
        self.render_toasts();
        self.animate_scroll_offsets();
        self.layout_root(self.root);
        self.apply_textarea_mouse_selections();
        self.update_focused_textarea_scroll();
        self.resolve_cursor();
        self.animate_visual_state();
        self.draw_ui_all();
        self.update_previous_clip_rects();
        let snapshot = self.snapshot();
        self.frame_presses.clear();
        self.prune_boxes();
        snapshot
    }

    pub(super) fn now_seconds(&self) -> f64 {
        os::timer_value() as f64 / self.timer_frequency
    }

    pub(super) fn sleep_for_fps_cap(&self) {
        if !self.cap_fps_to_refresh_rate {
            return;
        }

        let refresh_rate_hz = self.refresh_rate_hz.max(1.0) as f64;
        let target_frame_time = 1.0 / refresh_rate_hz;
        let elapsed = self.now_seconds() - self.last_frame_time;
        if elapsed < target_frame_time {
            std::thread::sleep(core::time::Duration::from_secs_f64(
                target_frame_time - elapsed,
            ));
        }
    }

    pub(super) fn update_fps(&mut self) {
        self.fps_frame_count += 1;
        let now = self.now_seconds();
        let elapsed = now - self.fps_window_start;
        if elapsed >= 0.5 {
            self.fps = self.fps_frame_count as f32 / elapsed as f32;
            self.fps_frame_count = 0;
            self.fps_window_start = now;
        }
    }

    pub(crate) fn resize(&mut self) -> Size {
        if let Some(drawer) = self.drawer.as_mut() {
            self.size = Size::from(drawer.renderer.win.get_size());
            let render_size = drawer.renderer.win.get_render_size();
            drawer.renderer.resize(render_size.0, render_size.1);
            self.refresh_rate_hz = drawer.renderer.win.refresh_rate_hz();
        } else if let Some(win) = self.os_window_mut() {
            let new_size = Size::from(win.get_size());
            let hz = win.refresh_rate_hz();
            self.size = new_size;
            self.refresh_rate_hz = hz;
        }
        self.size
    }

    pub fn fps(&self) -> f32 {
        self.fps
    }

    /// Current window/viewport size in logical pixels (width, height).
    /// Seconds elapsed since the previous frame, clamped to `[1/240, 1/15]`.
    ///
    /// The same delta the framework's own hover/focus/scroll easing runs on, so
    /// app-driven animation (a panel that slides, a view that cross-fades) stays
    /// in step with it. Pair with [`crate::imui::smooth_rate`] and
    /// [`crate::imui::animate_scalar`]; hold the animated value in app state
    /// rather than asking the toolkit to retain it, so the per-frame path stays
    /// allocation-free.
    pub fn dt(&self) -> f32 {
        self.animation_dt
    }

    pub fn window_size(&self) -> (f32, f32) {
        (self.size.width, self.size.height)
    }

    /// Set the window's title bar text.
    pub fn set_window_title(&self, title: &str) {
        if let Some(win) = self.os_window() {
            win.set_title(title);
        }
    }

    /// Set the application icon (Dock / app switcher) from encoded PNG bytes.
    pub fn set_app_icon(&self, png_bytes: &[u8]) {
        if let Some(win) = self.os_window() {
            win.set_app_icon(png_bytes);
        }
    }

    pub fn render_continuously(&self) -> bool {
        self.render_continuously
    }

    pub fn set_render_continuously(&mut self, enabled: bool) {
        if self.render_continuously != enabled {
            self.render_continuously = enabled;
            self.request_repaint();
        }
    }

    pub fn renderer_backend(&self) -> render::Backend {
        self.drawer
            .as_ref()
            .map(|drawer| drawer.renderer.backend())
            .unwrap_or_else(render::Backend::default_backend)
    }

    pub fn set_renderer_backend(&mut self, backend: render::Backend) {
        if let Some(drawer) = self.drawer.as_mut() {
            drawer.renderer.set_backend(backend);
            drawer.renderer.vsync(self.vsync_enabled);
            self.refresh_rate_hz = drawer.renderer.win.refresh_rate_hz();
        }
        self.request_repaint();
    }

    pub fn vsync_enabled(&self) -> bool {
        self.vsync_enabled
    }

    pub fn set_vsync_enabled(&mut self, enabled: bool) {
        if self.vsync_enabled != enabled {
            self.vsync_enabled = enabled;
            if let Some(drawer) = self.drawer.as_mut() {
                drawer.renderer.vsync(enabled);
            }
            self.request_repaint();
        }
    }

    pub fn cap_fps_to_refresh_rate(&self) -> bool {
        self.cap_fps_to_refresh_rate
    }

    pub fn set_cap_fps_to_refresh_rate(&mut self, enabled: bool) {
        if self.cap_fps_to_refresh_rate != enabled {
            self.cap_fps_to_refresh_rate = enabled;
            if enabled {
                if let Some(drawer) = self.drawer.as_mut() {
                    self.refresh_rate_hz = drawer.renderer.win.refresh_rate_hz();
                }
            }
            self.request_repaint();
        }
    }

    pub fn refresh_rate_hz(&self) -> f32 {
        self.refresh_rate_hz
    }

    pub fn request_repaint(&mut self) {
        self.repaint_requested = true;
    }

    /// Whether another frame has been asked for, clearing the request — exactly
    /// what [`IMUI::eventloop`] does before it builds.
    ///
    /// For tests that need to model the *on-demand* loop. A test that simply
    /// draws frames in a row can never see work that was deferred to a frame
    /// nobody requested, which is the shape of every "stale until you move the
    /// mouse" bug.
    pub fn take_repaint_request(&mut self) -> bool {
        std::mem::take(&mut self.repaint_requested)
    }

    #[cfg(feature = "png_capture")]
    pub fn request_capture(&mut self, path: impl Into<String>) {
        if let Some(drawer) = self.drawer.as_mut() {
            drawer.renderer.request_capture(path.into());
        }
    }

    pub fn theme(&self) -> &UITheme {
        &self.theme
    }

    pub fn set_theme(&mut self, theme: UITheme) {
        let changed = self.theme.kind != theme.kind;
        let old_bg = self.theme.app_bg;
        let new_bg = theme.app_bg;
        let clear_changed =
            (old_bg.r, old_bg.g, old_bg.b, old_bg.a) != (new_bg.r, new_bg.g, new_bg.b, new_bg.a);
        self.theme = theme;
        // Whatever the UI does not paint shows through as the clear colour —
        // including anything drawn below full opacity, which composites against
        // it. Following `app_bg` means a fading view dissolves toward the
        // theme's own background instead of the default transparent black,
        // which read as a dark flash on a light theme (and the reverse).
        if clear_changed {
            let bg = self.theme.app_bg;
            if let Some(drawer) = self.drawer.as_mut() {
                drawer.renderer.set_clear_color(crate::render::V4f32 {
                    r: bg.r,
                    g: bg.g,
                    b: bg.b,
                    a: bg.a,
                });
            }
        }
        if changed {
            // Cached editor layouts bake in theme colors; drop them so text is
            // re-styled with the new palette (otherwise e.g. dark-theme near-white
            // text stays invisible on the light background).
            self.editor_layouts.clear();
            self.request_repaint();
        }
    }

    pub fn bounds(&self, handle: UIBoxHandle) -> RectCoords {
        self.boxes
            .get(handle.idx)
            .map(|b| b.rect)
            .unwrap_or_else(|| RectCoords::from_size(0.0, 0.0, 0.0, 0.0))
    }

    /// Layout the frame under construction, then snapshot it. For automation
    /// driving a *windowed* event loop, where the caller runs between build
    /// and `end_frame`: boxes created this frame have not been placed yet, so
    /// a plain [`snapshot`](Self::snapshot) would report zero rects for them.
    /// The extra layout solve is deterministic and repeated by `end_frame`.
    #[cfg(feature = "testkit")]
    pub fn snapshot_laid_out(&mut self) -> crate::testkit::UiSnapshot {
        self.layout_root(self.root);
        self.snapshot()
    }

    #[cfg(feature = "testkit")]
    pub fn snapshot(&self) -> crate::testkit::UiSnapshot {
        let nodes = self
            .frame_boxes
            .iter()
            .filter_map(|idx| self.boxes.get(*idx).map(|node| (*idx, node)))
            .map(|(idx, node)| crate::testkit::UiNodeSnapshot {
                key: node.key,
                parent_key: node.parent.map(|parent| self.boxes[parent].key),
                depth: self.node_depth(idx),
                child_count: node.children.len(),
                label: node.debug_label.clone(),
                key_id: node.key_id.clone(),
                text: node.string.clone(),
                bounds: node.rect,
                computed_size: node.computed_size,
                scroll: node.scroll,
                scroll_max: node.scroll_max,
                content_size: node.content_size,
                clip_rect: self.clipped_rect(idx),
                signal: node.signal,
                flags: node.flags,
                visible: node.visible,
                focused: self.focus_key == Some(node.key),
                layout_axis: node.child_layout_axis,
                padding: node.padding,
                child_gap: node.child_gap,
                main_axis_align: node.main_axis_align,
                cross_axis_align: node.cross_axis_align,
                style: node.style,
                hot_t: node.hot_t,
                active_t: node.active_t,
                focus_t: node.focus_t,
                appear_t: node.appear_t,
                opacity: self.box_opacity(idx),
                text_edit: self.text_edit_states.get(&node.key).cloned(),
            })
            .collect();
        crate::testkit::UiSnapshot { nodes }
    }

    #[cfg(feature = "testkit")]
    pub(super) fn node_depth(&self, idx: usize) -> usize {
        let mut depth = 0;
        let mut parent = self.boxes[idx].parent;
        while let Some(parent_idx) = parent {
            depth += 1;
            parent = self.boxes[parent_idx].parent;
        }
        depth
    }
}
