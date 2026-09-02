use super::*;

impl IMUI {
    pub fn row(&mut self, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(None, Axis::X, UIBoxFlags::NONE, children)
    }

    pub fn column(&mut self, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(None, Axis::Y, UIBoxFlags::NONE, children)
    }

    pub fn named_row(&mut self, id: &str, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(Some(id), Axis::X, UIBoxFlags::NONE, children)
    }

    pub fn named_column(&mut self, id: &str, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(Some(id), Axis::Y, UIBoxFlags::NONE, children)
    }

    /// A named row *or* column, with the axis chosen at runtime.
    ///
    /// The one thing a layout that changes shape with the window needs from
    /// the framework. `named_row`/`named_column` bake the axis into the call,
    /// so a container that is a row on a wide window and a column on a narrow
    /// one had to be written as two separate calls — two different boxes,
    /// which on the DOM backend means two different `DomKey`s and a subtree
    /// that remounts every time the breakpoint is crossed, dropping the
    /// scroll offset, focus and animation state it was holding. One box with
    /// one id keeps that identity across the flip.
    pub fn named_container(
        &mut self,
        id: &str,
        axis: Axis,
        children: impl FnOnce(&mut IMUI),
    ) -> UIBoxHandle {
        self.container(Some(id), axis, UIBoxFlags::NONE, children)
    }

    pub(super) fn container(
        &mut self,
        label: Option<&str>,
        axis: Axis,
        flags: UIBoxFlags,
        children: impl FnOnce(&mut IMUI),
    ) -> UIBoxHandle {
        let handle = self.alloc_box(label, flags);
        self.boxes[handle.idx].child_layout_axis = axis;
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::ChildrenSum];
        if axis == Axis::X {
            self.boxes[handle.idx].pref_size = [UISize::ChildrenSum, UISize::ParentPct(1.0)];
        }
        self.parent_stack.push(handle.idx);
        children(self);
        self.parent_stack.pop();
        handle
    }

    /// A row that reports click/hover signals, for building rich clickable list items
    /// out of arbitrary child widgets. Children should be non-interactive (labels/icons).
    pub fn clickable_row(&mut self, id: &str, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(Some(id), Axis::X, UIBoxFlags::MOUSE_CLICKABLE, children)
    }

    /// Column variant of [`clickable_row`](Self::clickable_row).
    pub fn clickable_column(&mut self, id: &str, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        self.container(Some(id), Axis::Y, UIBoxFlags::MOUSE_CLICKABLE, children)
    }

    pub fn label(&mut self, label: &str) -> UIBoxHandle {
        let handle = self.alloc_box(None, UIBoxFlags::DRAW_TEXT);
        self.set_display_string(handle.idx, label.to_string());
        self.boxes[handle.idx].pref_size = [UISize::TextContent(0.0), UISize::TextContent(0.0)];
        handle
    }

    /// A label that word-wraps to its content width across multiple lines, its
    /// height following the wrapped line count. Give it a bounded width
    /// (`Fill` / `ParentPct` / `Pixels`) so there is a width to wrap against;
    /// the default fills the parent. Explicit `\n` in the text also break lines.
    pub fn wrapping_label(&mut self, label: &str) -> UIBoxHandle {
        let handle = self.alloc_box(None, UIBoxFlags::DRAW_TEXT | UIBoxFlags::TEXT_WRAP);
        self.set_display_string(handle.idx, label.to_string());
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::TextContent(0.0)];
        handle
    }

    /// A standalone image box that draws the registered image for `key`
    /// (`./blob/<name>`), contain-fit and centered within the box. Defaults to
    /// filling its parent; chain `.width`/`.height` to size it. Non-interactive
    /// (no resize grip) — for the inline editor images see the textarea.
    pub fn image(&mut self, id: &str, key: &str) -> UIBoxHandle {
        let handle = self.alloc_box(Some(id), UIBoxFlags::DRAW_IMAGE);
        self.set_display_string(handle.idx, key.to_string());
        self.boxes[handle.idx].pref_size = [UISize::Fill, UISize::Fill];
        handle
    }

    /// A non-interactive glyph rendered from the icon font. Pair with `font_size`/
    /// `text_color` builders to size and tint it.
    pub fn icon_label(&mut self, glyph: &str) -> UIBoxHandle {
        let handle = self.alloc_box(None, UIBoxFlags::DRAW_TEXT);
        self.set_display_string(handle.idx, glyph.to_string());
        self.boxes[handle.idx].style.font_icon = true;
        self.boxes[handle.idx].pref_size = [UISize::TextContent(0.0), UISize::TextContent(0.0)];
        handle
    }

    /// A box that paints arbitrary geometry via a deferred callback — the
    /// escape hatch for custom rendering (waveforms, meters, charts) that the
    /// stock widgets don't cover. Defaults to filling its parent; chain
    /// `.width`/`.height` to size it, and wrap in a `CLIP` parent to bound it.
    ///
    /// `paint` runs in the paint pass with `(drawer, content_rect, clip_rect)`
    /// and must be `'static` (capture owned/`Copy`/`Arc` data). See
    /// [`CanvasPaint`].
    pub fn canvas<F>(&mut self, id: &str, paint: F) -> UIBoxHandle
    where
        F: FnMut(&mut Drawer, RectCoords, RectCoords) + 'static,
    {
        let handle = self.alloc_box(Some(id), UIBoxFlags::CUSTOM_DRAW);
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::ParentPct(1.0)];
        let index = self.canvas_paints.len();
        self.canvas_paints.push(Box::new(paint));
        self.boxes[handle.idx].canvas_paint = Some(index);
        handle
    }

    pub fn button(&mut self, label: &str, tooltip_text: Option<&str>) -> UIBoxHandle {
        let handle = self.alloc_box(Some(label), UIBoxFlags::BUTTON);
        self.configure_button_box(handle);
        self.show_tooltip_for_hover(handle, tooltip_text);
        handle
    }

    pub fn button_icon(&mut self, label: &str, tooltip_text: Option<&str>) -> UIBoxHandle {
        let handle = self.button(label, tooltip_text);
        self.boxes[handle.idx].style.font_icon = true;
        self.boxes[handle.idx].style.font_size = 24.0;
        self.width(handle, UISize::Pixels(32.0));
        self.height(handle, UISize::Pixels(32.0));
        handle
    }

    pub fn button_icon_plain(&mut self, label: &str, tooltip_text: Option<&str>) -> UIBoxHandle {
        let handle = self.alloc_box(Some(label), UIBoxFlags::CLICKABLE | UIBoxFlags::DRAW_TEXT);
        self.boxes[handle.idx].style.font_icon = true;
        self.boxes[handle.idx].style.font_size = 24.0;
        self.boxes[handle.idx].style.text_color = if handle.dragging() || handle.pressed() {
            self.theme.accent_active
        } else if handle.hover() {
            self.theme.accent_hover
        } else {
            self.theme.text_muted
        };
        self.boxes[handle.idx].padding = Padding::all(2.0);
        self.width(handle, UISize::Pixels(32.0));
        self.height(handle, UISize::Pixels(32.0));
        self.show_tooltip_for_hover(handle, tooltip_text);
        handle
    }

    /// A full-window custom-draw layer parented to the overlay root, painted
    /// above all regular content. Unlike [`IMUI::floating_pane_at`] it draws no
    /// background/border and is not clickable, so it never blocks pointer input
    /// to the UI beneath it — suited for purely cosmetic overlays (a scripted
    /// demo cursor, debug annotations).
    pub fn overlay_canvas<F>(&mut self, id: &str, paint: F) -> UIBoxHandle
    where
        F: FnMut(&mut Drawer, RectCoords, RectCoords) + 'static,
    {
        self.parent_stack.push(self.overlay_root);
        let handle = self.canvas(id, paint);
        self.boxes[handle.idx].flags |= UIBoxFlags::FLOATING_X | UIBoxFlags::FLOATING_Y;
        self.boxes[handle.idx].fixed_position = Point::new(0.0, 0.0);
        self.parent_stack.pop();
        handle
    }

    /// A floating pane positioned relative to an anchor rect (usually the
    /// widget that opened it, captured with [`IMUI::bounds`]), flipped to the
    /// opposite side and clamped to the window when the preferred side would
    /// overflow.
    ///
    /// `size` is the pane's intended `(width, height)` — supplied rather than
    /// measured, because a pane built this frame has no layout yet and reading
    /// last frame's `computed_size` would leave one frame at the wrong position.
    /// Every popover in practice knows its width and can estimate its height
    /// from its row count, so this is the cheap, flicker-free option.
    ///
    /// The pane's own width/height are still yours to set; `size` only drives
    /// placement, so pass what you are about to style it with.
    pub fn anchored_pane(
        &mut self,
        anchor: RectCoords,
        side: PopoverSide,
        size: (f32, f32),
        id: Option<&str>,
        children: impl FnOnce(&mut IMUI),
    ) -> UIBoxHandle {
        let pos = self.anchored_position(anchor, side, size);
        self.floating_pane_at(pos, id, children)
    }

    /// Placement math for [`Self::anchored_pane`], split out so it is testable
    /// and reusable without building a pane.
    pub fn anchored_position(
        &self,
        anchor: RectCoords,
        side: PopoverSide,
        size: (f32, f32),
    ) -> Point {
        let (width, height) = size;
        let win = self.size;
        let margin = 4.0;
        let gap = 2.0;

        // Preferred placement, then flip to the opposite side if it would
        // overflow *and* the opposite side has more room. Flipping beats
        // clamping here: a clamped menu overlaps the control that opened it.
        let (mut x, mut y) = match side {
            PopoverSide::Below => (anchor.x0, anchor.y1 + gap),
            PopoverSide::Above => (anchor.x0, anchor.y0 - gap - height),
            PopoverSide::Right => (anchor.x1 + gap, anchor.y0),
            PopoverSide::Left => (anchor.x0 - gap - width, anchor.y0),
        };
        match side {
            PopoverSide::Below | PopoverSide::Above => {
                let below = anchor.y1 + gap;
                let above = anchor.y0 - gap - height;
                if y + height + margin > win.height && above >= margin {
                    y = above;
                } else if y < margin && below + height + margin <= win.height {
                    y = below;
                }
            }
            PopoverSide::Right | PopoverSide::Left => {
                let right = anchor.x1 + gap;
                let left = anchor.x0 - gap - width;
                if x + width + margin > win.width && left >= margin {
                    x = left;
                } else if x < margin && right + width + margin <= win.width {
                    x = right;
                }
            }
        }

        // Whatever room is left on the cross axis: slide, don't overflow.
        x = x.clamp(margin, (win.width - width - margin).max(margin));
        y = y.clamp(margin, (win.height - height - margin).max(margin));
        Point::new(x, y)
    }

    pub fn floating_pane_at(
        &mut self,
        pos: Point,
        id: Option<&str>,
        children: impl FnOnce(&mut IMUI),
    ) -> UIBoxHandle {
        self.parent_stack.push(self.overlay_root);
        let handle = self.alloc_box(id, UIBoxFlags::DRAW_BACKGROUND | UIBoxFlags::DRAW_BORDER);
        self.boxes[handle.idx].fixed_position = pos;
        self.boxes[handle.idx].flags |= UIBoxFlags::FLOATING_X | UIBoxFlags::FLOATING_Y;
        self.boxes[handle.idx].child_layout_axis = Axis::Y;
        self.boxes[handle.idx].pref_size = [UISize::ChildrenSum, UISize::ChildrenSum];
        self.parent_stack.push(handle.idx);
        children(self);
        self.parent_stack.pop();
        self.parent_stack.pop();
        handle
    }

    pub(super) fn width(&mut self, handle: UIBoxHandle, width: UISize) -> &mut Self {
        self.boxes[handle.idx].pref_size[axis_idx(Axis::X)] = width;
        self
    }

    pub(super) fn height(&mut self, handle: UIBoxHandle, height: UISize) -> &mut Self {
        self.boxes[handle.idx].pref_size[axis_idx(Axis::Y)] = height;
        self
    }

    pub(super) fn min_width(&mut self, handle: UIBoxHandle, width: f32) -> &mut Self {
        self.boxes[handle.idx].min_size.width = width;
        self
    }

    pub(super) fn min_height(&mut self, handle: UIBoxHandle, height: f32) -> &mut Self {
        self.boxes[handle.idx].min_size.height = height;
        self
    }

    pub(super) fn opacity(&mut self, handle: UIBoxHandle, opacity: f32) -> &mut Self {
        self.boxes[handle.idx].style.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub(super) fn background(&mut self, handle: UIBoxHandle, color: Color) -> &mut Self {
        self.boxes[handle.idx].flags |= UIBoxFlags::DRAW_BACKGROUND;
        self.boxes[handle.idx].style.bg_color = color;
        self
    }

    pub(super) fn text_color(&mut self, handle: UIBoxHandle, color: Color) -> &mut Self {
        self.boxes[handle.idx].style.text_color = color;
        self
    }

    pub(super) fn text_center(&mut self, handle: UIBoxHandle, center: bool) -> &mut Self {
        self.boxes[handle.idx].style.text_align_center = center;
        self
    }

    pub(super) fn font_size(&mut self, handle: UIBoxHandle, size: f32) -> &mut Self {
        self.boxes[handle.idx].style.font_size = size.max(1.0);
        self
    }

    pub(super) fn border_color(&mut self, handle: UIBoxHandle, color: Color) -> &mut Self {
        self.boxes[handle.idx].flags |= UIBoxFlags::DRAW_BORDER;
        self.boxes[handle.idx].style.border_color = color;
        self
    }

    pub(super) fn corner_radius(&mut self, handle: UIBoxHandle, radius: f32) -> &mut Self {
        self.boxes[handle.idx].style.corner_radius = radius.max(0.0);
        self
    }

    pub(super) fn margin(&mut self, handle: UIBoxHandle, margin: f32) -> &mut Self {
        self.boxes[handle.idx].style.margin = margin.max(0.0);
        self
    }

    pub(super) fn text_highlights(
        &mut self,
        handle: UIBoxHandle,
        ranges: Vec<(usize, usize)>,
        color: Color,
    ) -> &mut Self {
        self.boxes[handle.idx].text_highlights = ranges;
        self.boxes[handle.idx].highlight_color = color;
        self
    }

    pub(super) fn cursor(&mut self, handle: UIBoxHandle, cursor: OSCursor) -> &mut Self {
        self.boxes[handle.idx].cursor = Some(cursor);
        self
    }

    pub(super) fn hit_padding_x(&mut self, handle: UIBoxHandle, value: f32) -> &mut Self {
        let value = value.max(0.0);
        self.boxes[handle.idx].hit_padding.left = value;
        self.boxes[handle.idx].hit_padding.right = value;
        self
    }

    pub(super) fn padding_all(&mut self, handle: UIBoxHandle, value: f32) -> &mut Self {
        self.boxes[handle.idx].padding = Padding::all(value);
        self
    }

    pub(super) fn padding(
        &mut self,
        handle: UIBoxHandle,
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
    ) -> &mut Self {
        self.boxes[handle.idx].padding = Padding {
            top,
            right,
            bottom,
            left,
        };
        self
    }

    pub(super) fn gap(&mut self, handle: UIBoxHandle, value: f32) -> &mut Self {
        self.boxes[handle.idx].child_gap = value;
        self
    }

    pub(super) fn scroll_x(&mut self, handle: UIBoxHandle, enabled: bool) -> &mut Self {
        if enabled {
            self.boxes[handle.idx].flags |= UIBoxFlags::SCROLL_X;
            self.absorb_pending_scroll_for_box(handle.idx);
            let key = self.boxes[handle.idx].key;
            let flags = self.boxes[handle.idx].flags;
            self.apply_scrollbar_events(handle.idx, key, flags);
        } else {
            self.boxes[handle.idx].flags.0 &= !UIBoxFlags::SCROLL_X.0;
        }
        self
    }

    pub(super) fn scroll_y(&mut self, handle: UIBoxHandle, enabled: bool) -> &mut Self {
        if enabled {
            self.boxes[handle.idx].flags |= UIBoxFlags::SCROLL_Y;
            self.absorb_pending_scroll_for_box(handle.idx);
            let key = self.boxes[handle.idx].key;
            let flags = self.boxes[handle.idx].flags;
            self.apply_scrollbar_events(handle.idx, key, flags);
        } else {
            self.boxes[handle.idx].flags.0 &= !UIBoxFlags::SCROLL_Y.0;
        }
        self
    }

    pub(super) fn clip(&mut self, handle: UIBoxHandle, enabled: bool) -> &mut Self {
        if enabled {
            self.boxes[handle.idx].flags |= UIBoxFlags::CLIP;
        } else {
            self.boxes[handle.idx].flags.0 &= !UIBoxFlags::CLIP.0;
        }
        self
    }

    pub(super) fn align(
        &mut self,
        handle: UIBoxHandle,
        main: MainAxisAlign,
        cross: CrossAxisAlign,
    ) -> &mut Self {
        self.boxes[handle.idx].main_axis_align = main;
        self.boxes[handle.idx].cross_axis_align = cross;
        self
    }

    pub(super) fn configure_button_box(&mut self, handle: UIBoxHandle) {
        self.boxes[handle.idx].pref_size = [UISize::TextContent(16.0), UISize::TextContent(10.0)];
        self.boxes[handle.idx].padding = Padding {
            top: 5.0,
            right: 8.0,
            bottom: 5.0,
            left: 8.0,
        };
        self.boxes[handle.idx].style.bg_color = self.theme.surface_bg;
        self.boxes[handle.idx].style.border_color = self.theme.border;
        self.boxes[handle.idx].style.corner_radius = self.theme.radius;
    }

    pub(super) fn show_tooltip_for_hover(
        &mut self,
        handle: UIBoxHandle,
        tooltip_text: Option<&str>,
    ) {
        let (Some(text), Some(mouse)) = (tooltip_text, self.mouse) else {
            return;
        };
        if !handle.hover() {
            return;
        }

        let offset = 12.0;
        let tooltip = self.floating_pane_at(
            Point::new(mouse.x + offset, mouse.y + offset),
            Some("#tooltip"),
            |ui| {
                let label = ui.label(text);
                ui.padding_all(label, 5.0);
            },
        );
        self.background(tooltip, self.theme.popover_bg);
        self.border_color(tooltip, self.theme.border);
        self.corner_radius(tooltip, self.theme.radius);
        self.padding_all(tooltip, 4.0);

        // Keep the tooltip on-screen near a window border. Its size must be known *this*
        // frame: relying on the previous frame's `computed_size` would leave one frame at
        // the un-flipped anchor, which flickers. So we measure synchronously, mirroring the
        // layout math (the label's `TextContent` size — text + its padding/margin — plus
        // the pane's padding). When the default down-right placement would overflow, flip
        // to the other side of the cursor so the tooltip never lands under the pointer (an
        // overlay box blocks the pointer, which would otherwise suppress the hover that
        // spawned it and make the tooltip flicker).
        let (width, height) = self.tooltip_measure(tooltip.idx);
        let win = self.size;
        let margin = 4.0;
        let mut x = mouse.x + offset;
        let mut y = mouse.y + offset;
        if x + width + margin > win.width {
            x = mouse.x - offset - width;
        }
        x = x.clamp(margin, (win.width - width - margin).max(margin));
        if y + height + margin > win.height {
            y = mouse.y - offset - height;
        }
        y = y.clamp(margin, (win.height - height - margin).max(margin));
        self.boxes[tooltip.idx].fixed_position = Point::new(x, y);
    }

    /// Measure a freshly-built tooltip pane the way layout will, so it can be positioned
    /// on the same frame it appears. Replicates `calc_standalone` for the label's
    /// `TextContent` size and `calc_upwards` for the pane's `ChildrenSum` (single child),
    /// reading the boxes' real padding/margin rather than hardcoding constants.
    fn tooltip_measure(&mut self, pane_idx: usize) -> (f32, f32) {
        let pane_padding = self.boxes[pane_idx].padding;
        let Some(&label_idx) = self.boxes[pane_idx].children.first() else {
            return (pane_padding.horizontal(), pane_padding.vertical());
        };
        let font_size = self.boxes[label_idx].style.font_size;
        let (text_w, text_h) = self.text_size_for_box(label_idx, font_size);
        let label_padding = self.boxes[label_idx].padding;
        let label_margin = self.boxes[label_idx].style.margin;
        let width =
            text_w + label_padding.horizontal() + label_margin * 2.0 + pane_padding.horizontal();
        let height = text_h.max(font_size)
            + label_padding.vertical()
            + label_margin * 2.0
            + pane_padding.vertical();
        (width, height)
    }

    pub(super) fn apply_click_to_focus(&mut self, handle: UIBoxHandle) {
        if self.boxes[handle.idx].flags.click_to_focus() && (handle.pressed() || handle.clicked()) {
            self.focus_key = Some(handle.key);
        }
    }

    pub(super) fn box_is_focused(&self, handle: UIBoxHandle) -> bool {
        self.focus_key == Some(handle.key)
    }

    pub(super) fn set_edit_display_text(
        &mut self,
        handle: UIBoxHandle,
        buffer: &str,
        masked: bool,
    ) {
        let mut display = if masked {
            "*".repeat(buffer.chars().count())
        } else {
            buffer.to_string()
        };
        // Inject the IME preedit (composing text) inline at the caret while focused.
        // Skipped for masked fields so a password isn't revealed mid-composition.
        if !masked && self.box_is_focused(handle) {
            if let Some(preedit) = self.ime_preedit.clone()
                && !preedit.is_empty()
            {
                let caret = self.text_cursor(handle.key());
                let caret_byte = char_to_byte(&display, caret);
                display.insert_str(caret_byte, &preedit);
            }
        }
        if self.boxes[handle.idx].display_string.as_deref() != Some(display.as_str()) {
            self.boxes[handle.idx].measured_text = None;
        }
        self.boxes[handle.idx].string = Some(display.clone());
        self.boxes[handle.idx].display_string = Some(display);
    }

    pub(super) fn box_from_key(&self, key: UiKey) -> Option<usize> {
        if key.is_zero() {
            None
        } else {
            self.box_table.get(&key).copied()
        }
    }

    pub(super) fn allocate_box_storage(
        &mut self,
        key: UiKey,
        flags: UIBoxFlags,
        display_string: Option<String>,
        key_id: Option<String>,
    ) -> usize {
        if let Some(idx) = self.free_boxes.pop() {
            self.boxes[idx] = UIBox::new(key, flags, display_string, key_id, &self.theme);
            idx
        } else {
            let idx = self.boxes.len();
            self.boxes
                .push(UIBox::new(key, flags, display_string, key_id, &self.theme));
            idx
        }
    }

    pub(super) fn alloc_box(&mut self, label: Option<&str>, flags: UIBoxFlags) -> UIBoxHandle {
        let parent_idx = self.parent_stack.last().copied();
        let seed = parent_idx.map(|idx| self.boxes[idx].key.0).unwrap_or(0);
        let box_is_overlay = parent_idx.is_some_and(|idx| {
            idx == self.overlay_root || self.box_has_ancestor(idx, self.overlay_root)
        });
        let key_string = label.map(hash_part_from_key_string);
        let mut key = key_string
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| UiKey(u64_hash_from_string(seed, s)))
            .unwrap_or_default();
        let display_string = label
            .map(display_part_from_key_string)
            .filter(|s| !s.is_empty());
        let mut existing_idx = self.box_from_key(key);
        if let Some(idx) = existing_idx {
            if self.boxes[idx].last_touched_frame == self.build_index {
                key = UiKey::default();
                existing_idx = None;
            }
        }

        let signal = self.signal_from_key_and_flags(key, flags, existing_idx, box_is_overlay);
        let idx = if let Some(idx) = existing_idx {
            self.boxes[idx].reset_for_frame(key, flags, display_string, key_string, &self.theme);
            idx
        } else {
            self.allocate_box_storage(key, flags, display_string, key_string)
        };

        if !key.is_zero() && existing_idx.is_none() {
            self.box_table.insert(key, idx);
            self.boxes[idx].first_touched_frame = self.build_index;
        }

        self.boxes[idx].parent = parent_idx;
        self.boxes[idx].signal = signal;
        self.boxes[idx].last_touched_frame = self.build_index;
        self.apply_scroll_signal(idx);

        if let Some(parent_idx) = parent_idx {
            self.boxes[parent_idx].children.push(idx);
        }
        self.frame_boxes.push(idx);
        UIBoxHandle { idx, key, signal }
    }

    pub(super) fn apply_scroll_signal(&mut self, idx: usize) {
        let signal = self.boxes[idx].signal;
        if self.boxes[idx].flags.scrolls_x() && signal.scroll_x != 0.0 {
            self.boxes[idx].scroll_target.x -= signal.scroll_x * 16.0;
            self.boxes[idx].scroll_target.x = self.boxes[idx].scroll_target.x.max(0.0);
        }
        if self.boxes[idx].flags.scrolls_y() && signal.scroll_y != 0.0 {
            self.boxes[idx].scroll_target.y -= signal.scroll_y * 16.0;
            self.boxes[idx].scroll_target.y = self.boxes[idx].scroll_target.y.max(0.0);
        }
    }

    pub(super) fn release_box(&mut self, idx: usize) {
        self.boxes[idx] = UIBox::new(UiKey::default(), UIBoxFlags::NONE, None, None, &self.theme);
        self.free_boxes.push(idx);
    }

    pub(super) fn prune_boxes(&mut self) {
        let frame = self.build_index;
        let stale_keys: Vec<UiKey> = self
            .box_table
            .iter()
            .filter_map(|(key, &idx)| (self.boxes[idx].last_touched_frame < frame).then_some(*key))
            .collect();

        for key in stale_keys {
            if let Some(idx) = self.box_table.remove(&key) {
                self.release_box(idx);
            }
        }

        let transient_boxes: Vec<usize> = self
            .frame_boxes
            .iter()
            .copied()
            .filter(|&idx| self.boxes[idx].key.is_zero())
            .collect();
        for idx in transient_boxes {
            self.release_box(idx);
        }

        if self
            .active_left_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.active_left_key = None;
        }
        if self
            .active_right_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.active_right_key = None;
        }
        if self
            .active_scrollbar
            .is_some_and(|drag| !self.box_table.contains_key(&drag.key))
        {
            self.active_scrollbar = None;
        }
        if self
            .hot_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.hot_key = None;
        }
        if self
            .focus_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.focus_key = None;
        }
        if self
            .next_focus_key
            .is_some_and(|key| !self.box_table.contains_key(&key))
        {
            self.next_focus_key = None;
        }
    }
}
