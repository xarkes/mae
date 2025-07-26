use crate::imui::{UILayout, uibox::UIBoxFlag};

use super::{
    IMUI, Point, Size, UISize,
    uibox::{UIBoxRef, UIBoxStyle, u64_hash_from_string},
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
        let style = UIBoxStyle::default();
        let uibox = self.new_floating_root(key, pos, size, style);
        self.handle_uibox_event(uibox.clone());
        {
            self.parent_stack.push(uibox.clone());
            let foldable_frame_id = "fp_frame";

            // xarkes: draw floating pane horizontal title bar
            let style = UIBoxStyle::default();
            self.horizontal(|ui| {
                if ui
                    .button(
                        (UISize::TextContent, UISize::TextContent),
                        "Fold >##fp_fold",
                        style,
                    )
                    .borrow()
                    .clicked()
                {
                    let (key, _) = ui.get_key_from_string(Some(foldable_frame_id), uibox.clone());
                    if let Some(uibox) = ui.uiboxes.get(&key) {
                        let old_visible = uibox.borrow().visible;
                        uibox.borrow_mut().visible = !old_visible;
                    }
                }
                ui.label(title, style);
            });

            // xarkes: draw pane content
            children(self);
            self.parent_stack.pop();
        }
        uibox
    }

    // TODO --> api needs scrolling view size
    fn scrollbar(&mut self, scrollable: UIBoxRef) {
        debug_assert!(scrollable.borrow().scrollable_x());
        // XXX: not yet resilient against multiple scrollbars in one root -> key collision
        let container = self.add_box_from_string(
            (UISize::Percents(1.0), UISize::Percents(1.0)),
            None,
            self.parent().borrow().style.clone(),
            UIBoxFlag::DrawBackground as u64,
        );
        container.borrow_mut().layout = Some(UILayout::Horizontal);
        self.parent_stack.push(container);
        {
            let vsize = 1000.;
            let virtual_size = f32::max(scrollable.borrow().size.width, vsize);
            let scroll_pos = f32::max(0., scrollable.borrow().scrollx);
            // let pre_scrollbar = self
            //     .add_box_from_string(Some("#scrollbar_pre_x"), UIBoxFlag::DrawBackground as u64);
            // let scrollbar = self.add_box_from_string(
            //     Some("#scrollbar_bar_x"),
            //     UIBoxFlag::DrawBackground as u64 | UIBoxFlag::Clickable as u64,
            // );
            // let post_scrollbar = self
            //     .add_box_from_string(Some("#scrollbar_post_x"), UIBoxFlag::DrawBackground as u64);
        }
        self.parent_stack.pop();
    }
}
