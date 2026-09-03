use super::*;

/// A `MOUSE_CLICKABLE` box's native browser-derived hover/press/click state
/// since it was last consumed, when the DOM backend's own per-element
/// listeners are the source of truth for that box instead of this file's
/// `point_in_rect` geometry check — see `imui/paint_dom.rs`'s
/// `attach_interactive_listeners` (the writer) and `IMUI::dom_pointer_state`
/// (the reader, called from `signal_from_key_and_flags` below). Defined
/// here (not in `paint_dom.rs`) so it exists as a type regardless of the
/// `dom` feature — only the *implementation* that populates it is
/// `#[cfg(feature = "dom")]`-gated.
#[derive(Default)]
pub(super) struct DomPointerState {
    pub hovering: bool,
    pub left_pressed: bool,
    pub left_released: bool,
    pub left_clicked: bool,
    pub right_clicked: bool,
}

impl IMUI {
    pub fn pull_consume_events(&mut self) {
        self.adopt_dom_scrolls();
        if let Some(win) = self.os_window_mut() {
            self.events = win.get_events();
        }
        // Refresh the IME preedit (composing text) for inline rendering this frame.
        self.ime_preedit = self.os_window().and_then(|win| win.ime_preedit());
        // Ctrl+C / `kill` don't come through the window server, so they're
        // folded in here as the same `Quit` event AppKit's own termination
        // request produces — one shutdown path for every way of closing.
        if crate::os::take_quit_signal() {
            self.events.push(OSEvent::quit());
        }

        for ev in self.events.clone() {
            self.apply_event_side_effects(&ev);
        }
    }

    pub(super) fn apply_event_side_effects(&mut self, ev: &OSEvent) {
        if let Some(pos) = ev.pos {
            self.mouse = Some(pos);
        }
        // Remember the press *before* any box gets the chance to consume it —
        // see `IMUI::frame_presses` and `press_outside`.
        if ev.ty == OSEventType::Press
            && mouse_button_from_key(ev.key).is_some()
            && let Some(pos) = ev.pos.or(self.mouse)
        {
            self.frame_presses.push(pos);
        }
        if ev.key == OSKey::LeftMouseButton {
            match ev.ty {
                OSEventType::Press => self.left_mouse_down = true,
                OSEventType::Release => self.left_mouse_down = false,
                _ => {}
            }
        }
        if ev.key == OSKey::RightMouseButton {
            match ev.ty {
                OSEventType::Press => self.right_mouse_down = true,
                OSEventType::Release => self.right_mouse_down = false,
                _ => {}
            }
        }
        if ev.ty == OSEventType::Press && ev.key == OSKey::Keyboard(OSKeyCode::KeyEscape) {
            self.focus_key = None;
            self.next_focus_key = None;
        }
        if ev.ty == OSEventType::Quit {
            self.quit_requested = true;
        }
        // Resize/Repaint need no handling here: the event loop already forces a
        // repaint whenever any event is present (`had_events`), and it polls
        // win.get_size() each iteration to drive resize().
    }

    /// Inject a synthetic event as if it came from the OS, through the exact
    /// pipeline real events use. This is the automation entry point (showcase
    /// scripts, tests). Inject before building widgets so the frame under
    /// construction sees the event.
    pub fn inject_event(&mut self, ev: OSEvent) {
        self.apply_event_side_effects(&ev);
        self.events.push(ev);
    }

    #[cfg(feature = "testkit")]
    pub(crate) fn push_test_event(&mut self, ev: OSEvent) {
        self.inject_event(ev);
    }

    pub(super) fn remove_event(&mut self, ev_idx: usize) {
        self.events.remove(ev_idx);
        self.request_repaint();
    }

    pub(super) fn capture_pointer_blacklist_rects(&mut self) {
        self.pointer_blacklist_rects.clear();
        for frame_pos in 0..self.frame_boxes.len() {
            let idx = self.frame_boxes[frame_pos];
            if self.blocks_pointer(idx) && self.is_overlay_box(idx) {
                self.pointer_blacklist_rects.push(expanded_rect(
                    self.clipped_rect(idx),
                    self.boxes[idx].hit_padding,
                ));
            }
        }
    }

    /// Take up whatever the browser has scrolled since the last frame, so
    /// mae's own `scroll`/`scroll_target` describe where the content really
    /// is — see `paint_dom.rs`'s `attach_scroll_listener`. Called before the
    /// build, so this frame lays out against it.
    ///
    /// Both are set, not just `scroll`: `scroll_target` is what Rust *wants*,
    /// and leaving it behind would have the next frame yank the list back to
    /// where the user just scrolled away from.
    pub(super) fn adopt_dom_scrolls(&mut self) {
        #[cfg(feature = "dom")]
        {
            let Some(dom) = self.dom.as_ref() else {
                return;
            };
            let scrolls = dom.take_pending_scrolls();
            for (key, (x, y)) in scrolls {
                let Some(idx) = self.box_from_key(key) else {
                    continue;
                };
                self.boxes[idx].scroll = Point::new(x, y);
                self.boxes[idx].scroll_target = Point::new(x, y);
            }
        }
    }

    pub(super) fn capture_scrollbar_hit_areas(&mut self) {
        self.scrollbar_hit_areas.clear();
        // The browser draws and drags the scrollbar on this backend (see
        // `paint_dom.rs`), so there is no Rust-side thumb to reserve the
        // pointer for — and reserving it anyway would blank out a strip down
        // the right edge of every scrollable box for every other widget.
        if self.css_drives_animation() {
            return;
        }
        for frame_pos in 0..self.frame_boxes.len() {
            let idx = self.frame_boxes[frame_pos];
            let key = self.boxes[idx].key;
            if key.is_zero() {
                continue;
            }
            for axis in [Axis::Y, Axis::X] {
                if let Some(rect) = self.scrollbar_track_rect(idx, axis) {
                    self.scrollbar_hit_areas
                        .push(ScrollbarHitArea { key, rect });
                }
            }
        }
    }

    pub(super) fn pointer_event_allowed_for_box(&self, ev: OSEvent, box_is_overlay: bool) -> bool {
        if box_is_overlay
            || !matches!(
                ev.ty,
                OSEventType::Press | OSEventType::Release | OSEventType::Scroll
            )
            || (ev.ty != OSEventType::Scroll && mouse_button_from_key(ev.key).is_none())
        {
            return true;
        }

        let Some(pos) = ev.pos.or(self.mouse) else {
            return true;
        };
        self.pointer_pos_allowed_for_box(pos, box_is_overlay)
    }

    pub(super) fn pointer_event_allowed_for_key(
        &self,
        ev: OSEvent,
        key: UiKey,
        box_is_overlay: bool,
    ) -> bool {
        if !self.pointer_event_allowed_for_box(ev, box_is_overlay) {
            return false;
        }
        if box_is_overlay
            || !matches!(
                ev.ty,
                OSEventType::Press | OSEventType::Release | OSEventType::Scroll
            )
            || (ev.ty != OSEventType::Scroll && mouse_button_from_key(ev.key).is_none())
        {
            return true;
        }
        let Some(pos) = ev.pos.or(self.mouse) else {
            return true;
        };
        !self.scrollbar_hit_areas.iter().any(|area| {
            area.key != key
                && self.box_from_key(area.key).is_some()
                && point_in_rect(&area.rect, Some(pos))
        })
    }

    pub(super) fn pointer_pos_allowed_for_box(&self, pos: Point, box_is_overlay: bool) -> bool {
        if box_is_overlay {
            return true;
        }
        !self
            .pointer_blacklist_rects
            .iter()
            .any(|rect| point_in_rect(rect, Some(pos)))
    }

    pub(super) fn pointer_pos_allowed_for_key(
        &self,
        pos: Point,
        key: UiKey,
        box_is_overlay: bool,
    ) -> bool {
        if self
            .active_scrollbar
            .is_some_and(|drag| self.left_mouse_down && drag.key != key)
        {
            return false;
        }
        self.pointer_pos_allowed_for_box(pos, box_is_overlay)
            && (box_is_overlay
                || !self.scrollbar_hit_areas.iter().any(|area| {
                    area.key != key
                        && self.box_from_key(area.key).is_some()
                        && point_in_rect(&area.rect, Some(pos))
                }))
    }

    pub fn input(&mut self, key: OSKey, flags: Option<OSEventFlag>) -> bool {
        let mut handled = false;
        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            if ev.ty == OSEventType::Press && ev.key == key && flags_match(flags, ev.flags) {
                handled = true;
                self.remove_event(ev_idx);
            } else {
                ev_idx += 1;
            }
        }
        handled
    }

    /// Like [`input`](Self::input) but the modifier flags must match *exactly*
    /// rather than just overlapping — so e.g. `Ctrl+F` and `Ctrl+Shift+F` are
    /// distinguishable (plain [`input`](Self::input) would fire both).
    pub fn input_exact(&mut self, key: OSKey, flags: Option<OSEventFlag>) -> bool {
        let mut handled = false;
        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            if ev.ty == OSEventType::Press && ev.key == key && ev.flags == flags {
                handled = true;
                self.remove_event(ev_idx);
            } else {
                ev_idx += 1;
            }
        }
        handled
    }

    pub fn mouse_position(&self) -> Option<Point> {
        self.mouse
    }

    pub fn mouse_down(&self) -> bool {
        self.left_mouse_down
    }

    /// Was a mouse button *pressed* this frame outside every one of `panes`?
    ///
    /// The dismissal test for popovers, menus and palettes. Unlike
    /// [`Self::mouse_down`] — which is level-triggered, and reports `true` for
    /// every frame a button is merely held — this is edge-triggered, so a
    /// surface opened by a left-click is not torn down again by the very press
    /// that opened it while the button is still down.
    ///
    /// Reads [`IMUI::frame_presses`] rather than the event queue: a press that
    /// landed on a clickable box outside the pane has already been consumed by
    /// the time an overlay built later in the frame asks about it, and clicking
    /// something *behind* an open palette is the most ordinary way there is to
    /// dismiss it.
    ///
    /// Panes are tested against their painted rect from the previous frame, so
    /// a pane that has none — one built for the first time this frame — reports
    /// nothing: a surface that was not on screen when the button went down
    /// cannot have been clicked away from, and the press that opened it is
    /// almost always still in this frame's record. That covers the opening
    /// frame on its own; callers need no `armed` flag of their own.
    ///
    /// Takes `&mut self` to request a repaint when it fires: the caller closes
    /// its surface *during* this build, after the surface itself was already
    /// emitted, so the frame that no longer shows it still has to be asked for.
    pub fn press_outside(&mut self, panes: &[UIBoxHandle]) -> bool {
        let painted_last_frame = |pane: &UIBoxHandle| {
            let rect = self.boxes[pane.idx()].previous_clip_rect;
            rect.width() > 0.0 && rect.height() > 0.0
        };
        if !panes.iter().all(painted_last_frame) {
            return false;
        }
        let outside = self.frame_presses.iter().any(|pos| {
            !panes
                .iter()
                .any(|pane| point_in_rect(&self.boxes[pane.idx()].previous_clip_rect, Some(*pos)))
        });
        if outside {
            self.request_repaint();
        }
        outside
    }

    pub fn reset_text_input_state(&mut self) {
        self.focus_key = None;
        self.next_focus_key = None;
    }

    pub fn set_focus_active(&mut self, id: &str) {
        let seed = self.boxes.get(self.root).map(|b| b.key).unwrap_or_default();
        self.next_focus_key = Some(UiKey(u64_hash_from_string(seed.0, id)));
    }

    /// Focus the given box on the next frame using its resolved key. Unlike
    /// [`set_focus_active`](Self::set_focus_active), this works for boxes nested
    /// under floating panes (where the string-id hash seed differs), e.g. to
    /// autofocus a text input when a palette opens.
    pub fn focus_box(&mut self, handle: UIBoxHandle) {
        self.next_focus_key = Some(handle.key);
    }

    pub(super) fn signal_from_key_and_flags(
        &mut self,
        key: UiKey,
        flags: UIBoxFlags,
        existing_idx: Option<usize>,
        box_is_overlay: bool,
    ) -> UISignal {
        let mut signal = UISignal::default();
        let rect = existing_idx
            .map(|idx| {
                expanded_rect(
                    self.boxes[idx].previous_clip_rect,
                    self.boxes[idx].hit_padding,
                )
            })
            .unwrap_or_else(|| RectCoords::from_size(-10000.0, -10000.0, 0.0, 0.0));

        // DOM builds with an active reconciler let the browser's own
        // per-element hit-test decide a MOUSE_CLICKABLE box's hover/press/
        // click (see `imui/paint_dom.rs`'s `attach_interactive_listeners`)
        // instead of this function's geometry check below — `None` for
        // every non-clickable box, and for every box under testkit/native
        // (no DOM reconciler exists there), which fall back to geometry
        // exactly as before this existed.
        #[cfg(feature = "dom")]
        let dom_pointer = self.dom_pointer_state(key, flags);
        #[cfg(not(feature = "dom"))]
        let dom_pointer: Option<DomPointerState> = None;

        let mouse_over = dom_pointer
            .as_ref()
            .map(|d| d.hovering)
            .unwrap_or_else(|| point_in_rect(&rect, self.mouse));
        let hover_allowed = self
            .mouse
            .is_none_or(|pos| self.pointer_pos_allowed_for_key(pos, key, box_is_overlay));
        let focused = self.focus_key == Some(key);

        if let Some(idx) = existing_idx {
            self.apply_scrollbar_events(idx, key, flags);
        }

        if let Some(dom) = &dom_pointer {
            // Delivering any of these means the app is about to react to it
            // *during this build* — and whatever state it changes was
            // already rendered from its old value earlier in this same
            // frame, so the result only becomes visible on the next one.
            // On the DOM backend nothing else guarantees that next frame
            // happens: `run_dom` only keeps ticking while something asks it
            // to, and these signals arrive through `dom_pointer_edges`
            // rather than the `OSEvent` queue its `has_actionable_event`
            // check looks at. Without this, a click's effect could sit
            // invisible until some unrelated event happened to drive
            // another frame — a toggle button appearing to ignore every
            // other click, a floating pane staying on screen after its
            // close button had already closed it. Self-limiting: the next
            // frame consumes no edge, so asks for no further repaint.
            if dom.left_pressed || dom.left_released || dom.left_clicked || dom.right_clicked {
                self.repaint_requested = true;
            }
            // The browser already resolved *which* element this is for by
            // dispatching directly to it — no `in_bounds`/`event_allowed`
            // geometry check needed, unlike the queued-event path below.
            if dom.left_pressed {
                self.hot_key = Some(key);
                self.set_active_key(MouseButton::Left, Some(key));
                self.drag_start_mouse = self.mouse;
                signal.flags |= UISignal::LEFT_PRESSED;
                signal.left_press_pos = self.mouse;
            }
            if dom.left_released {
                self.set_active_key(MouseButton::Left, None);
                signal.flags |= UISignal::LEFT_RELEASED;
            }
            if dom.left_clicked {
                signal.flags |= UISignal::LEFT_CLICKED | UISignal::COMMIT;
            }
            if dom.right_clicked {
                signal.flags |= UISignal::RIGHT_CLICKED;
            }
        }

        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            let in_bounds = point_in_rect(&rect, ev.pos.or(self.mouse));
            let mut taken = false;
            let event_allowed = self.pointer_event_allowed_for_key(ev, key, box_is_overlay);

            if flags.is_mouse_clickable() && dom_pointer.is_none() {
                if let Some(button) = mouse_button_from_key(ev.key) {
                    match ev.ty {
                        OSEventType::Press if in_bounds && event_allowed => {
                            self.hot_key = Some(key);
                            self.set_active_key(button, Some(key));
                            self.drag_start_mouse = ev.pos.or(self.mouse);
                            if button == MouseButton::Left {
                                signal.flags |= UISignal::LEFT_PRESSED;
                                signal.left_press_pos = ev.pos.or(self.mouse);
                            }
                            taken = true;
                        }
                        OSEventType::Release
                            if self.active_key(button) == Some(key) && event_allowed =>
                        {
                            self.set_active_key(button, None);
                            if button == MouseButton::Left {
                                signal.flags |= UISignal::LEFT_RELEASED;
                                if in_bounds {
                                    signal.flags |= UISignal::LEFT_CLICKED | UISignal::COMMIT;
                                }
                            }
                            if button == MouseButton::Right && in_bounds {
                                signal.flags |= UISignal::RIGHT_CLICKED;
                            }
                            if !in_bounds && self.hot_key == Some(key) {
                                self.hot_key = None;
                            }
                            taken = true;
                        }
                        _ => {}
                    }
                }
            }

            if !taken
                && flags.is_keyboard_clickable()
                && focused
                && ev.ty == OSEventType::Press
                && matches!(
                    ev.key,
                    OSKey::Keyboard(OSKeyCode::KeyEnter) | OSKey::Keyboard(OSKeyCode::KeySpace)
                )
            {
                signal.flags |= UISignal::COMMIT | UISignal::LEFT_CLICKED;
                taken = true;
            }

            // Enter in a focused single-line field reports COMMIT — "the user
            // accepted this value" — so an inline edit (renaming a row in place)
            // can react on the widget's own signal instead of the app sniffing
            // raw key events and guessing which field had focus. Deliberately
            // not `taken`: the editor still sees the key, and multiline boxes
            // are excluded because there Enter inserts a newline.
            if flags.contains(UIBoxFlags::TEXT_INPUT)
                && !flags.contains(UIBoxFlags::MULTILINE)
                && focused
                && ev.ty == OSEventType::Press
                && ev.key == OSKey::Keyboard(OSKeyCode::KeyEnter)
            {
                signal.flags |= UISignal::COMMIT;
                // Whatever the app does with the commit — rename the row, close
                // the field — happens during *this* build, after this widget was
                // already emitted from its pre-commit state, so the result shows
                // only on the next frame. Nothing else asks for that frame here:
                // the key is deliberately left in the queue for the editor (no
                // `remove_event`, hence no repaint from it), a single-line field
                // ignores Enter, and there is no key-up event on any platform to
                // wake the loop again. The rename would sit visibly uncommitted
                // until some unrelated event happened to drive another frame.
                // Self-limiting: the next frame sees no Enter press.
                self.request_repaint();
            }

            if !taken
                && ev.ty == OSEventType::Scroll
                && in_bounds
                && event_allowed
                && (flags.scrolls_x() || flags.scrolls_y())
            {
                // Holding Alt turns a vertical wheel into a horizontal scroll.
                let (ev_dx, ev_dy) = if has_flag(ev.flags, OSEventFlag::Alt) {
                    (ev.deltax + ev.deltay, 0.0)
                } else {
                    (ev.deltax, ev.deltay)
                };
                // Only consume on an axis this box actually scrolls *and*
                // isn't already clamped against in that direction — see
                // `scroll.rs`'s `absorb_pending_scroll_for_box`, which this
                // mirrors for boxes whose `SCROLL_X`/`SCROLL_Y` flag is set
                // from `alloc_box` time (e.g. `TEXTAREA`) rather than via a
                // later chained `.scroll_x()`/`.scroll_y()` call — without
                // this, a box already scrolled to its limit would swallow
                // every further wheel tick instead of letting it chain to
                // an ancestor scrollable.
                let (would_move_x, would_move_y) = existing_idx
                    .map(|idx| {
                        let target = self.boxes[idx].scroll_target;
                        let max = self.boxes[idx].scroll_max;
                        (
                            flags.scrolls_x()
                                && ev_dx != 0.0
                                && ((ev_dx > 0.0 && target.x > 0.0)
                                    || (ev_dx < 0.0 && target.x < max.x)),
                            flags.scrolls_y()
                                && ev_dy != 0.0
                                && ((ev_dy > 0.0 && target.y > 0.0)
                                    || (ev_dy < 0.0 && target.y < max.y)),
                        )
                    })
                    .unwrap_or((ev_dx != 0.0, ev_dy != 0.0));
                if would_move_x || would_move_y {
                    signal.scroll_x += ev_dx;
                    signal.scroll_y += ev_dy;
                    taken = true;
                }
            }

            if taken {
                self.remove_event(ev_idx);
            } else {
                ev_idx += 1;
            }
        }

        if mouse_over {
            signal.flags |= UISignal::MOUSE_OVER;
        }
        if flags.is_mouse_clickable()
            && mouse_over
            && hover_allowed
            && (self.hot_key.is_none() || self.hot_key == Some(key))
            && (self.active_left_key.is_none() || self.active_left_key == Some(key))
            && (self.active_right_key.is_none() || self.active_right_key == Some(key))
        {
            self.hot_key = Some(key);
            signal.flags |= UISignal::HOVERING;
        }

        if self.active_left_key == Some(key)
            && self.left_mouse_down
            && self.left_drag_moved_past_threshold()
        {
            signal.flags |= UISignal::LEFT_DRAGGING;
        }
        signal
    }

    fn left_drag_moved_past_threshold(&self) -> bool {
        const DRAG_THRESHOLD_PX: f32 = 2.0;
        let Some(start) = self.drag_start_mouse else {
            return false;
        };
        let Some(mouse) = self.mouse else {
            return false;
        };
        let dx = mouse.x - start.x;
        let dy = mouse.y - start.y;
        dx * dx + dy * dy >= DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX
    }

    pub(super) fn active_key(&self, button: MouseButton) -> Option<UiKey> {
        match button {
            MouseButton::Left => self.active_left_key,
            MouseButton::Right => self.active_right_key,
        }
    }

    pub(super) fn set_active_key(&mut self, button: MouseButton, key: Option<UiKey>) {
        match button {
            MouseButton::Left => {
                self.active_left_key = key;
                if key.is_none() {
                    self.drag_start_mouse = None;
                }
            }
            MouseButton::Right => self.active_right_key = key,
        }
    }

    pub(super) fn clipped_rect(&self, idx: usize) -> RectCoords {
        let mut rect = self.boxes[idx].rect;
        let mut parent = self.boxes[idx].parent;
        while let Some(parent_idx) = parent {
            let parent_box = &self.boxes[parent_idx];
            if parent_box.flags.contains(UIBoxFlags::CLIP) {
                rect = intersect_rects(rect, parent_box.rect);
            }
            parent = parent_box.parent;
        }
        rect
    }

    pub(super) fn update_previous_clip_rects(&mut self) {
        let frame_len = self.frame_boxes.len();
        for frame_pos in 0..frame_len {
            let idx = self.frame_boxes[frame_pos];
            self.boxes[idx].previous_clip_rect = self.clipped_rect(idx);
        }
    }

    /// Resolve the mouse cursor from the frame's hover winner, and request a
    /// repaint if the hovered box changed since last frame.
    ///
    /// Hover, `MOUSE_OVER` and `LEFT_DRAGGING` are computed inline per box in
    /// [`Self::signal_from_key_and_flags`] during build (one frame lagged, using
    /// `previous_clip_rect`), so this is an O(1) finalize rather than a pass over
    /// every box. `hot_key` already holds the inline hover winner; we only map it
    /// to a cursor here, where each box's current-frame `cursor`/flags are known.
    pub(super) fn resolve_cursor(&mut self) {
        self.cursor = OSCursor::Arrow;
        if let Some(idx) = self.hot_key.and_then(|key| self.box_from_key(key)) {
            self.cursor = if self.boxes[idx].flags.accepts_text_input() {
                OSCursor::IBeam
            } else {
                self.boxes[idx].cursor.unwrap_or(OSCursor::Hand)
            };
        }

        // While the left button is held, the box that captured the press dictates
        // the cursor (e.g. a drag handle), even if the pointer leaves its bounds.
        if self.left_mouse_down {
            if let Some(idx) = self.active_left_key.and_then(|key| self.box_from_key(key)) {
                if let Some(cursor) = self.boxes[idx].cursor {
                    self.cursor = cursor;
                }
            }
        }

        if self.hot_key != self.prev_hot_key {
            self.request_repaint();
        }
        self.prev_hot_key = self.hot_key;
    }

    pub(super) fn blocks_pointer(&self, idx: usize) -> bool {
        if idx == self.overlay_root || !self.boxes[idx].visible {
            return false;
        }
        let flags = self.boxes[idx].flags;
        flags.is_mouse_clickable()
            || flags.scrolls_x()
            || flags.scrolls_y()
            || flags.accepts_text_input()
            || (self.is_overlay_box(idx)
                && (flags.contains(UIBoxFlags::DRAW_BACKGROUND)
                    || flags.contains(UIBoxFlags::DRAW_BORDER)))
    }

    pub(super) fn is_overlay_box(&self, idx: usize) -> bool {
        idx == self.overlay_root || self.box_has_ancestor(idx, self.overlay_root)
    }

    pub(super) fn box_has_ancestor(&self, idx: usize, ancestor: usize) -> bool {
        let mut parent = self.boxes[idx].parent;
        while let Some(parent_idx) = parent {
            if parent_idx == ancestor {
                return true;
            }
            parent = self.boxes[parent_idx].parent;
        }
        false
    }
}
