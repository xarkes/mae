use super::*;

impl IMUI {
    pub(super) fn absorb_pending_scroll_for_box(&mut self, idx: usize) {
        let flags = self.boxes[idx].flags;
        if !flags.scrolls_x() && !flags.scrolls_y() {
            return;
        }
        let rect = self.boxes[idx].rect;
        let mut signal = UISignal::default();
        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            if ev.ty != OSEventType::Scroll
                || !point_in_rect(&rect, ev.pos.or(self.mouse))
                || !self.pointer_event_allowed_for_box(ev, self.is_overlay_box(idx))
            {
                ev_idx += 1;
                continue;
            }

            // Holding Alt turns a vertical wheel into a horizontal scroll. Platforms
            // report the wheel on `deltay` regardless, so route it here.
            let (ev_dx, ev_dy) = if has_flag(ev.flags, OSEventFlag::Alt) {
                (ev.deltax + ev.deltay, 0.0)
            } else {
                (ev.deltax, ev.deltay)
            };

            // Only consume the event on an axis this box actually scrolls
            // *and* isn't already clamped against in that direction —
            // otherwise a nested scrollable that's hit its limit (e.g. the
            // vertical list scrolled all the way down) would swallow every
            // further wheel tick with no visible effect instead of letting
            // it chain to whatever scrollable ancestor is next (the merged
            // page body), matching how scroll-chaining works on any normal
            // webpage once a nested `overflow` region bottoms out.
            let would_move_x = flags.scrolls_x()
                && ev_dx != 0.0
                && ((ev_dx > 0.0 && self.boxes[idx].scroll_target.x > 0.0)
                    || (ev_dx < 0.0
                        && self.boxes[idx].scroll_target.x < self.boxes[idx].scroll_max.x));
            let would_move_y = flags.scrolls_y()
                && ev_dy != 0.0
                && ((ev_dy > 0.0 && self.boxes[idx].scroll_target.y > 0.0)
                    || (ev_dy < 0.0
                        && self.boxes[idx].scroll_target.y < self.boxes[idx].scroll_max.y));
            let taken = would_move_x || would_move_y;

            if taken {
                signal.scroll_x += ev_dx;
                signal.scroll_y += ev_dy;
                self.remove_event(ev_idx);
            } else {
                ev_idx += 1;
            }
        }

        if signal.scroll_x != 0.0 || signal.scroll_y != 0.0 {
            self.boxes[idx].signal.scroll_x += signal.scroll_x;
            self.boxes[idx].signal.scroll_y += signal.scroll_y;
            self.apply_scroll_signal(idx);
        }
    }

    pub(super) fn scrollbar_track_rect(&self, idx: usize, axis: Axis) -> Option<RectCoords> {
        if !self.scrollbar_available(idx, axis) {
            return None;
        }
        let rect = self.boxes[idx].rect;
        let thickness = SCROLLBAR_HOVER_THICKNESS;
        let inset = SCROLLBAR_EDGE_INSET;
        Some(match axis {
            Axis::X => RectCoords::from_size(
                rect.x0,
                rect.y1 - thickness - inset * 2.0,
                rect.width(),
                thickness + inset * 2.0,
            ),
            Axis::Y => RectCoords::from_size(
                rect.x1 - thickness - inset * 2.0,
                rect.y0,
                thickness + inset * 2.0,
                rect.height(),
            ),
        })
    }

    pub(super) fn scrollbar_thumb_rect(
        &self,
        idx: usize,
        axis: Axis,
        thickness: f32,
    ) -> Option<RectCoords> {
        if !self.scrollbar_available(idx, axis) {
            return None;
        }
        let rect = self.boxes[idx].rect;
        let content = self.boxes[idx].content_size;
        let scroll = self.boxes[idx].scroll;
        let scroll_max = self.boxes[idx].scroll_max;
        let inset = SCROLLBAR_EDGE_INSET;

        Some(match axis {
            Axis::X => {
                let track_w = rect.width().max(1.0);
                let thumb_w = scrollbar_thumb_len(track_w, content.width)
                    .max(12.0)
                    .min(track_w);
                let thumb_x =
                    rect.x0 + (track_w - thumb_w) * (scroll.x / scroll_max.x).clamp(0.0, 1.0);
                RectCoords::from_size(thumb_x, rect.y1 - thickness - inset, thumb_w, thickness)
            }
            Axis::Y => {
                let track_h = rect.height().max(1.0);
                let thumb_h = scrollbar_thumb_len(track_h, content.height)
                    .max(12.0)
                    .min(track_h);
                let thumb_y =
                    rect.y0 + (track_h - thumb_h) * (scroll.y / scroll_max.y).clamp(0.0, 1.0);
                RectCoords::from_size(rect.x1 - thickness - inset, thumb_y, thickness, thumb_h)
            }
        })
    }

    pub(super) fn scrollbar_available(&self, idx: usize, axis: Axis) -> bool {
        let flags = self.boxes[idx].flags;
        let scroll_max = self.boxes[idx].scroll_max;
        let content = self.boxes[idx].content_size;
        match axis {
            Axis::X => flags.scrolls_x() && scroll_max.x > 0.0 && content.width > 0.0,
            Axis::Y => flags.scrolls_y() && scroll_max.y > 0.0 && content.height > 0.0,
        }
    }

    pub(super) fn scrollbar_is_hot_or_active(&self, idx: usize, axis: Axis) -> bool {
        let key = self.boxes[idx].key;
        if self
            .active_scrollbar
            .is_some_and(|drag| drag.key == key && drag.axis == axis)
        {
            return true;
        }
        self.scrollbar_track_rect(idx, axis)
            .is_some_and(|rect| point_in_rect(&rect, self.mouse))
    }

    pub(super) fn scrollbar_thickness(&self, idx: usize, axis: Axis) -> f32 {
        let t = match axis {
            Axis::X => self.boxes[idx].scrollbar_x_t,
            Axis::Y => self.boxes[idx].scrollbar_y_t,
        }
        .clamp(0.0, 1.0);
        SCROLLBAR_THICKNESS + (SCROLLBAR_HOVER_THICKNESS - SCROLLBAR_THICKNESS) * t
    }

    pub(super) fn apply_scrollbar_events(&mut self, idx: usize, key: UiKey, flags: UIBoxFlags) {
        if key.is_zero() || (!flags.scrolls_x() && !flags.scrolls_y()) {
            return;
        }
        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            let mut taken = false;
            let event_allowed =
                self.pointer_event_allowed_for_key(ev, key, self.is_overlay_box(idx));

            match ev.ty {
                OSEventType::Press if ev.key == OSKey::LeftMouseButton && event_allowed => {
                    if let Some((axis, pos)) = self.scrollbar_hit(idx, ev.pos.or(self.mouse)) {
                        self.begin_scrollbar_drag(idx, key, axis, pos);
                        taken = true;
                    }
                }
                OSEventType::MouseMove
                    if self.active_scrollbar.is_some_and(|drag| drag.key == key)
                        && self.left_mouse_down =>
                {
                    if let Some(pos) = ev.pos.or(self.mouse) {
                        self.drag_scrollbar_to(idx, pos);
                        taken = true;
                    }
                }
                OSEventType::Release
                    if ev.key == OSKey::LeftMouseButton
                        && self.active_scrollbar.is_some_and(|drag| drag.key == key) =>
                {
                    if let Some(pos) = ev.pos.or(self.mouse) {
                        self.drag_scrollbar_to(idx, pos);
                    }
                    self.active_scrollbar = None;
                    taken = true;
                }
                _ => {}
            }

            if taken {
                self.remove_event(ev_idx);
            } else {
                ev_idx += 1;
            }
        }
    }

    pub(super) fn scrollbar_hit(&self, idx: usize, pos: Option<Point>) -> Option<(Axis, Point)> {
        let pos = pos?;
        for axis in [Axis::Y, Axis::X] {
            if self
                .scrollbar_track_rect(idx, axis)
                .is_some_and(|rect| point_in_rect(&rect, Some(pos)))
            {
                return Some((axis, pos));
            }
        }
        None
    }

    pub(super) fn begin_scrollbar_drag(&mut self, idx: usize, key: UiKey, axis: Axis, pos: Point) {
        let thumb = self.scrollbar_thumb_rect(idx, axis, SCROLLBAR_HOVER_THICKNESS);
        let thumb_grab_offset = thumb
            .filter(|thumb| point_in_rect(thumb, Some(pos)))
            .map(|thumb| pos.axis(axis) - rect_min_axis(thumb, axis))
            .unwrap_or_else(|| self.scrollbar_thumb_len(idx, axis) * 0.5);

        self.active_scrollbar = Some(ScrollbarDrag {
            key,
            axis,
            thumb_grab_offset,
        });
        self.drag_scrollbar_to(idx, pos);
    }

    pub(super) fn drag_scrollbar_to(&mut self, idx: usize, pos: Point) {
        let Some(drag) = self.active_scrollbar else {
            return;
        };
        let rect = self.boxes[idx].rect;
        let scroll_max = self.boxes[idx].scroll_max.axis(drag.axis);
        if scroll_max <= 0.0 {
            return;
        }
        let track_min = rect_min_axis(rect, drag.axis);
        let track_len = rect_size_axis(rect, drag.axis).max(1.0);
        let thumb_len = self.scrollbar_thumb_len(idx, drag.axis);
        let movable = (track_len - thumb_len).max(1.0);
        let thumb_min =
            (pos.axis(drag.axis) - drag.thumb_grab_offset - track_min).clamp(0.0, movable);
        let value = scroll_max * (thumb_min / movable);
        self.boxes[idx].scroll.set_axis(drag.axis, value);
        self.boxes[idx].scroll_target.set_axis(drag.axis, value);
        self.request_repaint();
    }

    pub(super) fn scrollbar_thumb_len(&self, idx: usize, axis: Axis) -> f32 {
        let rect = self.boxes[idx].rect;
        let content = self.boxes[idx].content_size;
        let track_len = rect_size_axis(rect, axis).max(1.0);
        scrollbar_thumb_len(track_len, content.axis(axis))
            .max(12.0)
            .min(track_len)
    }

    pub(super) fn animate_scroll_offsets(&mut self) {
        let rate = smooth_rate(self.theme.motion.scroll_rate, self.animation_dt);
        let epsilon = 0.5;
        let mut animating = false;
        let snap = false;
        for frame_pos in 0..self.frame_boxes.len() {
            let idx = self.frame_boxes[frame_pos];
            let box_ = &mut self.boxes[idx];
            if snap || box_.first_touched_frame == self.build_index {
                box_.scroll = box_.scroll_target;
                continue;
            }
            for axis in [Axis::X, Axis::Y] {
                let current = box_.scroll.axis(axis);
                let target = box_.scroll_target.axis(axis);
                let next = current + (target - current) * rate;
                if (target - next).abs() <= epsilon {
                    box_.scroll.set_axis(axis, target);
                } else {
                    box_.scroll.set_axis(axis, next);
                    animating = true;
                }
            }
        }
        if animating {
            self.request_repaint();
        }
    }

    /// Scroll `handle` so `y` (a content-space offset, in pixels from the top of
    /// its content) becomes the top of the visible area.
    ///
    /// Sets the animation *target*, so the view eases there at
    /// [`UIMotion::scroll_rate`] like a wheel scroll rather than jumping. The
    /// offset is clamped against the box's scrollable range on the next layout.
    /// Needed to keep a keyboard cursor visible in a scrolling list — arrowing
    /// past the last visible row in a palette has to bring it into view.
    pub fn scroll_to_y(&mut self, handle: UIBoxHandle, y: f32) {
        let max = self.boxes[handle.idx()].scroll_max.y;
        self.boxes[handle.idx()].scroll_target.y = y.clamp(0.0, max.max(0.0));
        self.request_repaint();
    }
}
