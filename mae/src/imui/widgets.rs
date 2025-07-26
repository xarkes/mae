use crate::{
    imui::UISize,
    imui::{UILayout, uibox::UIBoxFlag},
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
        mut children: impl FnMut(&mut IMUI),
    ) -> UIBoxRef {
        let key = u64_hash_from_string(4736251, title);
        let uibox = self.new_floating_root(key, pos, size);
        // uibox.borrow_mut().pref_size = (UISize::Children, UISize::Children);
        self.handle_uibox_event(uibox.clone());
        self.parent_stack.push(uibox.clone());
        {
            let foldable_frame_id = "fp_frame";

            // xarkes: draw floating pane horizontal title bar
            self.container(
                UILayout::Horizontal,
                UIBoxFlag::DrawBackground as u64,
                |ui| {
                    if ui.button("Fold >##fp_fold").borrow().clicked() {
                        let (key, _) =
                            ui.get_key_from_string(Some(foldable_frame_id), uibox.clone());
                        if let Some(uibox) = ui.uiboxes.get(&key) {
                            let old_visible = uibox.borrow().visible;
                            uibox.borrow_mut().visible = !old_visible;
                        }
                    }
                    ui.label(title);
                },
            );

            // xarkes: draw pane content
            let (key, _) = self.get_key_from_string(Some(foldable_frame_id), uibox.clone());
            let frame = self.get_or_create_box_from_key(key, None, 0, false);
            self.parent_stack
                .last()
                .unwrap()
                .borrow_mut()
                .children
                .push(frame.clone());
            frame.borrow_mut().layout = Some(UILayout::Vertical);
            frame.borrow_mut().pref_size = (uisize!("100%"), uisize!("100%"));
            frame.borrow_mut().style.bg_color = color_rgb(255, 255, 0);
            self.parent_stack.push(frame);
            self.label("");
            children(self);
            self.parent_stack.pop();
        }
        self.parent_stack.pop();
        uibox
    }

    // TODO --> api needs scrolling view size
    fn scrollbar(&mut self, scrollable: UIBoxRef) {
        debug_assert!(scrollable.borrow().scrollable_x());
        // XXX: not yet resilient against multiple scrollbars in one root -> key collision
        let container = self.add_box_from_string(None, UIBoxFlag::DrawBackground as u64);
        container.borrow_mut().layout = Some(UILayout::Horizontal);
        self.parent_stack.push(container);
        {
            let vsize = 1000.;
            let virtual_size = f32::max(scrollable.borrow().size.width, vsize);
            let scroll_pos = f32::max(0., scrollable.borrow().scrollx);
            let pre_scrollbar = self
                .add_box_from_string(Some("#scrollbar_pre_x"), UIBoxFlag::DrawBackground as u64);
            let scrollbar = self.add_box_from_string(
                Some("#scrollbar_bar_x"),
                UIBoxFlag::DrawBackground as u64 | UIBoxFlag::Clickable as u64,
            );
            let post_scrollbar = self
                .add_box_from_string(Some("#scrollbar_post_x"), UIBoxFlag::DrawBackground as u64);
        }
        self.parent_stack.pop();
    }
}
