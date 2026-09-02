use super::*;

impl IMUI {
    pub(super) fn layout_root(&mut self, root: usize) {
        for axis in [Axis::X, Axis::Y] {
            self.calc_standalone(root, axis);
            self.calc_upwards(root, axis);
            self.calc_downwards(root, axis);
            self.enforce_constraints(root, axis);
            self.reconcile_overflow(root, axis);
            self.clamp_scroll_offsets(root, axis);
            self.position(root, axis);
        }
    }

    pub(super) fn clamp_scroll_offsets(&mut self, idx: usize, axis: Axis) {
        if self.boxes[idx].child_layout_axis == axis {
            let content_size = (self.boxes[idx].computed_size.axis(axis)
                - self.boxes[idx].padding.axis(axis))
            .max(0.0);
            let used_size = self.total_children_size(idx, axis);
            let max_scroll = (used_size - content_size).max(0.0);
            self.boxes[idx].content_size.set_axis(axis, used_size);
            self.boxes[idx].scroll_max.set_axis(axis, max_scroll);
            match axis {
                Axis::X => {
                    self.boxes[idx].scroll.x = self.boxes[idx].scroll.x.clamp(0.0, max_scroll);
                    self.boxes[idx].scroll_target.x =
                        self.boxes[idx].scroll_target.x.clamp(0.0, max_scroll);
                }
                Axis::Y => {
                    self.boxes[idx].scroll.y = self.boxes[idx].scroll.y.clamp(0.0, max_scroll);
                    self.boxes[idx].scroll_target.y =
                        self.boxes[idx].scroll_target.y.clamp(0.0, max_scroll);
                }
            }
        } else if (axis == Axis::X && self.boxes[idx].flags.scrolls_x())
            || (axis == Axis::Y && self.boxes[idx].flags.scrolls_y())
        {
            let content_size = (self.boxes[idx].computed_size.axis(axis)
                - self.boxes[idx].padding.axis(axis))
            .max(0.0);
            let used_size = self.max_child_size(idx, axis);
            let max_scroll = (used_size - content_size).max(0.0);
            self.boxes[idx].content_size.set_axis(axis, used_size);
            self.boxes[idx].scroll_max.set_axis(axis, max_scroll);
            match axis {
                Axis::X => {
                    self.boxes[idx].scroll.x = self.boxes[idx].scroll.x.clamp(0.0, max_scroll);
                    self.boxes[idx].scroll_target.x =
                        self.boxes[idx].scroll_target.x.clamp(0.0, max_scroll);
                }
                Axis::Y => {
                    self.boxes[idx].scroll.y = self.boxes[idx].scroll.y.clamp(0.0, max_scroll);
                    self.boxes[idx].scroll_target.y =
                        self.boxes[idx].scroll_target.y.clamp(0.0, max_scroll);
                }
            }
        }
        let child_len = self.boxes[idx].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[idx].children[child_pos];
            self.clamp_scroll_offsets(child, axis);
        }
    }

    pub(super) fn reconcile_overflow(&mut self, idx: usize, axis: Axis) {
        self.reconcile_container_overflow(idx, axis);
        let child_len = self.boxes[idx].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[idx].children[child_pos];
            self.reconcile_overflow(child, axis);
        }
    }

    pub(super) fn reconcile_container_overflow(&mut self, parent: usize, axis: Axis) {
        if self.boxes[parent].child_layout_axis != axis {
            return;
        }
        if (axis == Axis::X && self.boxes[parent].flags.scrolls_x())
            || (axis == Axis::Y && self.boxes[parent].flags.scrolls_y())
        {
            // Scroll containers keep child sizes and rely on scrolling instead of shrinking.
            return;
        }
        let mut child_count = 0usize;
        let mut sum_children = 0.0f32;
        for &child in &self.boxes[parent].children {
            if self.box_is_out_of_flow(child) {
                continue;
            }
            child_count += 1;
            sum_children += self.boxes[child].computed_size.axis(axis);
        }
        if child_count == 0 {
            return;
        }

        let content_size = (self.boxes[parent].computed_size.axis(axis)
            - self.boxes[parent].padding.axis(axis))
        .max(0.0);
        let gaps = self.boxes[parent].child_gap * child_count.saturating_sub(1) as f32;
        let mut overflow = (sum_children + gaps - content_size).max(0.0);
        if overflow <= 0.0 {
            return;
        }

        overflow = self.shrink_group_to_fit(parent, axis, overflow, true);
        if overflow > 0.0 {
            overflow = self.shrink_group_to_fit(parent, axis, overflow, false);
        }
        if overflow > 0.0 {
            // Hard fallback guarantee: shrink from tail down to zero.
            for child_pos in (0..self.boxes[parent].children.len()).rev() {
                if overflow <= 0.0 {
                    break;
                }
                let child = self.boxes[parent].children[child_pos];
                if self.box_is_out_of_flow(child) {
                    continue;
                }
                let cur = self.boxes[child].computed_size.axis(axis);
                let take = cur.min(overflow);
                self.boxes[child].computed_size.set_axis(axis, cur - take);
                overflow -= take;
            }
        }
    }

    pub(super) fn shrink_group_to_fit(
        &mut self,
        parent: usize,
        axis: Axis,
        mut overflow: f32,
        fill_only: bool,
    ) -> f32 {
        if overflow <= 0.0 {
            return 0.0;
        }
        let mut total_capacity = 0.0f32;
        for &idx in &self.boxes[parent].children {
            if self.box_is_out_of_flow(idx) {
                continue;
            }
            let is_fill = self.boxes[idx].pref_size[axis_idx(axis)] == UISize::Fill;
            if fill_only != is_fill {
                continue;
            }
            let cur = self.boxes[idx].computed_size.axis(axis);
            let min = self.boxes[idx].min_size.axis(axis);
            total_capacity += (cur - min).max(0.0);
        }
        if total_capacity <= 0.0 {
            return overflow;
        }

        let target = overflow.min(total_capacity);
        let mut taken_total = 0.0;
        let child_len = self.boxes[parent].children.len();
        for child_pos in 0..child_len {
            let idx = self.boxes[parent].children[child_pos];
            if self.box_is_out_of_flow(idx) {
                continue;
            }
            let is_fill = self.boxes[idx].pref_size[axis_idx(axis)] == UISize::Fill;
            if fill_only != is_fill {
                continue;
            }
            let cur = self.boxes[idx].computed_size.axis(axis);
            let min = self.boxes[idx].min_size.axis(axis);
            let cap = (cur - min).max(0.0);
            if cap <= 0.0 {
                continue;
            }
            let take = (target * (cap / total_capacity)).min(cap);
            self.boxes[idx].computed_size.set_axis(axis, cur - take);
            taken_total += take;
        }

        overflow -= taken_total;
        overflow.max(0.0)
    }

    pub(super) fn calc_standalone(&mut self, idx: usize, axis: Axis) {
        let child_len = self.boxes[idx].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[idx].children[child_pos];
            self.calc_standalone(child, axis);
        }
        let pref = self.boxes[idx].pref_size[axis_idx(axis)];
        let value = match pref {
            UISize::Pixels(v) => v,
            UISize::TextContent(padding) => {
                let font_size = self.boxes[idx].style.font_size;
                let margin = self.boxes[idx].style.margin;
                let wrap = self.boxes[idx].flags.contains(UIBoxFlags::TEXT_WRAP);
                match axis {
                    Axis::X => {
                        let (w, _) = self.text_size_for_box(idx, font_size);
                        w + padding + self.boxes[idx].padding.horizontal() + margin * 2.0
                    }
                    // Layout solves X before Y, so the box's width is final here:
                    // a wrapping box measures its line count against that width.
                    Axis::Y if wrap => {
                        let font_icon = self.boxes[idx].style.font_icon;
                        let avail = (self.boxes[idx].computed_size.width
                            - self.boxes[idx].padding.horizontal()
                            - margin * 2.0)
                            .max(0.0);
                        let lines = self.ensure_wrapped(idx, font_size, font_icon, avail);
                        lines as f32 * self.line_height_for(font_size)
                            + padding
                            + self.boxes[idx].padding.vertical()
                            + margin * 2.0
                    }
                    Axis::Y => {
                        let (_, h) = self.text_size_for_box(idx, font_size);
                        h.max(font_size)
                            + padding
                            + self.boxes[idx].padding.vertical()
                            + margin * 2.0
                    }
                }
            }
            UISize::ParentPct(_) | UISize::ChildrenSum | UISize::Fill => {
                self.boxes[idx].min_size.axis(axis)
            }
        };
        self.boxes[idx].computed_size.set_axis(axis, value);
    }

    pub(super) fn calc_upwards(&mut self, idx: usize, axis: Axis) {
        let child_len = self.boxes[idx].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[idx].children[child_pos];
            self.calc_upwards(child, axis);
        }
        if self.boxes[idx].pref_size[axis_idx(axis)] != UISize::ChildrenSum {
            return;
        }
        let child_axis = self.boxes[idx].child_layout_axis;
        let mut size: f32 = 0.0;
        if axis == child_axis {
            for child in &self.boxes[idx].children {
                if self.box_is_out_of_flow(*child) {
                    continue;
                }
                size += self.boxes[*child].computed_size.axis(axis);
            }
            let child_count = self.in_flow_child_count(idx);
            if child_count > 1 {
                size += self.boxes[idx].child_gap * (child_count - 1) as f32;
            }
        } else {
            for child in &self.boxes[idx].children {
                if self.box_is_out_of_flow(*child) {
                    continue;
                }
                size = size.max(self.boxes[*child].computed_size.axis(axis));
            }
        }
        size += self.boxes[idx].padding.axis(axis);
        self.boxes[idx].computed_size.set_axis(axis, size);
    }

    pub(super) fn calc_downwards(&mut self, idx: usize, axis: Axis) {
        let parent_content = if let Some(parent) = self.boxes[idx].parent {
            (self.boxes[parent].computed_size.axis(axis) - self.boxes[parent].padding.axis(axis))
                .max(0.0)
        } else {
            self.boxes[idx].computed_size.axis(axis)
        };
        self.apply_downward_size(idx, axis, parent_content);

        // Resolve direct children on this axis before recursing so descendants
        // observe the final parent size (especially for Fill children).
        let child_len = self.boxes[idx].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[idx].children[child_pos];
            if self.box_is_out_of_flow(child) {
                continue;
            }
            let content = (self.boxes[idx].computed_size.axis(axis)
                - self.boxes[idx].padding.axis(axis))
            .max(0.0);
            self.apply_downward_size(child, axis, content);
        }
        if self.boxes[idx].child_layout_axis == axis {
            self.distribute_fill_children(idx, axis);
        }

        let child_len = self.boxes[idx].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[idx].children[child_pos];
            self.calc_downwards(child, axis);
        }
    }

    pub(super) fn apply_downward_size(&mut self, idx: usize, axis: Axis, parent_content: f32) {
        match self.boxes[idx].pref_size[axis_idx(axis)] {
            UISize::ParentPct(pct) => self.boxes[idx]
                .computed_size
                .set_axis(axis, (parent_content * pct).max(0.0)),
            UISize::Fill => {
                // On the parent's main axis, Fill is resolved by
                // `distribute_fill_children`; don't overwrite that result here.
                if let Some(parent) = self.boxes[idx].parent {
                    if self.boxes[parent].child_layout_axis == axis {
                        return;
                    }
                }
                // On cross-axis, Fill behaves like ParentPct(1.0).
                self.boxes[idx]
                    .computed_size
                    .set_axis(axis, parent_content.max(0.0));
            }
            _ => {}
        }
    }

    pub(super) fn enforce_constraints(&mut self, idx: usize, axis: Axis) {
        let min = self.boxes[idx].min_size.axis(axis);
        let size = self.boxes[idx].computed_size.axis(axis).max(min);
        self.boxes[idx].computed_size.set_axis(axis, size);
        let child_len = self.boxes[idx].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[idx].children[child_pos];
            self.enforce_constraints(child, axis);
        }
    }

    pub(super) fn position(&mut self, idx: usize, axis: Axis) {
        self.position_with_origin(idx, axis, None);
    }

    pub(super) fn position_with_origin(
        &mut self,
        idx: usize,
        axis: Axis,
        origin_override: Option<f32>,
    ) {
        let origin = if self.boxes[idx].flags.contains(match axis {
            Axis::X => UIBoxFlags::FLOATING_X,
            Axis::Y => UIBoxFlags::FLOATING_Y,
        }) {
            self.boxes[idx].fixed_position.axis(axis)
        } else if let Some(origin) = origin_override {
            origin
        } else if let Some(parent) = self.boxes[idx].parent {
            let parent_axis = self.boxes[parent].child_layout_axis;
            if axis == parent_axis {
                self.position_on_main_axis(idx, parent, axis)
            } else {
                self.position_on_cross_axis(idx, parent, axis)
            }
        } else {
            0.0
        };
        self.set_rect_axis(idx, axis, origin, self.boxes[idx].computed_size.axis(axis));
        self.distribute_fill_children(idx, axis);
        if self.boxes[idx].child_layout_axis == axis {
            self.position_children_on_main_axis(idx, axis);
            return;
        }
        let child_len = self.boxes[idx].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[idx].children[child_pos];
            self.position_with_origin(child, axis, None);
        }
    }

    pub(super) fn position_children_on_main_axis(&mut self, parent: usize, axis: Axis) {
        let mut child_count = 0usize;
        let mut total_size = 0.0f32;
        for &child in &self.boxes[parent].children {
            if self.box_is_out_of_flow(child) {
                continue;
            }
            child_count += 1;
            total_size += self.boxes[child].computed_size.axis(axis);
        }

        let padding_start = self.boxes[parent].padding.min_axis(axis);
        let padding_end = self.boxes[parent].padding.max_axis(axis);
        let content_start = self.boxes[parent].rect_axis_min(axis) + padding_start;
        let content_size =
            (self.boxes[parent].computed_size.axis(axis) - padding_start - padding_end).max(0.0);
        let total_children_size = if child_count > 1 {
            total_size + self.boxes[parent].child_gap * (child_count - 1) as f32
        } else {
            total_size
        };
        let extra = (content_size - total_children_size).max(0.0);
        let mut start = content_start;
        let mut gap = self.boxes[parent].child_gap;
        match self.boxes[parent].main_axis_align {
            MainAxisAlign::Start => {}
            MainAxisAlign::Center => start += extra / 2.0,
            MainAxisAlign::End => start += extra,
            MainAxisAlign::SpaceBetween if child_count > 1 => {
                gap += extra / (child_count - 1) as f32;
            }
            MainAxisAlign::SpaceAround if child_count > 0 => {
                gap += extra / child_count as f32;
                start += gap / 2.0;
            }
            MainAxisAlign::SpaceEvenly if child_count > 0 => {
                gap += extra / (child_count + 1) as f32;
                start += gap;
            }
            _ => {}
        }

        let scroll = if self.boxes[parent].flags.scrolls_x() && axis == Axis::X {
            self.boxes[parent].scroll.x
        } else if self.boxes[parent].flags.scrolls_y() && axis == Axis::Y {
            self.boxes[parent].scroll.y
        } else {
            0.0
        };
        let mut pos = start - scroll;
        let child_len = self.boxes[parent].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[parent].children[child_pos];
            if self.box_is_out_of_flow(child) {
                self.position_with_origin(child, axis, None);
                continue;
            }
            let child_origin = pos;
            pos += self.boxes[child].computed_size.axis(axis) + gap;
            self.position_with_origin(child, axis, Some(child_origin));
        }
    }

    pub(super) fn position_on_main_axis(&self, idx: usize, parent: usize, axis: Axis) -> f32 {
        if self.box_is_out_of_flow(idx) {
            return self.boxes[idx].fixed_position.axis(axis);
        }
        let mut child_count = 0usize;
        let mut child_pos = 0usize;
        let mut size_before = 0.0f32;
        let mut total_size = 0.0f32;
        let mut found = false;
        for &child in &self.boxes[parent].children {
            if self.box_is_out_of_flow(child) {
                continue;
            }
            if child == idx {
                child_pos = child_count;
                found = true;
            } else if !found {
                size_before += self.boxes[child].computed_size.axis(axis);
            }
            child_count += 1;
            total_size += self.boxes[child].computed_size.axis(axis);
        }
        let padding_start = self.boxes[parent].padding.min_axis(axis);
        let padding_end = self.boxes[parent].padding.max_axis(axis);
        let content_start = self.boxes[parent].rect_axis_min(axis) + padding_start;
        let content_size =
            (self.boxes[parent].computed_size.axis(axis) - padding_start - padding_end).max(0.0);
        let total_children_size = if child_count > 1 {
            total_size + self.boxes[parent].child_gap * (child_count - 1) as f32
        } else {
            total_size
        };
        let extra = (content_size - total_children_size).max(0.0);
        let mut start = content_start;
        let mut gap = self.boxes[parent].child_gap;
        match self.boxes[parent].main_axis_align {
            MainAxisAlign::Start => {}
            MainAxisAlign::Center => start += extra / 2.0,
            MainAxisAlign::End => start += extra,
            MainAxisAlign::SpaceBetween if child_count > 1 => {
                gap += extra / (child_count - 1) as f32;
            }
            MainAxisAlign::SpaceAround if child_count > 0 => {
                gap += extra / child_count as f32;
                start += gap / 2.0;
            }
            MainAxisAlign::SpaceEvenly if child_count > 0 => {
                gap += extra / (child_count + 1) as f32;
                start += gap;
            }
            _ => {}
        }
        let mut pos = start + size_before + gap * child_pos as f32;
        if self.boxes[parent].flags.scrolls_x() && axis == Axis::X {
            pos -= self.boxes[parent].scroll.x;
        }
        if self.boxes[parent].flags.scrolls_y() && axis == Axis::Y {
            pos -= self.boxes[parent].scroll.y;
        }
        pos
    }

    pub(super) fn position_on_cross_axis(&self, idx: usize, parent: usize, axis: Axis) -> f32 {
        let padding_start = self.boxes[parent].padding.min_axis(axis);
        let padding_end = self.boxes[parent].padding.max_axis(axis);
        let base = self.boxes[parent].rect_axis_min(axis) + padding_start;
        let available =
            (self.boxes[parent].computed_size.axis(axis) - padding_start - padding_end).max(0.0);
        let child_size = self.boxes[idx].computed_size.axis(axis);
        let pos = match self.boxes[parent].cross_axis_align {
            CrossAxisAlign::Start | CrossAxisAlign::Stretch => base,
            CrossAxisAlign::Center => base + (available - child_size).max(0.0) / 2.0,
            CrossAxisAlign::End => base + (available - child_size).max(0.0),
        };
        let scroll = if axis == Axis::X && self.boxes[parent].flags.scrolls_x() {
            self.boxes[parent].scroll.x
        } else if axis == Axis::Y && self.boxes[parent].flags.scrolls_y() {
            self.boxes[parent].scroll.y
        } else {
            0.0
        };
        pos - scroll
    }

    pub(super) fn distribute_fill_children(&mut self, parent: usize, axis: Axis) {
        if self.boxes[parent].child_layout_axis != axis {
            return;
        }
        let mut fill_count = 0usize;
        let mut child_count = 0usize;
        let mut fixed = 0.0f32;
        for &child in &self.boxes[parent].children {
            if self.box_is_out_of_flow(child) {
                continue;
            }
            child_count += 1;
            if self.boxes[child].pref_size[axis_idx(axis)] == UISize::Fill {
                fill_count += 1;
            } else {
                fixed += self.boxes[child].computed_size.axis(axis);
            }
        }
        if fill_count == 0 {
            return;
        }
        let padding = self.boxes[parent].padding.axis(axis);
        let gaps = self.boxes[parent].child_gap * child_count.saturating_sub(1) as f32;
        let available =
            (self.boxes[parent].computed_size.axis(axis) - padding - gaps - fixed).max(0.0);
        let each = available / fill_count as f32;
        let child_len = self.boxes[parent].children.len();
        for child_pos in 0..child_len {
            let child = self.boxes[parent].children[child_pos];
            if self.box_is_out_of_flow(child)
                || self.boxes[child].pref_size[axis_idx(axis)] != UISize::Fill
            {
                continue;
            }
            self.boxes[child].computed_size.set_axis(axis, each);
        }
    }

    pub(super) fn total_children_size(&self, parent: usize, axis: Axis) -> f32 {
        let mut count = 0usize;
        let mut total = 0.0f32;
        for &child in &self.boxes[parent].children {
            if self.box_is_out_of_flow(child) {
                continue;
            }
            count += 1;
            total += self.boxes[child].computed_size.axis(axis);
        }
        if count > 1 {
            total += self.boxes[parent].child_gap * (count - 1) as f32;
        }
        total
    }

    pub(super) fn max_child_size(&self, parent: usize, axis: Axis) -> f32 {
        self.boxes[parent]
            .children
            .iter()
            .copied()
            .filter(|child| !self.box_is_out_of_flow(*child))
            .map(|child| self.boxes[child].computed_size.axis(axis))
            .fold(0.0, f32::max)
    }

    pub(super) fn box_is_out_of_flow(&self, idx: usize) -> bool {
        self.boxes[idx].flags.contains(UIBoxFlags::FLOATING_X)
            || self.boxes[idx].flags.contains(UIBoxFlags::FLOATING_Y)
    }

    pub(super) fn in_flow_child_count(&self, parent: usize) -> usize {
        self.boxes[parent]
            .children
            .iter()
            .filter(|child| !self.box_is_out_of_flow(**child))
            .count()
    }

    pub(super) fn set_rect_axis(&mut self, idx: usize, axis: Axis, min: f32, size: f32) {
        match axis {
            Axis::X => {
                self.boxes[idx].rect.x0 = min;
                self.boxes[idx].rect.x1 = min + size;
            }
            Axis::Y => {
                self.boxes[idx].rect.y0 = min;
                self.boxes[idx].rect.y1 = min + size;
            }
        }
    }
}
