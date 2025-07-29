use crate::{
    imui::{
        UILayout, UISize,
        uibox::{UIBoxFlag, UIBoxParams},
    },
    uisize,
};

use super::{
    IMUI, Point, Size, color_rgb,
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
        uibox.borrow_mut().pref_size = (UISize::DPixels(size.width), UISize::Children);
        self.handle_uibox_event(uibox.clone());
        self.parent_stack.push(uibox.clone());
        {
            let foldable_frame_id = "fp_frame";

            // xarkes: draw floating pane horizontal title bar
            let bla = self.container(
                None,
                UILayout::Horizontal,
                UIBoxFlag::DrawBackground as u64,
                None,
                |ui| {
                    if ui.button("Fold > ##fp_fold").borrow().clicked() {
                        let (key, _) =
                            ui.get_key_from_string(Some(foldable_frame_id), uibox.clone());
                        if let Some(uibox) = ui.uiboxes.get(&key) {
                            let old_size = uibox.borrow().pref_size;
                            uibox.borrow_mut().pref_size.1 = match old_size.1 {
                                UISize::Children => UISize::DPixels(0.),
                                _ => UISize::Children,
                            };
                        }
                    }
                    ui.label(title);
                    if ui.button("X##fp_quit").borrow().clicked() {
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

    pub fn prompt(&mut self, title: &str, mut children: impl FnMut(&mut IMUI)) -> UIBoxRef {
        let key = u64_hash_from_string(4736251, title);
        let uibox = self.new_floating_root(
            key,
            Point::new(self.size.width / 4., self.size.height / 10.),
        );
        uibox.borrow_mut().pref_size = (
            UISize::DPixels(self.size.width / 2.),
            UISize::DPixels(self.size.height / 4.),
        );
        self.parent_stack.push(uibox.clone());
        {
            children(self);
        }
        self.parent_stack.pop();
        uibox
    }

    pub(crate) fn scrollbar(&mut self, scrollable: UIBoxRef, virtual_size: f32) {
        debug_assert!(scrollable.borrow().scrollable_x());
        if virtual_size <= scrollable.borrow().size.width {
            return;
        }
        let mut params = UIBoxParams::new();
        params.height(uisize!("20px"));
        let cont = self.container(
            None,
            UILayout::Horizontal,
            UIBoxFlag::DrawBackground as u64,
            Some(params),
            |ui| {
                let box_size = scrollable.borrow().size;
                let scroll_pos = scrollable.borrow().scrollx;
                let scroll_percent = -1. * scroll_pos / virtual_size;
                let bar_size = (box_size.width / virtual_size) * box_size.width;
                let pre_size = box_size.width * scroll_percent;
                let post_size = box_size.width - bar_size - pre_size;
                debug_assert!(post_size + pre_size + bar_size == box_size.width);

                let pre_scrollbar = ui.add_box_from_string(
                    Some("#scrollbar_pre_x"),
                    UIBoxFlag::DrawBackground as u64,
                );
                pre_scrollbar
                    .borrow_mut()
                    .set_pref_size((UISize::DPixels(pre_size), uisize!("100%")));
                pre_scrollbar.borrow_mut().style.bg_color = color_rgb(255, 255, 0);
                let scrollbar = ui.add_box_from_string(
                    Some("#scrollbar_bar_x"),
                    UIBoxFlag::DrawBackground as u64 | UIBoxFlag::Clickable as u64,
                );
                scrollbar
                    .borrow_mut()
                    .set_pref_size((UISize::DPixels(bar_size), uisize!("100%")));
                scrollbar.borrow_mut().style.bg_color = color_rgb(0, 255, 255);
                let post_scrollbar = ui.add_box_from_string(
                    Some("#scrollbar_post_x"),
                    UIBoxFlag::DrawBackground as u64,
                );
                post_scrollbar
                    .borrow_mut()
                    .set_pref_size((UISize::DPixels(post_size), uisize!("100%")));
                post_scrollbar.borrow_mut().style.bg_color = color_rgb(255, 0, 255);
            },
        );
        cont.borrow_mut().pref_size.0 = uisize!("100%");
    }
}
