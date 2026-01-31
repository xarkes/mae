use crate::imui::{
    UILayout, UISize,
    uibox::{UIBoxFlag, UIBoxParams},
};

use super::{
    Axis, IMUI, Point, Size, color_rgb,
    uibox::{UIBoxRef, u64_hash_from_string},
};

impl IMUI {
    pub fn floating_pane(
        &mut self,
        pos: Point,
        size: Size,
        title: &str,
        children: impl FnMut(&mut IMUI),
    ) -> UIBoxRef {
        let key = u64_hash_from_string(4736251, title);
        let uibox = self.new_floating_root(key, pos);
        uibox.borrow_mut().width = UISize::Fixed(size.width);
        uibox.borrow_mut().height = UISize::Fit;
        self.handle_uibox_event(uibox.clone());
        self.parent_stack.push(uibox.clone());
        {
            let foldable_frame_id = "fp_frame";

            // xarkes: draw floating pane horizontal title bar
            self.container(
                None,
                UILayout::Horizontal,
                UIBoxFlag::DrawBackground as u64,
                None,
                |ui| {
                    if ui.button("Fold > ##fp_fold", None).borrow().clicked() {
                        let (key, _) =
                            ui.get_key_from_string(Some(foldable_frame_id), uibox.clone());
                        if let Some(uibox) = ui.uiboxes.get(&key) {
                            let old_size = uibox.borrow().height;
                            uibox.borrow_mut().height = match old_size {
                                UISize::Fit => UISize::Fixed(0.),
                                _ => UISize::Fit,
                            };
                        }
                    }
                    ui.label(title);
                    if ui.button("X##fp_quit", None).borrow().clicked() {
                        uibox.borrow_mut().visible = false;
                    }
                },
            );

            // xarkes: draw pane content
            self.container(
                Some(foldable_frame_id),
                UILayout::Vertical,
                0,
                None,
                children,
            );
        }
        self.parent_stack.pop();
        uibox
    }

    pub fn prompt(
        &mut self,
        title: &str,
        show: &mut bool,
        mut children: impl FnMut(&mut IMUI, &mut bool),
    ) {
        let key = u64_hash_from_string(4736251, title);
        if self.uiboxes.contains_key(&key) && self.prompt.is_none() {
            // uibox was created before, but self.prompt is None, we should revert the bool
            self.uiboxes.remove(&key);
            *show = false;
            return;
        }
        if *show {
            let uibox = self.new_floating_root(
                key,
                Point::new(self.size.width / 4., self.size.height / 10.),
            );
            uibox.borrow_mut().width = UISize::Fixed(self.size.width / 2.);
            uibox.borrow_mut().height = UISize::Fixed(self.size.height / 4.);
            self.parent_stack.push(uibox.clone());
            {
                children(self, show);
            }
            self.parent_stack.pop();

            self.prompt = Some(uibox.clone());
        }
    }

    pub(crate) fn clear_prompt(&mut self) {
        if self.prompt.is_some() {
            self.prompt = None;
        }
    }

    pub(crate) fn scrollbar(&mut self, scrollable: UIBoxRef, virtual_size: f32, axis: Axis) {
        debug_assert!(scrollable.borrow().scrollable_x());
        if virtual_size <= *scrollable.borrow().computed_size.axis(axis) {
            return;
        }
        let mut params = UIBoxParams::new();
        match axis {
            Axis::X => {
                params.height(UISize::Fixed(10.));
            }
            Axis::Y => {
                params.width(UISize::Fixed(10.));
            }
        }
        let layout = match axis {
            Axis::X => UILayout::Horizontal,
            Axis::Y => UILayout::Vertical,
        };
        let axis_letter = match axis {
            Axis::X => 'x',
            Axis::Y => 'y',
        };
        self.container(None, layout, 0, Some(params), |ui| {
            let box_size = scrollable.borrow().computed_size;
            let scroll_pos = match axis {
                Axis::X => scrollable.borrow().scrollx,
                Axis::Y => scrollable.borrow().scrolly,
            };
            let scroll_percent = -1. * scroll_pos / virtual_size;
            let bar_size = (box_size.axis(axis) / virtual_size) * box_size.axis(axis);
            let pre_size = box_size.axis(axis) * scroll_percent;
            let post_size = box_size.axis(axis) - bar_size - pre_size;
            debug_assert!(
                (post_size + pre_size + bar_size).round() == box_size.axis(axis).round(),
                "{} {}",
                post_size + pre_size + bar_size,
                *box_size.axis(axis)
            );

            let bar_color = color_rgb(100, 100, 100);
            let bar_empty_color = color_rgb(40, 40, 40);

            let pre_scrollbar = ui.add_box_from_string(
                Some(format!("#scrollbar_pre_{}", axis_letter).as_str()),
                UIBoxFlag::DrawBackground as u64,
            );
            match axis {
                Axis::X => {
                    pre_scrollbar.borrow_mut().width = UISize::Fixed(pre_size);
                    pre_scrollbar.borrow_mut().height = UISize::Percent(1.);
                }
                Axis::Y => {
                    pre_scrollbar.borrow_mut().width = UISize::Percent(1.);
                    pre_scrollbar.borrow_mut().height = UISize::Fixed(pre_size);
                }
            }
            pre_scrollbar.borrow_mut().style.bg_color = bar_empty_color;
            let scrollbar = ui.add_box_from_string(
                Some(format!("#scrollbar_bar_{}", axis_letter).as_str()),
                UIBoxFlag::DrawBackground as u64
                    | UIBoxFlag::Clickable as u64
                    | UIBoxFlag::DrawHot as u64,
            );
            match axis {
                Axis::X => {
                    scrollbar.borrow_mut().width = UISize::Fixed(bar_size);
                    scrollbar.borrow_mut().height = UISize::Percent(1.);
                }
                Axis::Y => {
                    scrollbar.borrow_mut().width = UISize::Percent(1.);
                    scrollbar.borrow_mut().height = UISize::Fixed(bar_size);
                }
            }
            scrollbar.borrow_mut().style.bg_color = bar_color;
            let post_scrollbar = ui.add_box_from_string(
                Some(format!("#scrollbar_post_{}", axis_letter).as_str()),
                UIBoxFlag::DrawBackground as u64,
            );
            match axis {
                Axis::X => {
                    post_scrollbar.borrow_mut().width = UISize::Fixed(post_size);
                    post_scrollbar.borrow_mut().height = UISize::Percent(1.);
                }
                Axis::Y => {
                    post_scrollbar.borrow_mut().width = UISize::Percent(1.);
                    post_scrollbar.borrow_mut().height = UISize::Fixed(post_size);
                }
            }
            post_scrollbar.borrow_mut().style.bg_color = bar_empty_color;
        });
    }
}
