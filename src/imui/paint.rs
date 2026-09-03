use super::*;

impl IMUI {
    pub(super) fn animate_visual_state(&mut self) {
        let hot_rate = smooth_rate(self.theme.motion.hot_rate, self.animation_dt);
        let active_rate = smooth_rate(self.theme.motion.active_rate, self.animation_dt);
        let focus_rate = smooth_rate(self.theme.motion.focus_rate, self.animation_dt);
        let appear_rate = smooth_rate(self.theme.motion.menu_rate, self.animation_dt);
        let color_rate = smooth_rate(30.0, self.animation_dt);
        let epsilon = self.theme.motion.epsilon;
        let mut animating = false;
        // On the DOM backend the browser eases these — see
        // `css_drives_animation`. Every box then takes its target value in
        // one step, exactly like the `key.is_zero()` and first-frame arms
        // below already do, and `animating` stays false so the loop is not
        // asked for another frame.
        let snap = self.css_drives_animation();

        for frame_pos in 0..self.frame_boxes.len() {
            let idx = self.frame_boxes[frame_pos];
            let key = self.boxes[idx].key;
            let is_hot = self.boxes[idx].signal.hovering();
            let is_active = self.active_left_key == Some(key)
                || self.active_right_key == Some(key)
                || self.boxes[idx].signal.pressed();
            let is_focused = self.focus_key == Some(key);
            let is_floating = self.boxes[idx].flags.contains(UIBoxFlags::FLOATING_X)
                || self.boxes[idx].flags.contains(UIBoxFlags::FLOATING_Y);
            let draws_background = self.boxes[idx].flags.contains(UIBoxFlags::DRAW_BACKGROUND);
            let draws_border = self.boxes[idx].flags.contains(UIBoxFlags::DRAW_BORDER);
            let draws_text = self.boxes[idx].flags.contains(UIBoxFlags::DRAW_TEXT);
            let animates_interaction =
                self.boxes[idx].flags.contains(UIBoxFlags::DRAW_HOT_EFFECTS) || draws_border;
            let animates_appearance =
                is_floating && (draws_background || draws_border || draws_text);
            let scrollbar_x_target = (self.scrollbar_available(idx, Axis::X)
                && self.scrollbar_is_hot_or_active(idx, Axis::X))
                as u8 as f32;
            let scrollbar_y_target = (self.scrollbar_available(idx, Axis::Y)
                && self.scrollbar_is_hot_or_active(idx, Axis::Y))
                as u8 as f32;
            let box_ = &mut self.boxes[idx];

            if snap || key.is_zero() {
                box_.hot_t = is_hot as u8 as f32;
                box_.active_t = is_active as u8 as f32;
                box_.focus_t = is_focused as u8 as f32;
                box_.appear_t = 1.0;
                box_.scrollbar_x_t = scrollbar_x_target;
                box_.scrollbar_y_t = scrollbar_y_target;
                box_.bg_color_animated = box_.style.bg_color;
                box_.border_color_animated = box_.style.border_color;
                continue;
            }

            if box_.first_touched_frame == self.build_index {
                box_.hot_t = is_hot as u8 as f32;
                box_.active_t = is_active as u8 as f32;
                box_.focus_t = is_focused as u8 as f32;
                box_.appear_t = if animates_appearance {
                    appear_rate
                } else {
                    1.0
                };
                box_.scrollbar_x_t = scrollbar_x_target;
                box_.scrollbar_y_t = scrollbar_y_target;
                box_.bg_color_animated = box_.style.bg_color;
                box_.border_color_animated = box_.style.border_color;
                if animates_appearance && box_.appear_t < 1.0 - epsilon {
                    animating = true;
                }
                continue;
            }

            box_.hot_t = animate_scalar(box_.hot_t, is_hot as u8 as f32, hot_rate, epsilon);
            box_.active_t =
                animate_scalar(box_.active_t, is_active as u8 as f32, active_rate, epsilon);
            box_.focus_t =
                animate_scalar(box_.focus_t, is_focused as u8 as f32, focus_rate, epsilon);
            box_.appear_t = if animates_appearance {
                animate_scalar(box_.appear_t, 1.0, appear_rate, epsilon)
            } else {
                1.0
            };
            box_.scrollbar_x_t =
                animate_scalar(box_.scrollbar_x_t, scrollbar_x_target, hot_rate, epsilon);
            box_.scrollbar_y_t =
                animate_scalar(box_.scrollbar_y_t, scrollbar_y_target, hot_rate, epsilon);

            let mut target_bg = box_.style.bg_color;
            if box_.flags.contains(UIBoxFlags::DRAW_HOT_EFFECTS) {
                target_bg = color_mix(target_bg, self.theme.surface_hover, box_.hot_t * 0.55);
                target_bg = color_mix(target_bg, self.theme.accent_active, box_.active_t * 0.35);
            }
            let target_border = color_mix(box_.style.border_color, self.theme.accent, box_.focus_t);
            if draws_background {
                box_.bg_color_animated = color_lerp(box_.bg_color_animated, target_bg, color_rate);
            } else {
                box_.bg_color_animated = box_.style.bg_color;
            }
            if draws_border {
                box_.border_color_animated =
                    color_lerp(box_.border_color_animated, target_border, color_rate);
            } else {
                box_.border_color_animated = box_.style.border_color;
            }

            animating = animating
                || (animates_interaction
                    && ((box_.hot_t - is_hot as u8 as f32).abs() > epsilon
                        || (box_.active_t - is_active as u8 as f32).abs() > epsilon
                        || (box_.focus_t - is_focused as u8 as f32).abs() > epsilon))
                || (1.0 - box_.appear_t).abs() > epsilon
                || (box_.scrollbar_x_t - scrollbar_x_target).abs() > epsilon
                || (box_.scrollbar_y_t - scrollbar_y_target).abs() > epsilon
                || (draws_background
                    && color_distance(box_.bg_color_animated, target_bg) > epsilon)
                || (draws_border
                    && color_distance(box_.border_color_animated, target_border) > epsilon);
        }

        if animating {
            self.request_repaint();
        }
    }

    pub(super) fn draw_ui_all(&mut self) {
        if self.drawer.is_none() {
            return;
        }
        let root_clip = self.boxes[self.root].rect;
        self.draw_ui_root_skipping_clipped(self.root, Some(self.overlay_root), root_clip);
        self.draw_ui_root_clipped(self.overlay_root, root_clip);
    }

    pub(super) fn draw_ui_root_clipped(&mut self, idx: usize, clip: RectCoords) {
        self.draw_ui_root_skipping_clipped(idx, None, clip);
    }

    pub(super) fn draw_ui_root_skipping_clipped(
        &mut self,
        idx: usize,
        skip_idx: Option<usize>,
        clip: RectCoords,
    ) {
        if skip_idx == Some(idx) {
            return;
        }
        if !self.boxes[idx].visible {
            return;
        }
        let rect = self.boxes[idx].rect;
        let draw_rect = intersect_rects(rect, clip);
        if draw_rect.width() <= 0.0 || draw_rect.height() <= 0.0 {
            return;
        }
        let flags = self.boxes[idx].flags;
        let style = self.boxes[idx].style;
        let opacity = self.box_opacity(idx);
        let draw_bg = flags.contains(UIBoxFlags::DRAW_BACKGROUND);
        let draw_border = flags.contains(UIBoxFlags::DRAW_BORDER);
        let rounded_with_border = draw_bg && draw_border && style.corner_radius > 0.0;

        if rounded_with_border {
            // Rounded border: draw outer border shape then inset background shape.
            self.drawer.as_mut().unwrap().draw_rect(
                &draw_rect,
                color_mul_alpha(self.boxes[idx].border_color_animated, opacity),
                style.corner_radius,
            );

            let inset = style.border_size.max(0.0);
            let inner_w = (draw_rect.width() - inset * 2.0).max(0.0);
            let inner_h = (draw_rect.height() - inset * 2.0).max(0.0);
            if inner_w > 0.0 && inner_h > 0.0 {
                let inner = RectCoords::from_size(
                    draw_rect.x0 + inset,
                    draw_rect.y0 + inset,
                    inner_w,
                    inner_h,
                );
                let inner_radius = (style.corner_radius - inset).max(0.0);
                self.drawer.as_mut().unwrap().draw_rect(
                    &inner,
                    color_mul_alpha(self.boxes[idx].bg_color_animated, opacity),
                    inner_radius,
                );
            }
        } else if draw_bg {
            self.drawer.as_mut().unwrap().draw_rect(
                &draw_rect,
                color_mul_alpha(self.boxes[idx].bg_color_animated, opacity),
                style.corner_radius,
            );
        }
        if draw_border && !rounded_with_border {
            self.drawer.as_mut().unwrap().draw_empty_rect(
                &draw_rect,
                color_mul_alpha(self.boxes[idx].border_color_animated, opacity),
                style.border_size,
            );
        }
        if flags.contains(UIBoxFlags::CUSTOM_DRAW)
            && let Some(ci) = self.boxes[idx].canvas_paint
        {
            // Hand the painter `&mut Drawer` by moving it out of `self` for the
            // duration of the call: that frees `self` so the closure (held in
            // `self.canvas_paints`) can be borrowed mutably alongside it. The
            // closure only gets the drawer — it can't re-enter the UI.
            if let Some(mut drawer) = self.drawer.take() {
                if let Some(cb) = self.canvas_paints.get_mut(ci) {
                    cb(&mut drawer, rect, draw_rect);
                }
                self.drawer = Some(drawer);
            }
        }
        if flags.contains(UIBoxFlags::DRAW_IMAGE)
            && let Some(key) = self.boxes[idx].display_string.clone()
        {
            match self.image_texture_for_paint(&key) {
                Some((tex_id, iw, ih)) if tex_id != 0 && iw > 0 && ih > 0 => {
                    // Contain-fit the image within the box, centered: an inline
                    // image's box is already aspect-correct (fills exactly),
                    // while a standalone viewer box (Fill) letterboxes.
                    let bw = rect.width();
                    let bh = rect.height();
                    let scale = (bw / iw as f32).min(bh / ih as f32).max(0.0);
                    let fw = iw as f32 * scale;
                    let fh = ih as f32 * scale;
                    let fit = RectCoords::from_size(
                        rect.x0 + (bw - fw) * 0.5,
                        rect.y0 + (bh - fh) * 0.5,
                        fw,
                        fh,
                    );
                    self.drawer
                        .as_mut()
                        .unwrap()
                        .draw_image(&fit, &draw_rect, tex_id);
                    // Resize grip: only for interactive (textarea) images. The
                    // nested image box never wins `hot_key` (the parent textarea
                    // consumes events), so hover is derived directly from the
                    // cursor over the fitted rect, plus any active resize drag.
                    let hovering = point_in_rect(&fit, self.mouse);
                    let resizing = self
                        .image_resize_drag_key()
                        .is_some_and(|k| Some(k) == self.boxes[idx].display_string.as_deref());
                    if flags.contains(UIBoxFlags::MOUSE_CLICKABLE) && (hovering || resizing) {
                        // The grip's grab region (matches the textarea hit-test).
                        const HIT: f32 = 22.0;
                        let over_grip = point_in_rect(
                            &RectCoords::from_size(fit.x1 - HIT, fit.y1 - HIT, HIT, HIT),
                            self.mouse,
                        );
                        // Diagonal resize cursor when over the grip (or dragging).
                        if over_grip || resizing {
                            self.cursor = crate::os::OSCursor::ResizeNWSE;
                        }
                        let grip = 12.0;
                        let grip_rect = intersect_rects(
                            RectCoords::from_size(fit.x1 - grip, fit.y1 - grip, grip, grip),
                            draw_rect,
                        );
                        if grip_rect.width() > 0.0 && grip_rect.height() > 0.0 {
                            let color = color_mul_alpha(self.theme.accent, opacity * 0.85);
                            self.drawer
                                .as_mut()
                                .unwrap()
                                .draw_rect(&grip_rect, color, 2.0);
                        }
                    }
                    if self.image_unsynced.contains(&key) {
                        self.draw_image_unsynced_badge(fit, draw_rect, opacity);
                    }
                }
                _ => {
                    // Not decoded yet: ask the host to provide it and draw a
                    // muted placeholder so the line still occupies its space.
                    self.request_image(&key);
                    let placeholder = color_mul_alpha(self.theme.surface_bg, opacity);
                    self.drawer.as_mut().unwrap().draw_rect(
                        &draw_rect,
                        placeholder,
                        self.theme.radius,
                    );
                }
            }
        }
        if flags.contains(UIBoxFlags::DRAW_TEXT)
            && self.boxes[idx].display_string.is_some()
            && flags.contains(UIBoxFlags::TEXT_WRAP)
        {
            self.draw_wrapped_text(idx, rect, clip, opacity);
        } else if flags.contains(UIBoxFlags::DRAW_TEXT) && self.boxes[idx].display_string.is_some()
        {
            let padding = self.boxes[idx].padding;
            // Buttons center their label within the content box; everything else keeps
            // the default top-left placement. Measure first (needs `&mut self`) so the
            // borrow ends before we re-borrow `display_string` below.
            let (center_x, center_y) = if style.text_align_center {
                let (tw, th) = self.text_size_for_box(idx, style.font_size);
                let avail_w = (rect.width() - padding.horizontal() - style.margin * 2.0).max(0.0);
                let avail_h = (rect.height() - padding.vertical() - style.margin * 2.0).max(0.0);
                (
                    ((avail_w - tw) * 0.5).max(0.0),
                    ((avail_h - th) * 0.5).max(0.0),
                )
            } else if flags.contains(UIBoxFlags::LINE_EDIT) {
                // Single-line inputs center the text vertically within their
                // content box so it lines up with the (centered) caret instead
                // of sitting at the top — and stays centered if the font is
                // taller than the box (clipping symmetrically rather than only
                // at the bottom). Left edge is untouched (no horizontal center).
                let (_tw, th) = self.text_size_for_box(idx, style.font_size);
                let avail_h = (rect.height() - padding.vertical() - style.margin * 2.0).max(0.0);
                (0.0, (avail_h - th) * 0.5)
            } else {
                (0.0, 0.0)
            };
            let text = self.boxes[idx].display_string.as_deref().unwrap();
            // Horizontal scroll (line edits) shifts the text left and clips it to the
            // content's left edge so it doesn't spill into the padding. It is zero for
            // every non-scrolling box, leaving their rendering untouched.
            let content_left = rect.x0 + padding.left + style.margin + center_x;
            let scroll_x = self.boxes[idx].scroll.x;
            let left_clip = if scroll_x > 0.0 {
                clip.x0.max(content_left)
            } else {
                clip.x0
            };
            // Paint highlight rects behind matched byte ranges (search results),
            // measuring prefix/range widths the same way the glyphs are laid out
            // so the highlight lines up exactly — and so the text itself is one
            // continuous, un-split run (no per-segment clipping of glyph ink).
            if !self.boxes[idx].text_highlights.is_empty() {
                let hl_color = color_mul_alpha(self.boxes[idx].highlight_color, opacity);
                let top = rect.y0 + padding.top + style.margin + center_y;
                let clip_bottom = (rect.y1 - padding.bottom - style.margin).min(clip.y1);
                let xmax = (rect.x1 - padding.right - style.margin).min(clip.x1);
                let rects: Vec<RectCoords> = {
                    let drawer = self.drawer.as_ref().unwrap();
                    let th = drawer.get_text_size(style.font_size, "M", 1).1;
                    let bottom = (top + th).min(clip_bottom);
                    self.boxes[idx]
                        .text_highlights
                        .iter()
                        .filter_map(|&(s, e)| {
                            if s >= e
                                || e > text.len()
                                || !text.is_char_boundary(s)
                                || !text.is_char_boundary(e)
                            {
                                return None;
                            }
                            let x0 = content_left - scroll_x
                                + drawer.get_text_size(style.font_size, &text[..s], s).0;
                            let x1 =
                                x0 + drawer.get_text_size(style.font_size, &text[s..e], e - s).0;
                            let x0 = x0.max(left_clip);
                            let x1 = x1.min(xmax);
                            (x1 > x0).then(|| {
                                RectCoords::from_size(x0, top, x1 - x0, (bottom - top).max(0.0))
                            })
                        })
                        .collect()
                };
                for r in &rects {
                    self.drawer.as_mut().unwrap().draw_rect(r, hl_color, 3.0);
                }
            }
            self.drawer.as_mut().unwrap().draw_text(
                content_left - scroll_x,
                rect.y0 + padding.top + style.margin + center_y,
                style.font_size,
                text,
                text.len(),
                left_clip,
                clip.y0,
                (rect.x1 - padding.right - style.margin).min(clip.x1),
                (rect.y1 - padding.bottom - style.margin).min(clip.y1),
                color_mul_alpha(style.text_color, opacity),
                false,
                style.font_icon,
            );
        }
        let child_clip = if flags.contains(UIBoxFlags::CLIP) {
            intersect_rects(clip, rect)
        } else {
            clip
        };
        self.draw_textarea_block_decorations(idx, child_clip);
        let child_len = self.boxes[idx].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[idx].children[child_pos];
            self.draw_ui_root_skipping_clipped(child, skip_idx, child_clip);
        }
        self.draw_scrollbars(idx, clip);
        self.draw_text_selection_if_focused(idx);
        self.draw_text_caret_if_focused(idx);
        self.draw_remote_carets(idx);
    }

    pub(super) fn draw_textarea_block_decorations(&mut self, idx: usize, clip: RectCoords) {
        if self.drawer.is_none() || !self.boxes[idx].flags.contains(UIBoxFlags::MULTILINE) {
            return;
        }
        let key = self.boxes[idx].key;
        let block_count = self
            .editor_layouts
            .get(&key)
            .map(|layout| layout.blocks.len())
            .unwrap_or(0);
        if block_count == 0 {
            return;
        }
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let margin = self.boxes[idx].style.margin;
        let opacity = self.box_opacity(idx);
        let content_x0 = rect.x0 + padding.left + margin;
        let content_x1 = rect.x1 - padding.right - margin;

        for block_idx in 0..block_count {
            let block = self.editor_layouts[&key].blocks[block_idx];
            // From the layout, not the rows: a code fence taller than the
            // viewport has its first and/or last line outside the emitted
            // window while its background still has to span the whole screen.
            let (Some(first), Some(last)) = (
                self.textarea_line_rect(idx, block.first_visual_line),
                self.textarea_line_rect(idx, block.last_visual_line),
            ) else {
                continue;
            };
            let y0 = first.y0 - block.padding.top;
            let y1 = last.y1 + block.padding.bottom;
            let block_rect = RectCoords {
                x0: content_x0,
                y0,
                x1: content_x1,
                y1,
            };
            let block_rect = intersect_rects(block_rect, clip);
            if block_rect.width() <= 0.0 || block_rect.height() <= 0.0 {
                continue;
            }
            self.drawer.as_mut().unwrap().draw_rect(
                &block_rect,
                color_mul_alpha(block.bg_color, opacity),
                block.corner_radius,
            );
            if let Some(label) = block.label {
                let label_font = 11.0;
                let label_x = block_rect.x1 - block.padding.right - block.label_width;
                let label_y = block_rect.y0 + 3.0;
                if label_x > block_rect.x0 + block.padding.left {
                    self.drawer.as_mut().unwrap().draw_text(
                        label_x,
                        label_y,
                        label_font,
                        label,
                        label.len(),
                        block_rect.x0,
                        block_rect.y0,
                        block_rect.x1,
                        block_rect.y1,
                        color_mul_alpha(self.theme.text_muted, opacity),
                        false,
                        false,
                    );
                }
            }
        }
    }

    pub(super) fn box_opacity(&self, idx: usize) -> f32 {
        let mut opacity = 1.0;
        let mut current = Some(idx);
        while let Some(idx) = current {
            // Both the framework's appear animation and the app-set style
            // opacity inherit down the tree, so a faded parent fades its whole
            // subtree without every child having to opt in.
            opacity *= self.boxes[idx].appear_t.clamp(0.0, 1.0);
            opacity *= self.boxes[idx].style.opacity.clamp(0.0, 1.0);
            current = self.boxes[idx].parent;
        }
        opacity
    }

    pub(super) fn draw_scrollbars(&mut self, idx: usize, clip: RectCoords) {
        if self.drawer.is_none() {
            return;
        }
        let color = color_mul_alpha(self.theme.scrollbar, self.box_opacity(idx));
        if self.scrollbar_available(idx, Axis::Y) {
            let thickness = self.scrollbar_thickness(idx, Axis::Y);
            let Some(bar) = self.scrollbar_thumb_rect(idx, Axis::Y, thickness) else {
                return;
            };
            let bar = intersect_rects(bar, clip);
            if bar.width() > 0.0 && bar.height() > 0.0 {
                self.drawer
                    .as_mut()
                    .unwrap()
                    .draw_rect(&bar, color, thickness * 0.5);
            }
        }
        if self.scrollbar_available(idx, Axis::X) {
            let thickness = self.scrollbar_thickness(idx, Axis::X);
            let Some(bar) = self.scrollbar_thumb_rect(idx, Axis::X, thickness) else {
                return;
            };
            let bar = intersect_rects(bar, clip);
            if bar.width() > 0.0 && bar.height() > 0.0 {
                self.drawer
                    .as_mut()
                    .unwrap()
                    .draw_rect(&bar, color, thickness * 0.5);
            }
        }
    }

    pub(super) fn draw_text_caret_if_focused(&mut self, idx: usize) {
        if self.drawer.is_none() || self.focus_key != Some(self.boxes[idx].key) {
            return;
        }
        if !self.boxes[idx].flags.accepts_text_input() {
            return;
        }
        let now = self.now_seconds();
        let state = self.text_edit_states.get(&self.boxes[idx].key);
        let last_interaction = state.map(|s| s.last_interaction_time).unwrap_or(now);
        let elapsed = now - last_interaction;
        // Show caret for 0.5s after interaction, then blink at 2 Hz.
        if elapsed > 0.5 && ((elapsed - 0.5) * 2.0) as i64 % 2 != 0 {
            return;
        }

        if self.boxes[idx].flags.contains(UIBoxFlags::LINE_EDIT) {
            self.draw_line_edit_caret(idx);
        } else if self.boxes[idx].flags.contains(UIBoxFlags::MULTILINE) {
            self.draw_textarea_caret(idx);
        }
    }

    pub(super) fn draw_text_selection_if_focused(&mut self, idx: usize) {
        if self.drawer.is_none() || self.focus_key != Some(self.boxes[idx].key) {
            return;
        }
        if !self.boxes[idx].flags.accepts_text_input() {
            return;
        }
        let Some(range) = self
            .text_edit_states
            .get(&self.boxes[idx].key)
            .and_then(TextEditState::selection_range)
        else {
            return;
        };
        let mut color = self.theme.color_main;
        color.a = 0.35;
        if self.boxes[idx].flags.contains(UIBoxFlags::LINE_EDIT) {
            self.draw_line_edit_selection(idx, range, color);
        } else if self.boxes[idx].flags.contains(UIBoxFlags::MULTILINE) {
            self.draw_textarea_selection(idx, range, color);
        }
    }

    pub(super) fn draw_line_edit_selection(
        &mut self,
        idx: usize,
        range: (usize, usize),
        color: Color,
    ) {
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let style = self.boxes[idx].style;
        let Some(text) = self.boxes[idx].display_string.as_deref() else {
            return;
        };
        let text_len = char_count(text);
        let start_text = substring_chars(text, (0, range.0.min(text_len)));
        let selected_text = substring_chars(text, (range.0, range.1.min(text_len)));
        let start_w = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, &start_text, start_text.len())
            .0;
        let selected_w = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, &selected_text, selected_text.len())
            .0;
        let x = rect.x0 + padding.left + style.margin + start_w - self.boxes[idx].scroll.x;
        let y = rect.y0 + padding.top + style.margin;
        let h = (rect.y1 - padding.bottom - style.margin - y).max(1.0);
        let sel = RectCoords::from_size(x, y, selected_w, h);
        let sel = intersect_rects(sel, rect);
        if sel.width() > 0.0 && sel.height() > 0.0 {
            self.drawer.as_mut().unwrap().draw_rect(&sel, color, 1.0);
        }
    }

    pub(super) fn draw_textarea_selection(
        &mut self,
        idx: usize,
        range: (usize, usize),
        color: Color,
    ) {
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let style = self.boxes[idx].style;
        let text = self.boxes[idx].string.clone().unwrap_or_default();
        let line_h = self.theme.size_text + 6.0;
        let key = self.ensure_layout_for_box(idx, &text);
        let ranges = self.layout_ranges(key);
        let (start_line, _) = self.visual_line_col_from_cursor_with_ranges(&ranges, range.0);
        let (end_line, _) = self.visual_line_col_from_cursor_with_ranges(&ranges, range.1);
        for line in start_line..=end_line {
            let (line_start, line_end_idx) = ranges[line];
            let start = if line == start_line {
                range.0.max(line_start)
            } else {
                line_start
            };
            let end = if line == end_line {
                range.1.min(line_end_idx)
            } else {
                line_end_idx
            };
            if start >= end {
                continue;
            }
            let line_rect = self.textarea_line_rect(idx, line);
            let line_padding_left = line_rect.map(|line| line.padding.left).unwrap_or(0.0);
            // An image line has zero-width text geometry; highlight its full
            // displayed width so a selection over it is visible.
            let (start_w, selected_w) = match self.layout_line_image_width(key, line) {
                Some(image_w) => (0.0, image_w),
                None => {
                    let sw = self.layout_caret_x(key, line, start);
                    (sw, self.layout_caret_x(key, line, end) - sw)
                }
            };
            let x = rect.x0 + padding.left + style.margin + line_padding_left + start_w
                - self.boxes[idx].scroll.x;
            let (y, h) = line_rect
                .map(|line| (line.y0, line.y1 - line.y0))
                .unwrap_or_else(|| {
                    (
                        rect.y0 + padding.top + style.margin + line as f32 * line_h
                            - self.boxes[idx].scroll.y,
                        line_h,
                    )
                });
            let sel = RectCoords::from_size(x, y, selected_w, h);
            let sel = intersect_rects(sel, rect);
            if sel.width() > 0.0 && sel.height() > 0.0 {
                self.drawer.as_mut().unwrap().draw_rect(&sel, color, 1.0);
            }
        }
    }

    pub(super) fn draw_line_edit_caret(&mut self, idx: usize) {
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let style = self.boxes[idx].style;
        let Some(text) = self.boxes[idx].display_string.as_deref() else {
            return;
        };
        let text_len = char_count(text);
        // `display_string` includes any injected IME preedit; the caret renders after it.
        let preedit_len = self.focused_preedit_len(self.boxes[idx].key);
        let buffer_caret = self
            .text_edit_states
            .get(&self.boxes[idx].key)
            .map(|state| state.cursor)
            .unwrap_or(text_len)
            .min(text_len.saturating_sub(preedit_len));
        let cursor = (buffer_caret + preedit_len).min(text_len);
        let prefix = substring_chars(text, (0, cursor));
        let text_width = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, &prefix, prefix.len())
            .0;
        let text_height = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(style.font_size, "M", 1)
            .1;

        let content_x0 = rect.x0 + padding.left + style.margin;
        let content_y0 = rect.y0 + padding.top + style.margin;
        let content_x1 = rect.x1 - padding.right - style.margin;
        let content_y1 = rect.y1 - padding.bottom - style.margin;

        // Shift by the horizontal scroll offset, then keep the caret inside the box.
        let scroll_x = self.boxes[idx].scroll.x;
        let caret_x = line_edit_caret_x(content_x0, content_x1, text_width, scroll_x);
        // Center the caret vertically in the content box, matching how the
        // line-edit text is vertically centered (see the paint DRAW_TEXT path).
        let content_h = (content_y1 - content_y0).max(1.0);
        let caret_h = text_height.min(content_h);
        let caret_y = content_y0 + (content_h - caret_h) * 0.5;
        let caret_rect = RectCoords::from_size(caret_x, caret_y, 1.5, caret_h);

        // Underline the composing (preedit) text, from its start to the caret.
        if preedit_len > 0 {
            let start_prefix = substring_chars(text, (0, buffer_caret));
            let start_width = self
                .drawer
                .as_ref()
                .unwrap()
                .get_text_size(style.font_size, &start_prefix, start_prefix.len())
                .0;
            let start_x = line_edit_caret_x(content_x0, content_x1, start_width, scroll_x);
            let underline = RectCoords::from_size(
                start_x,
                caret_y + caret_h - 1.0,
                (caret_x - start_x).max(0.0),
                1.0,
            );
            self.drawer
                .as_mut()
                .unwrap()
                .draw_rect(&underline, self.theme.color_text, 0.0);
        }

        self.drawer
            .as_mut()
            .unwrap()
            .draw_rect(&caret_rect, self.theme.color_text, 0.0);
    }

    pub(super) fn draw_textarea_caret(&mut self, idx: usize) {
        let style = self.boxes[idx].style;
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let text = self.boxes[idx].string.clone().unwrap_or_default();
        // The display string includes any injected IME preedit, so the caret renders
        // *after* the composing text.
        let preedit_len = self.focused_preedit_len(self.boxes[idx].key);
        let cursor = (self
            .text_edit_states
            .get(&self.boxes[idx].key)
            .map(|state| state.cursor)
            .unwrap_or_else(|| char_count(&text))
            + preedit_len)
            .min(char_count(&text));
        let key = self.ensure_layout_for_box(idx, &text);
        let ranges = self.layout_ranges(key);
        let (visual_line, _col) = self.visual_line_col_from_cursor_with_ranges(&ranges, cursor);
        let content_x0 = rect.x0 + padding.left + style.margin;
        let line_h = self.theme.size_text + 6.0;
        let content_x1 = rect.x1 - padding.right - style.margin;
        let line_rect = self.textarea_line_rect(idx, visual_line);
        let (content_y0, content_y1, font_size) = line_rect
            .map(|line| {
                (
                    line.y0 + line.padding.top,
                    line.y1 - line.padding.bottom,
                    line.font_size,
                )
            })
            .unwrap_or_else(|| {
                let content_y0 = rect.y0 + padding.top + style.margin + visual_line as f32 * line_h
                    - self.boxes[idx].scroll.y;
                (
                    content_y0,
                    (content_y0 + line_h).min(rect.y1 - padding.bottom - style.margin),
                    style.font_size,
                )
            });

        let text_width = self.layout_caret_x(key, visual_line.min(ranges.len() - 1), cursor);
        let line_padding_left = line_rect.map(|line| line.padding.left).unwrap_or(0.0);
        let text_height = self
            .drawer
            .as_ref()
            .unwrap()
            .get_text_size(font_size, "M", 1)
            .1;

        // Caret-follow scrolling is handled in update_focused_textarea_scroll (which also
        // runs headless); here we only draw the caret at its current scrolled position.
        let caret_right = (content_x1 - 1.0).max(content_x0);
        let caret_x = (content_x0 + line_padding_left + text_width - self.boxes[idx].scroll.x)
            .clamp(content_x0, caret_right);
        let caret_h = text_height.min((content_y1 - content_y0).max(1.0));
        let caret_rect = intersect_rects(
            RectCoords::from_size(caret_x, content_y0, 1.5, caret_h),
            rect,
        );
        if caret_rect.width() <= 0.0 || caret_rect.height() <= 0.0 {
            return;
        }

        // Underline the composing (preedit) text, from its start to the caret.
        let preedit_len = self.focused_preedit_len(self.boxes[idx].key);
        if preedit_len > 0 {
            let start_cursor = cursor.saturating_sub(preedit_len);
            let start_width =
                self.layout_caret_x(key, visual_line.min(ranges.len() - 1), start_cursor);
            let start_x = (content_x0 + line_padding_left + start_width - self.boxes[idx].scroll.x)
                .clamp(content_x0, caret_right);
            let underline = intersect_rects(
                RectCoords::from_size(
                    start_x,
                    content_y0 + caret_h - 1.0,
                    (caret_x - start_x).max(0.0),
                    1.0,
                ),
                rect,
            );
            if underline.width() > 0.0 && underline.height() > 0.0 {
                self.drawer
                    .as_mut()
                    .unwrap()
                    .draw_rect(&underline, self.theme.color_text, 0.0);
            }
        }

        self.drawer
            .as_mut()
            .unwrap()
            .draw_rect(&caret_rect, self.theme.color_text, 0.0);
    }

    /// Draw collaborator carets registered via [`IMUI::set_remote_carets`]: a
    /// colored bar at each char index plus a small initial-letter badge above
    /// it. Same geometry math as the local textarea caret.
    pub(super) fn draw_remote_carets(&mut self, idx: usize) {
        if self.drawer.is_none() || !self.boxes[idx].flags.contains(UIBoxFlags::MULTILINE) {
            return;
        }
        let Some(carets) = self.remote_carets.get(&self.boxes[idx].key) else {
            return;
        };
        if carets.is_empty() {
            return;
        }
        let carets = carets.clone();
        let style = self.boxes[idx].style;
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let text = self.boxes[idx].string.clone().unwrap_or_default();
        let text_len = char_count(&text);
        let key = self.ensure_layout_for_box(idx, &text);
        let ranges = self.layout_ranges(key);
        let line_h = self.theme.size_text + 6.0;
        let content_x0 = rect.x0 + padding.left + style.margin;
        let content_x1 = rect.x1 - padding.right - style.margin;

        for caret in carets {
            // Selection highlight first, so the caret bar draws on top.
            if let Some((sel_start, sel_end)) = caret.selection
                && sel_start < sel_end
            {
                let mut highlight = caret.color;
                highlight.a *= 0.30;
                self.draw_textarea_selection(
                    idx,
                    (sel_start.min(text_len), sel_end.min(text_len)),
                    highlight,
                );
            }
            let cursor = caret.cursor.min(text_len);
            let (visual_line, _col) = self.visual_line_col_from_cursor_with_ranges(&ranges, cursor);
            let line_rect = self.textarea_line_rect(idx, visual_line);
            let (content_y0, content_y1, font_size) = line_rect
                .map(|line| {
                    (
                        line.y0 + line.padding.top,
                        line.y1 - line.padding.bottom,
                        line.font_size,
                    )
                })
                .unwrap_or_else(|| {
                    let content_y0 =
                        rect.y0 + padding.top + style.margin + visual_line as f32 * line_h
                            - self.boxes[idx].scroll.y;
                    (
                        content_y0,
                        (content_y0 + line_h).min(rect.y1 - padding.bottom - style.margin),
                        style.font_size,
                    )
                });

            let text_width = self.layout_caret_x(key, visual_line.min(ranges.len() - 1), cursor);
            let line_padding_left = line_rect.map(|line| line.padding.left).unwrap_or(0.0);
            let caret_right = (content_x1 - 1.0).max(content_x0);
            let caret_x = (content_x0 + line_padding_left + text_width - self.boxes[idx].scroll.x)
                .clamp(content_x0, caret_right);
            let caret_h = (content_y1 - content_y0).max(1.0);
            let caret_rect = intersect_rects(
                RectCoords::from_size(caret_x, content_y0, 2.0, caret_h),
                rect,
            );
            if caret_rect.width() <= 0.0 || caret_rect.height() <= 0.0 {
                continue;
            }
            self.drawer
                .as_mut()
                .unwrap()
                .draw_rect(&caret_rect, caret.color, 0.0);

            // Label sitting on top of the caret: a round initial badge by
            // default, expanding to the full username (no circle) while the
            // pointer hovers over the caret.
            let initial: String = caret.label.chars().take(1).collect();
            if initial.is_empty() {
                continue;
            }
            let badge = font_size * 0.85;
            let badge_top = content_y0 - badge;
            // Hover hit area: the caret bar plus the badge above it.
            let hover_rect = RectCoords {
                x0: caret_x - badge * 0.5,
                y0: badge_top,
                x1: caret_x + badge * 0.5,
                y1: content_y1,
            };
            let hovered = point_in_rect(&hover_rect, self.mouse);

            let mut text_color = Color::new("#ffffff");
            text_color.a = caret.color.a;

            if hovered {
                // Name tag: full username on a rounded pill, no circle.
                let label = caret.label.clone();
                let label_font = font_size * 0.72;
                let (label_w, label_h) = self.text_size(label_font, &label);
                let pad_x = badge * 0.4;
                let tag_w = label_w + pad_x * 2.0;
                let tag_x = (caret_x - tag_w * 0.5 + 1.0)
                    .clamp(content_x0, (content_x1 - tag_w).max(content_x0));
                let tag_box = RectCoords::from_size(tag_x, badge_top, tag_w, badge);
                let tag_rect = intersect_rects(tag_box, rect);
                if tag_rect.width() <= 0.0 || tag_rect.height() <= 0.0 {
                    continue;
                }
                let label_len = label.len();
                let drawer = self.drawer.as_mut().unwrap();
                drawer.draw_rect(&tag_rect, caret.color, badge * 0.25);
                drawer.draw_text(
                    tag_x + (tag_w - label_w) * 0.5,
                    badge_top + (badge - label_h) * 0.5,
                    label_font,
                    &label,
                    label_len,
                    tag_rect.x0,
                    tag_rect.y0,
                    tag_rect.x1,
                    tag_rect.y1,
                    text_color,
                    false,
                    false,
                );
            } else {
                // Round initial badge; centre the glyph using its measured size
                // rather than fixed offsets so it sits in the middle.
                let badge_box =
                    RectCoords::from_size(caret_x - badge * 0.5 + 1.0, badge_top, badge, badge);
                let badge_rect = intersect_rects(badge_box, rect);
                if badge_rect.width() <= 0.0 || badge_rect.height() <= 0.0 {
                    continue;
                }
                let initial_font = badge * 0.62;
                let (init_w, init_h) = self.text_size(initial_font, &initial);
                let initial_len = initial.len();
                let drawer = self.drawer.as_mut().unwrap();
                drawer.draw_rect(&badge_rect, caret.color, badge * 0.5);
                drawer.draw_text(
                    badge_box.x0 + (badge - init_w) * 0.5,
                    badge_top + (badge - init_h) * 0.5,
                    initial_font,
                    &initial,
                    initial_len,
                    badge_rect.x0,
                    badge_rect.y0,
                    badge_rect.x1,
                    badge_rect.y1,
                    text_color,
                    false,
                    false,
                );
            }
        }
    }

    pub(super) fn text_size(&mut self, font_size: f32, text: &str) -> (f32, f32) {
        if let Some(drawer) = self.drawer.as_ref() {
            drawer.get_text_size(font_size, text, text.len())
        } else {
            (text.chars().count() as f32 * font_size * 0.6, font_size)
        }
    }

    /// Per-char advances for `text` (whole-run shaping), used to build caret
    /// geometry that lines up with the rendered glyphs.
    pub(super) fn char_advances(&mut self, font_size: f32, text: &str, out: &mut Vec<f32>) {
        if let Some(drawer) = self.drawer.as_ref() {
            drawer.char_advances(font_size, text, out);
        } else {
            out.clear();
            out.extend(text.chars().map(|_| font_size * 0.6));
        }
    }

    pub(super) fn text_size_for_box(&mut self, idx: usize, font_size: f32) -> (f32, f32) {
        let font_icon = self.boxes[idx].style.font_icon;
        if let Some(measured) = self.boxes[idx].measured_text
            && measured.font_size == font_size
            && measured.font_icon == font_icon
        {
            return (measured.size.width, measured.size.height);
        }

        let Some(text) = self.boxes[idx].display_string.as_deref() else {
            return (0.0, font_size);
        };
        let size = if let Some(drawer) = self.drawer.as_ref() {
            drawer.get_text_size_for_font(font_size, text, text.len(), font_icon)
        } else {
            (text.chars().count() as f32 * font_size * 0.6, font_size)
        };
        self.boxes[idx].measured_text = Some(MeasuredText {
            font_size,
            font_icon,
            size: Size {
                width: size.0,
                height: size.1,
            },
        });
        size
    }

    pub(super) fn invalidate_text_measure(&mut self, idx: usize) {
        self.boxes[idx].measured_text = None;
    }

    /// Draw a small "not synced" warning badge in the top-right of an inline
    /// image. `fit` is the fitted image rect; `clip` bounds it to the box.
    fn draw_image_unsynced_badge(&mut self, fit: RectCoords, clip: RectCoords, opacity: f32) {
        const SIZE: f32 = 22.0;
        const INSET: f32 = 6.0;
        let x0 = (fit.x1 - SIZE - INSET).max(fit.x0);
        let y0 = fit.y0 + INSET;
        let badge = intersect_rects(RectCoords::from_size(x0, y0, SIZE, SIZE), clip);
        if badge.width() <= 0.0 || badge.height() <= 0.0 {
            return;
        }
        // Dark disc for contrast on any image, then the warning glyph on top.
        let backing = color_mul_alpha(Color::new("#0d1117"), opacity * 0.8);
        self.drawer
            .as_mut()
            .unwrap()
            .draw_rect(&badge, backing, SIZE * 0.5);
        let glyph = "\u{e002}"; // Material "warning"
        let font_size = 15.0;
        let (gw, gh) = match self.drawer.as_ref() {
            Some(drawer) => drawer.get_text_size_for_font(font_size, glyph, glyph.len(), true),
            None => (font_size, font_size),
        };
        let gx = x0 + (SIZE - gw) * 0.5;
        let gy = y0 + (SIZE - gh) * 0.5;
        let color = color_mul_alpha(self.theme.warning, opacity);
        self.drawer.as_mut().unwrap().draw_text(
            gx,
            gy,
            font_size,
            glyph,
            glyph.len(),
            badge.x0,
            badge.y0,
            badge.x1,
            badge.y1,
            color,
            false,
            true,
        );
    }

    fn measure_text_width(&self, font_size: f32, font_icon: bool, s: &str) -> f32 {
        match self.drawer.as_ref() {
            Some(drawer) => {
                drawer
                    .get_text_size_for_font(font_size, s, s.len(), font_icon)
                    .0
            }
            None => s.chars().count() as f32 * font_size * 0.6,
        }
    }

    pub(super) fn line_height_for(&self, font_size: f32) -> f32 {
        match self.drawer.as_ref() {
            Some(drawer) => drawer.line_height(font_size),
            None => font_size * 1.2,
        }
    }

    /// Greedy word-wrap `text` to `max_width`, returning one byte range per line.
    /// Explicit `\n` always break; a single word wider than the line is left to
    /// overflow (and clip) rather than mid-word split. Spaces (and `\n`) are
    /// single-byte in UTF-8, so scanning bytes for them stays on char boundaries.
    pub(super) fn wrap_text_ranges(
        &self,
        text: &str,
        font_size: f32,
        font_icon: bool,
        max_width: f32,
    ) -> Vec<std::ops::Range<usize>> {
        let mut lines: Vec<std::ops::Range<usize>> = Vec::new();
        let bytes = text.as_bytes();
        let len = text.len();
        let mut ls = 0usize; // current line start
        let mut le = 0usize; // end of committed content on the current line
        let mut line_empty = true;
        let mut cursor = 0usize;
        while cursor < len {
            let mut ws = cursor;
            while ws < len && bytes[ws] == b' ' {
                ws += 1;
            }
            if ws >= len {
                break;
            }
            if bytes[ws] == b'\n' {
                lines.push(ls..le);
                cursor = ws + 1;
                ls = cursor;
                le = cursor;
                line_empty = true;
                continue;
            }
            let mut we = ws;
            while we < len && bytes[we] != b' ' && bytes[we] != b'\n' {
                we += 1;
            }
            if line_empty {
                ls = ws;
                le = we;
                line_empty = false;
            } else if max_width <= 0.0
                || self.measure_text_width(font_size, font_icon, &text[ls..we]) <= max_width
            {
                le = we;
            } else {
                lines.push(ls..le);
                ls = ws;
                le = we;
            }
            cursor = we;
        }
        if !line_empty || lines.is_empty() {
            lines.push(ls..le);
        }
        lines
    }

    /// Ensure `idx`'s wrap cache matches the current font/width, recomputing only
    /// on a real change. Returns the line count (min 1).
    pub(super) fn ensure_wrapped(
        &mut self,
        idx: usize,
        font_size: f32,
        font_icon: bool,
        max_width: f32,
    ) -> usize {
        let fresh = self.boxes[idx].wrapped.as_ref().is_some_and(|w| {
            w.font_size == font_size
                && w.font_icon == font_icon
                && (w.max_width - max_width).abs() < 0.5
        });
        if !fresh {
            let text = self.boxes[idx].display_string.clone().unwrap_or_default();
            let lines = self.wrap_text_ranges(&text, font_size, font_icon, max_width);
            self.boxes[idx].wrapped = Some(WrappedText {
                font_size,
                font_icon,
                max_width,
                lines,
            });
        }
        self.boxes[idx]
            .wrapped
            .as_ref()
            .map(|w| w.lines.len().max(1))
            .unwrap_or(1)
    }

    /// Paint a `TEXT_WRAP` box's text across its wrapped lines, top-aligned.
    pub(super) fn draw_wrapped_text(
        &mut self,
        idx: usize,
        rect: RectCoords,
        clip: RectCoords,
        opacity: f32,
    ) {
        let padding = self.boxes[idx].padding;
        let font_size = self.boxes[idx].style.font_size;
        let margin = self.boxes[idx].style.margin;
        let font_icon = self.boxes[idx].style.font_icon;
        let color = color_mul_alpha(self.boxes[idx].style.text_color, opacity);
        let avail = (rect.width() - padding.horizontal() - margin * 2.0).max(0.0);
        self.ensure_wrapped(idx, font_size, font_icon, avail);
        let line_height = self.line_height_for(font_size);

        let x = rect.x0 + padding.left + margin;
        let top = rect.y0 + padding.top + margin;
        let left_clip = clip.x0;
        let clip_y0 = clip.y0;
        let xmax = (rect.x1 - padding.right - margin).min(clip.x1);
        let ymax = (rect.y1 - padding.bottom - margin).min(clip.y1);

        // Disjoint field borrows: text/ranges from `boxes`, sink from `drawer`.
        let text = self.boxes[idx].display_string.as_deref().unwrap_or("");
        let lines = self.boxes[idx]
            .wrapped
            .as_ref()
            .map(|w| w.lines.as_slice())
            .unwrap_or(&[]);
        let Some(drawer) = self.drawer.as_mut() else {
            return;
        };
        for (i, range) in lines.iter().enumerate() {
            let y = top + i as f32 * line_height;
            if y > ymax {
                break;
            }
            let slice = &text[range.clone()];
            drawer.draw_text(
                x,
                y,
                font_size,
                slice,
                slice.len(),
                left_clip,
                clip_y0,
                xmax,
                ymax,
                color,
                false,
                font_icon,
            );
        }
    }

    /// Set a box's text after `alloc_box` has already run — the anonymous and
    /// separately-labelled boxes (`label`, `icon`, the text editor's rows and
    /// spans). Writes into the buffers the box already holds rather than
    /// handing it two fresh `String`s.
    pub(super) fn set_display_string(&mut self, idx: usize, display: &str) {
        let box_ = &mut self.boxes[idx];
        let pool = &mut self.string_pool;
        let changed = pool.assign(&mut box_.display_string, Some(display));
        pool.assign(&mut box_.string, Some(display));
        if changed {
            self.invalidate_text_measure(idx);
        }
    }
}

/// X position of a line-edit caret, clamped to stay inside the content box.
///
/// A box sized to hug its text (e.g. an editable label) collapses its content
/// area to zero — or, with padding, negative — width when the text is empty, so
/// `content_x1` can fall below `content_x0`. Flooring the clamp's upper bound at
/// the lower bound keeps `min <= max`; without it `f32::clamp` panics.
pub(super) fn line_edit_caret_x(
    content_x0: f32,
    content_x1: f32,
    text_width: f32,
    scroll_x: f32,
) -> f32 {
    let caret_max = (content_x1 - 1.0).max(content_x0);
    (content_x0 + text_width - scroll_x).clamp(content_x0, caret_max)
}
