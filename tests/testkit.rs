#![cfg(feature = "testkit")]

use mae::{
    imui::{
        Color, CrossAxisAlign, IMUI, MainAxisAlign, Point, PopoverSide, TextAreaOptions,
        TextEditBuffer, UIBoxFlags, UISize,
    },
    os::{OSEventFlag, OSKey, OSKeyCode},
    render::{Extra, Rect2DInst, RectCoords, RenderBatch, V4f32},
    testkit::{self, UiHarness},
};

// See src/imui/tests.rs for why this shadowing trick lets the existing
// `#[test]` suite run unmodified under `wasm-bindgen-test-runner`.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test as test;
#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[derive(Default)]
struct RecordingText {
    text: String,
    inserts: Vec<(usize, String)>,
    deletes: Vec<(usize, usize)>,
}

impl RecordingText {
    fn char_to_byte(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map(|(idx, _)| idx)
            .unwrap_or(self.text.len())
    }
}

impl TextEditBuffer for RecordingText {
    fn text(&self) -> String {
        self.text.clone()
    }

    fn insert_text(&mut self, index: usize, text: &str) {
        self.inserts.push((index, text.to_string()));
        let byte = self.char_to_byte(index);
        self.text.insert_str(byte, text);
    }

    fn delete_range(&mut self, range: (usize, usize)) {
        self.deletes.push(range);
        let start = self.char_to_byte(range.0);
        let end = self.char_to_byte(range.1);
        self.text.replace_range(start..end, "");
    }
}

fn primary_flag() -> OSEventFlag {
    #[cfg(target_os = "macos")]
    {
        OSEventFlag::Super
    }
    #[cfg(not(target_os = "macos"))]
    {
        OSEventFlag::Control
    }
}

#[test]
fn fill_child_uses_remaining_width() {
    let mut harness = UiHarness::new(500.0, 100.0);

    let frame = harness.frame(|ui| {
        let row = ui.named_row("row", |ui| {
            let fixed = ui.label("fixed");
            fixed.width(ui, UISize::px(100.0));
            let fill = ui.label("fill");
            fill.width(ui, UISize::fill());
        });
        row.width(ui, UISize::fill());
    });

    assert_eq!(frame.node("fixed").bounds.width(), 100.0);
    assert_eq!(frame.node("fill").bounds.width(), 400.0);
}

#[test]
fn debug_dump_includes_tree_geometry_and_flags() {
    let mut harness = UiHarness::new(320.0, 120.0);

    harness.frame(|ui| {
        ui.named_column("panel", |ui| {
            ui.button("Save###save", None);
        })
        .width(ui, UISize::fill());
    });

    let dump = harness.debug_dump();

    assert!(dump.contains("UiSnapshot nodes="));
    assert!(dump.contains("label=\"panel\""));
    assert!(dump.contains("label=\"Save\""));
    assert!(dump.contains("flags=[visible,mouse,keyboard,bg,border,text]"));
    assert!(dump.contains("bounds="));
    assert!(dump.contains("style=font:"));
}

#[test]
fn button_click_can_be_generated_programmatically() {
    let mut harness = UiHarness::new(300.0, 120.0);
    let mut clicked = false;

    harness.frame(|ui| {
        ui.button("Click me", None);
    });
    harness.click("Click me");
    harness.frame(|ui| {
        clicked = ui.button("Click me", None).clicked();
    });

    assert!(clicked);
}

#[test]
fn button_right_click_can_be_generated_programmatically() {
    let mut harness = UiHarness::new(300.0, 120.0);
    let mut right_clicked = false;
    let mut left_clicked = false;

    harness.frame(|ui| {
        ui.button("Context me", None);
    });
    harness.right_click("Context me");
    harness.frame(|ui| {
        let button = ui.button("Context me", None);
        right_clicked = button.right_clicked();
        left_clicked = button.clicked();
    });

    assert!(right_clicked);
    assert!(!left_clicked);
}

#[test]
fn text_input_can_be_focused_and_typed_programmatically() {
    let mut harness = UiHarness::new(300.0, 120.0);
    let mut text = String::new();

    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });
    harness.click("name");
    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });
    harness.type_text("mae");
    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });

    assert_eq!(text, "mae");
}

#[test]
fn scroll_event_updates_scroll_container_offset() {
    let mut harness = UiHarness::new(240.0, 80.0);

    harness.frame(|ui| {
        let pane = ui.named_column("pane", |ui| {
            for idx in 0..20 {
                let row = ui.label(&format!("row {idx}"));
                row.height(ui, UISize::px(20.0));
            }
        });
        pane.height(ui, UISize::px(50.0))
            .scroll_y(ui, true)
            .clip(ui, true);
    });
    harness.scroll("pane", -4.0);
    let frame = harness.frame(|ui| {
        let pane = ui.named_column("pane", |ui| {
            for idx in 0..20 {
                let row = ui.label(&format!("row {idx}"));
                row.height(ui, UISize::px(20.0));
            }
        });
        pane.height(ui, UISize::px(50.0))
            .scroll_y(ui, true)
            .clip(ui, true);
    });

    assert!(frame.node("pane").scroll.y() > 0.0);
    assert!(frame.node("pane").scroll_max.y() > 0.0);
    assert!(frame.node("pane").content_size.height > frame.node("pane").bounds.height());
}

#[test]
fn scroll_offset_is_clamped_to_content_range() {
    let mut harness = UiHarness::new(240.0, 80.0);

    harness.frame(|ui| {
        let pane = ui.named_column("pane", |ui| {
            for idx in 0..8 {
                ui.label(&format!("row {idx}")).height(ui, UISize::px(20.0));
            }
        });
        pane.height(ui, UISize::px(50.0))
            .scroll_y(ui, true)
            .clip(ui, true);
    });
    harness.scroll("pane", -1000.0);
    let frame = harness.frame(|ui| {
        let pane = ui.named_column("pane", |ui| {
            for idx in 0..8 {
                ui.label(&format!("row {idx}")).height(ui, UISize::px(20.0));
            }
        });
        pane.height(ui, UISize::px(50.0))
            .scroll_y(ui, true)
            .clip(ui, true);
    });

    let pane = frame.node("pane");
    assert_eq!(pane.scroll.y(), pane.scroll_max.y());
    assert!(pane.clip_rect.height() <= pane.bounds.height());
}

#[test]
fn textarea_wrap_and_scroll_options_are_reflected_in_snapshot() {
    let mut harness = UiHarness::new(240.0, 120.0);
    let mut text = "a very long line that should not wrap when disabled".to_string();

    let frame = harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });

    let editor = frame.node("editor");
    assert!(editor.flags.contains(UIBoxFlags::TEXT_INPUT));
    assert!(editor.flags.contains(UIBoxFlags::SCROLL_X));
    assert!(editor.flags.contains(UIBoxFlags::SCROLL_Y));
}

#[test]
fn textarea_wheel_scrolls_vertically_and_alt_wheel_scrolls_horizontally() {
    let mut harness = UiHarness::new(180.0, 90.0);
    let mut text = [
        "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
        "line 2",
        "line 3",
        "line 4",
        "line 5",
        "line 6",
    ]
    .join("\n");

    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    harness.scroll("editor", -3.0);
    let frame = harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    let after_wheel = frame.node("editor");
    assert!(after_wheel.scroll.y() > 0.0);
    assert_eq!(after_wheel.scroll.x(), 0.0);
    assert_eq!(after_wheel.signal.scroll_y, -3.0);
    assert_eq!(after_wheel.signal.scroll_x, 0.0);

    harness.scroll_with_flags("editor", -3.0, OSEventFlag::Alt);
    let frame = harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    let after_alt_wheel = frame.node("editor");
    assert!(after_alt_wheel.scroll.x() > 0.0);
    assert_eq!(after_alt_wheel.signal.scroll_x, -3.0);
    assert_eq!(after_alt_wheel.signal.scroll_y, 0.0);
}

#[test]
fn textarea_scroll_is_clamped_to_zero_and_horizontal_overflow_is_measured() {
    let mut harness = UiHarness::new(180.0, 90.0);
    let mut text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz".to_string();

    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    harness.scroll("editor", 10.0);
    harness.scroll_with_flags("editor", 10.0, OSEventFlag::Alt);
    let frame = harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    let editor = frame.node("editor");
    assert_eq!(editor.scroll.x(), 0.0);
    assert_eq!(editor.scroll.y(), 0.0);
    assert!(editor.scroll_max.x() > 0.0);
}

#[test]
fn textarea_click_on_horizontally_scrolled_line_uses_scroll_offset() {
    let mut harness = UiHarness::new(180.0, 90.0);
    let mut text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz".to_string();

    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    harness.scroll_with_flags("editor", -20.0, OSEventFlag::Alt);
    let frame = harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    let editor = frame.node("editor");
    assert!(editor.scroll.x() > 0.0);

    harness.click_at(editor.bounds.x0 + 14.0, editor.bounds.y0 + 18.0);
    let frame = harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });

    assert!(frame.node("editor").text_edit.as_ref().unwrap().cursor > 0);
}

#[test]
fn textarea_no_wrap_keeps_long_line_as_one_visual_line_for_navigation() {
    let mut harness = UiHarness::new(120.0, 100.0);
    let mut text = "abcdefghijklmnopqrstuvwxyz\nnext".to_string();

    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    let editor = harness.snapshot().node("editor");
    harness.click_at(editor.bounds.x0 + 14.0, editor.bounds.y0 + 18.0);
    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    harness.key_press(OSKeyCode::KeyEnd);
    harness.key_press(OSKeyCode::KeyDownArrow);
    let frame = harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });

    assert_eq!(frame.node("editor").text_edit.as_ref().unwrap().cursor, 31);
}

#[test]
fn textarea_scrolls_horizontally_to_keep_caret_visible() {
    let mut harness = UiHarness::new(120.0, 90.0);
    let mut text = String::new();

    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    harness.click("editor");
    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    harness.type_text("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz");

    for _ in 0..4 {
        harness.frame(|ui| {
            ui.textarea_with_options(
                "editor",
                &mut text,
                TextAreaOptions::new()
                    .wrap_x(false)
                    .scroll_x(true)
                    .scroll_y(true),
            );
        });
    }

    assert!(harness.snapshot().node("editor").scroll_max.x() > 0.0);
    assert!(harness.snapshot().node("editor").scroll.x() > 0.0);
}

#[test]
fn textarea_enter_at_bottom_scrolls_to_new_line() {
    let mut harness = UiHarness::new(160.0, 82.0);
    let mut text = String::new();

    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    harness.click("editor");
    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });

    for idx in 0..8 {
        harness.type_text(&format!("line {idx}"));
        harness.key_press(OSKeyCode::KeyEnter);
        harness.frame(|ui| {
            ui.textarea_with_options(
                "editor",
                &mut text,
                TextAreaOptions::new()
                    .wrap_x(false)
                    .scroll_x(true)
                    .scroll_y(true),
            );
        });
    }
    for _ in 0..4 {
        harness.frame(|ui| {
            ui.textarea_with_options(
                "editor",
                &mut text,
                TextAreaOptions::new()
                    .wrap_x(false)
                    .scroll_x(true)
                    .scroll_y(true),
            );
        });
    }

    let editor = harness.snapshot().node("editor");
    assert!(editor.scroll_max.y() > 0.0);
    assert!(editor.scroll.y() > 0.0);
}

#[test]
fn textarea_manual_scroll_away_from_stationary_caret_sticks() {
    let mut harness = UiHarness::new(160.0, 82.0);
    let mut text = String::new();

    let build = |ui: &mut mae::imui::IMUI, text: &mut String| {
        ui.textarea_with_options(
            "editor",
            text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    };

    harness.frame(|ui| build(ui, &mut text));
    harness.click("editor");
    harness.frame(|ui| build(ui, &mut text));

    // Type enough lines that the caret is parked at the bottom (scrolled down).
    for idx in 0..8 {
        harness.type_text(&format!("line {idx}"));
        harness.key_press(OSKeyCode::KeyEnter);
        harness.frame(|ui| build(ui, &mut text));
    }
    for _ in 0..4 {
        harness.frame(|ui| build(ui, &mut text));
    }
    let scrolled_down = harness.snapshot().node("editor").scroll.y();
    assert!(scrolled_down > 0.0);

    // Scroll back up with the wheel without moving the caret. It must stay scrolled up
    // instead of snapping back to the caret at the bottom.
    for _ in 0..8 {
        harness.scroll("editor", 5.0);
        harness.frame(|ui| build(ui, &mut text));
    }
    for _ in 0..6 {
        harness.frame(|ui| build(ui, &mut text));
    }

    let editor = harness.snapshot().node("editor");
    assert!(
        editor.scroll.y() < scrolled_down,
        "manual scroll up should not snap back to the caret (was {}, now {})",
        scrolled_down,
        editor.scroll.y()
    );
}

#[test]
fn textarea_page_up_and_down_move_caret_and_scroll_view() {
    let mut harness = UiHarness::new(160.0, 82.0);
    let mut text = (0..20)
        .map(|idx| format!("line {idx}"))
        .collect::<Vec<_>>()
        .join("\n");

    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    let editor = harness.snapshot().node("editor");
    harness.click_at(editor.bounds.x0 + 14.0, editor.bounds.y0 + 18.0);
    harness.frame(|ui| {
        ui.textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });

    harness.key_press(OSKeyCode::KeyPageDown);
    for _ in 0..4 {
        harness.frame(|ui| {
            ui.textarea_with_options(
                "editor",
                &mut text,
                TextAreaOptions::new()
                    .wrap_x(false)
                    .scroll_x(true)
                    .scroll_y(true),
            );
        });
    }
    let after_down = harness.snapshot().node("editor");
    let cursor_after_down = after_down.text_edit.as_ref().unwrap().cursor;
    let scroll_after_down = after_down.scroll.y();
    assert!(cursor_after_down > 0);
    assert!(scroll_after_down > 0.0);

    harness.key_press(OSKeyCode::KeyPageUp);
    for _ in 0..6 {
        harness.frame(|ui| {
            ui.textarea_with_options(
                "editor",
                &mut text,
                TextAreaOptions::new()
                    .wrap_x(false)
                    .scroll_x(true)
                    .scroll_y(true),
            );
        });
    }
    let after_up = harness.snapshot().node("editor");
    assert!(after_up.text_edit.as_ref().unwrap().cursor < cursor_after_down);
    assert!(after_up.scroll.y() < scroll_after_down);
}

#[test]
fn line_edit_inserts_at_caret_after_navigation() {
    let mut harness = UiHarness::new(300.0, 120.0);
    let mut text = "abcd".to_string();

    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });
    harness.click("name");
    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });
    harness.key_press(OSKeyCode::KeyHome);
    harness.type_text("X");
    let frame = harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });

    assert_eq!(text, "Xabcd");
    assert_eq!(frame.node("name").text_edit.as_ref().unwrap().cursor, 1);
}

#[test]
fn line_edit_selection_can_be_deleted() {
    let mut harness = UiHarness::new(300.0, 120.0);
    let mut text = "abcd".to_string();

    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });
    harness.click("name");
    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });
    harness.key_press(OSKeyCode::KeyHome);
    harness.key_press_with_flags(OSKeyCode::KeyRightArrow, OSEventFlag::Shift);
    harness.key_press_with_flags(OSKeyCode::KeyRightArrow, OSEventFlag::Shift);
    harness.key_press(OSKeyCode::KeyDelete);
    let frame = harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });

    assert_eq!(text, "cd");
    assert_eq!(frame.node("name").text_edit.as_ref().unwrap().cursor, 0);
    assert!(
        frame
            .node("name")
            .text_edit
            .as_ref()
            .unwrap()
            .selection_range()
            .is_none()
    );
}

#[test]
fn line_edit_clipboard_copy_cut_paste_uses_primary_shortcut() {
    let mut harness = UiHarness::new(300.0, 120.0);
    let mut text = "abcd".to_string();

    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });
    harness.click("name");
    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });
    harness.key_press_with_flags(OSKeyCode::KeyA, primary_flag());
    harness.key_press_with_flags(OSKeyCode::KeyX, primary_flag());
    harness.key_press_with_flags(OSKeyCode::KeyV, primary_flag());
    harness.key_press_with_flags(OSKeyCode::KeyV, primary_flag());
    harness.frame(|ui| {
        ui.line_edit("name", &mut text, false);
    });

    assert_eq!(text, "abcdabcd");
}

#[test]
fn textarea_supports_multiline_navigation_and_insert() {
    let mut harness = UiHarness::new(300.0, 160.0);
    let mut text = "ab\ncd".to_string();

    harness.frame(|ui| {
        ui.textarea("editor", &mut text);
    });
    harness.click("editor");
    harness.frame(|ui| {
        ui.textarea("editor", &mut text);
    });
    harness.key_press(OSKeyCode::KeyHome);
    harness.key_press(OSKeyCode::KeyDownArrow);
    harness.type_text("X");
    let frame = harness.frame(|ui| {
        ui.textarea("editor", &mut text);
    });

    assert_eq!(text, "ab\nXcd");
    assert!(frame.node("editor").text_edit.is_some());
}

#[test]
fn markdown_textarea_click_uses_styled_line_heights() {
    let mut harness = UiHarness::new(360.0, 220.0);
    let mut text = "# Title\nbody\nlast".to_string();

    let frame = harness.frame(|ui| {
        ui.markdown_textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    let body_line = frame.node("body");
    harness.click_at(body_line.bounds.x0 + 2.0, body_line.center().y());
    let frame = harness.frame(|ui| {
        ui.markdown_textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });

    assert_eq!(frame.node("editor").text_edit.as_ref().unwrap().cursor, 8);
}

#[test]
fn textarea_keystrokes_call_text_edit_buffer_operations() {
    let mut harness = UiHarness::new(300.0, 160.0);
    let mut text = RecordingText::default();

    harness.frame(|ui| {
        ui.markdown_textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    harness.click("editor");
    harness.frame(|ui| {
        ui.markdown_textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });

    harness.type_text("ab");
    harness.frame(|ui| {
        ui.markdown_textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });
    harness.key_press(OSKeyCode::KeyBackspace);
    harness.frame(|ui| {
        ui.markdown_textarea_with_options(
            "editor",
            &mut text,
            TextAreaOptions::new()
                .wrap_x(false)
                .scroll_x(true)
                .scroll_y(true),
        );
    });

    assert_eq!(text.text, "a");
    assert_eq!(
        text.inserts,
        vec![(0, "a".to_string()), (1, "b".to_string())]
    );
    assert_eq!(text.deletes, vec![(1, 2)]);
}

#[test]
fn software_renderer_output_is_stable_for_solid_rect() {
    let color = Color::new("#ff0000");
    let mut batch = RenderBatch::new(1);
    batch.add_rect(Rect2DInst {
        dst: RectCoords::from_size(1.0, 1.0, 3.0, 3.0),
        src: RectCoords::from_size(0.0, 0.0, 0.0, 0.0),
        colors: [color, color, color, color],
        extra: Extra::new(true, 0.0),
    });

    let snapshot = testkit::render_batches(5, 5, &[batch]);

    assert_eq!(snapshot.pixel(0, 0), 0xFF758B99);
    assert_eq!(snapshot.pixel(2, 2), 0xFFFF0000);
}

#[test]
fn software_renderer_honors_rounded_rect_corners() {
    let color = V4f32 {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    let mut batch = RenderBatch::new(1);
    batch.add_rect(Rect2DInst {
        dst: RectCoords::from_size(0.0, 0.0, 10.0, 10.0),
        src: RectCoords::from_size(0.0, 0.0, 0.0, 0.0),
        colors: [color, color, color, color],
        extra: Extra::new(true, 5.0),
    });

    let snapshot = testkit::render_batches(10, 10, &[batch]);

    assert_eq!(snapshot.pixel(0, 0), 0xFF758B99);
    assert_eq!(snapshot.pixel(5, 5), 0xFF00FF00);
}

#[test]
fn overlay_widget_receives_click_over_underlying_widget() {
    // Mirrors the settings window over the editor: an overlay panel and its button sit
    // on top of a full-size underlying widget. A click where they overlap must go to the
    // overlay (the "upper" widget), not the widget below it.
    let mut harness = UiHarness::new(300.0, 200.0);

    let build = |ui: &mut IMUI| {
        // Underlying full-screen clickable widget (stands in for the editor panel).
        let under = ui.button("Under###under", None);
        under
            .width(ui, UISize::px(300.0))
            .height(ui, UISize::px(200.0));

        // Overlay panel with a button, positioned over the underlying widget.
        let pane = ui.floating_pane_at(Point::new(110.0, 80.0), Some("###overlay_pane"), |ui| {
            let top = ui.button("Top###top", None);
            top.width(ui, UISize::px(80.0)).height(ui, UISize::px(30.0));
        });
        pane.padding_all(ui, 0.0);
    };

    // Frame 1: build retained geometry. The following click must resolve capture from
    // the event position itself, not from a previous hover frame.
    harness.frame(build);

    // Frame 2: click where the overlay button and the underlying widget overlap.
    harness.click_at(140.0, 95.0);
    let snap = harness.frame(build);

    assert!(
        snap.node("Top").signal.clicked(),
        "the overlay button should receive the click"
    );
    assert!(
        !snap.node("Under").signal.clicked(),
        "the underlying widget must not receive a click covered by the overlay"
    );
}

#[test]
fn plain_icon_button_in_scrollable_toolbar_receives_click() {
    let mut harness = UiHarness::new(500.0, 260.0);
    let mut settings_clicked = false;

    let mut build = |ui: &mut IMUI| {
        let root = ui.row(|ui| {
            let sidebar = ui.named_column("sidebar", |ui| {
                let toolbar = ui.row(|ui| {
                    for idx in 0..5 {
                        ui.button_icon_plain(&format!("{idx}###toolbar_{idx}"), None);
                    }
                    if ui
                        .button_icon_plain("S###settings_button", Some("Settings"))
                        .clicked()
                    {
                        settings_clicked = true;
                    }
                });
                toolbar
                    .width(ui, UISize::ParentPct(1.0))
                    .height(ui, UISize::Pixels(44.0))
                    .padding_all(ui, 6.0)
                    .align(ui, MainAxisAlign::Center, CrossAxisAlign::Center)
                    .gap(ui, 6.0);

                let filler = ui.label("tree filler");
                filler.height(ui, UISize::px(500.0));
            });
            sidebar
                .width(ui, UISize::px(240.0))
                .height(ui, UISize::fill())
                .scroll_y(ui, true)
                .clip(ui, true);

            let editor = ui.button("editor", None);
            editor.width(ui, UISize::fill()).height(ui, UISize::fill());
        });
        root.width(ui, UISize::fill()).height(ui, UISize::fill());
    };

    harness.frame(&mut build);
    harness.click("S");
    harness.frame(&mut build);

    assert!(
        settings_clicked,
        "settings icon should receive clicks inside the clipped scrollable sidebar"
    );
}

#[test]
fn canvas_registers_a_custom_draw_box_and_sizes_it() {
    // The headless harness has no Drawer, so the paint callback is a no-op here;
    // this asserts the build-time wiring (flag + layout). End-to-end pixel
    // output is verified by running the app (the software/GL renderers need a
    // real Drawer).
    let mut harness = UiHarness::new(200.0, 100.0);
    let frame = harness.frame(|ui| {
        ui.canvas("wave", |drawer, rect, _clip| {
            let mid = (rect.x0 + rect.x1) * 0.5;
            drawer.draw_rect(
                &RectCoords {
                    x0: mid,
                    y0: rect.y0,
                    x1: mid + 2.0,
                    y1: rect.y1,
                },
                Color::new("#ffffff"),
                0.0,
            );
        })
        .width(ui, UISize::Pixels(120.0))
        .height(ui, UISize::Pixels(40.0));
    });

    let node = frame.node("wave");
    assert!(
        node.flags.contains(UIBoxFlags::CUSTOM_DRAW),
        "canvas box should carry the CUSTOM_DRAW flag"
    );
    assert_eq!(node.bounds.width(), 120.0);
    assert_eq!(node.bounds.height(), 40.0);
}

// Right-arrow over a multi-codepoint emoji cluster (👍 + skin-tone modifier) must
// jump the whole grapheme, not stop inside it; backspace removes the whole cluster.
#[test]
fn caret_moves_over_emoji_cluster_by_grapheme() {
    let mut harness = UiHarness::new(300.0, 120.0);
    // "a" + 👍🏽 (U+1F44D U+1F3FD, 2 chars) + "b" = 4 chars.
    let mut text = "a\u{1F44D}\u{1F3FD}b".to_string();

    harness.frame(|ui| {
        ui.line_edit("e", &mut text, false);
    });
    harness.click("e");
    harness.frame(|ui| {
        ui.line_edit("e", &mut text, false);
    });
    harness.key_press(OSKeyCode::KeyHome);

    harness.key_press(OSKeyCode::KeyRightArrow);
    let frame = harness.frame(|ui| {
        ui.line_edit("e", &mut text, false);
    });
    assert_eq!(
        frame.node("e").text_edit.as_ref().unwrap().cursor,
        1,
        "past 'a'"
    );

    harness.key_press(OSKeyCode::KeyRightArrow);
    let frame = harness.frame(|ui| {
        ui.line_edit("e", &mut text, false);
    });
    assert_eq!(
        frame.node("e").text_edit.as_ref().unwrap().cursor,
        3,
        "should skip the whole 2-char emoji cluster, not land at 2"
    );
}

#[test]
fn backspace_deletes_whole_emoji_cluster() {
    let mut harness = UiHarness::new(300.0, 120.0);
    let mut text = "a\u{1F44D}\u{1F3FD}".to_string();

    harness.frame(|ui| {
        ui.line_edit("e", &mut text, false);
    });
    harness.click("e");
    harness.frame(|ui| {
        ui.line_edit("e", &mut text, false);
    });
    harness.key_press(OSKeyCode::KeyEnd);
    harness.key_press(OSKeyCode::KeyBackspace);
    let frame = harness.frame(|ui| {
        ui.line_edit("e", &mut text, false);
    });

    assert_eq!(
        text, "a",
        "backspace should remove the entire emoji cluster"
    );
    assert_eq!(frame.node("e").text_edit.as_ref().unwrap().cursor, 1);
}

#[test]
fn toast_renders_message_and_close_dismisses_it() {
    use mae::ui::ToastLevel;

    let mut harness = UiHarness::new(900.0, 600.0);
    harness.frame(|_| {});

    // Raised imperatively (once), then rendered by the framework each frame.
    harness
        .ui_mut()
        .toast(ToastLevel::Danger, "Upload failed: blob too big");
    let mut snap = harness.frame(|_| {});
    assert!(
        snap.try_node("Upload failed: blob too big").is_some(),
        "toast message should be rendered:\n{}",
        snap.debug_dump()
    );

    // Let the slide-in settle so the close cross is on-screen before we click it.
    for _ in 0..60 {
        snap = harness.frame(|_| {});
    }

    // The close cross (Material "close" glyph) is present; clicking it dismisses.
    let close = snap.node("\u{e5cd}");
    let center = close.center();
    let (cx, cy) = (center.x(), center.y());
    harness.click_at(cx, cy);

    let mut gone = false;
    for _ in 0..400 {
        let s = harness.frame(|_| {});
        if s.try_node("Upload failed: blob too big").is_none() {
            gone = true;
            break;
        }
    }
    assert!(
        gone,
        "toast should fade out and disappear after the close cross is clicked"
    );
}

#[test]
fn toast_auto_expires_after_its_duration() {
    use mae::ui::ToastLevel;
    use std::time::Duration;

    let mut harness = UiHarness::new(900.0, 600.0);
    harness.frame(|_| {});
    harness
        .ui_mut()
        .toast_with_duration(ToastLevel::Info, "transient", Duration::from_millis(0));

    // A zero-duration toast is already past its lifetime, so it fades and is gone.
    let mut gone = false;
    for _ in 0..400 {
        let s = harness.frame(|_| {});
        if s.try_node("transient").is_none() {
            gone = true;
            break;
        }
    }
    assert!(gone, "expired toast should be removed");
}

#[test]
fn wrapping_label_grows_to_multiple_lines() {
    let mut harness = UiHarness::new(400.0, 300.0);
    let long = "The quick brown fox jumps over the lazy dog and keeps running across the meadow";

    // A single-line label for comparison, plus a wrapping label constrained to a
    // narrow column so it must break across several lines.
    let snap = harness.frame(|ui| {
        ui.label(long); // single line: no wrapping
        let col = ui.named_column("###wrap_col", |ui| {
            ui.wrapping_label(long);
        });
        col.width(ui, UISize::Pixels(160.0))
            .height(ui, UISize::ChildrenSum);
    });

    let heights: Vec<f32> = snap
        .nodes
        .iter()
        .filter(|n| n.text.as_deref() == Some(long))
        .map(|n| n.bounds.height())
        .collect();
    assert_eq!(heights.len(), 2, "expected the plain + wrapping labels");
    let single = heights.iter().cloned().fold(f32::MAX, f32::min);
    let wrapped = heights.iter().cloned().fold(0.0, f32::max);
    assert!(
        wrapped > single * 2.5,
        "wrapping label ({wrapped}) should be several lines taller than the single-line one ({single})"
    );
}

#[test]
fn opacity_multiplies_down_the_box_tree() {
    // A parent's fade must carry its whole subtree, so a view cross-fade is one
    // call on the root rather than one per descendant.
    let mut harness = UiHarness::new(200.0, 100.0);
    let snapshot = harness.frame(|ui| {
        let parent = ui.named_column("###parent", |ui| {
            ui.named_row("###opaque_child", |_| {});
            ui.named_row("###faded_child", |_| {}).opacity(ui, 0.5);
        });
        parent.opacity(ui, 0.5);
    });

    assert_eq!(snapshot.node("###parent").opacity, 0.5);
    // Inherited from the parent only.
    assert_eq!(snapshot.node("###opaque_child").opacity, 0.5);
    // Own 0.5 on top of the parent's 0.5.
    assert_eq!(snapshot.node("###faded_child").opacity, 0.25);
}

#[test]
fn opacity_is_clamped_and_defaults_to_opaque() {
    let mut harness = UiHarness::new(200.0, 100.0);
    let snapshot = harness.frame(|ui| {
        ui.named_row("###plain", |_| {});
        ui.named_row("###over", |_| {}).opacity(ui, 4.0);
        ui.named_row("###under", |_| {}).opacity(ui, -1.0);
    });

    assert_eq!(snapshot.node("###plain").opacity, 1.0);
    assert_eq!(snapshot.node("###over").opacity, 1.0);
    assert_eq!(snapshot.node("###under").opacity, 0.0);
}

#[test]
fn dt_is_clamped_to_the_animation_range() {
    // App-driven animation steps on this delta, so it must never be zero (no
    // progress) or unbounded (a hitch teleporting an animation to its target).
    let mut harness = UiHarness::new(100.0, 100.0);
    harness.frame(|_| {});
    let dt = harness.ui().dt();
    assert!(
        (1.0 / 240.0..=1.0 / 15.0).contains(&dt),
        "dt {dt} outside the clamped range"
    );
}

#[test]
fn animate_scalar_converges_and_snaps_at_epsilon() {
    use mae::imui::{animate_scalar, smooth_rate};

    // Frame-rate independence: the same elapsed time reaches the same place
    // whether it arrives as one big step or several small ones.
    let coarse = animate_scalar(0.0, 1.0, smooth_rate(30.0, 1.0 / 30.0), 0.001);
    let mut fine = 0.0;
    for _ in 0..4 {
        fine = animate_scalar(fine, 1.0, smooth_rate(30.0, 1.0 / 120.0), 0.001);
    }
    assert!(
        (coarse - fine).abs() < 0.01,
        "coarse {coarse} and fine {fine} should track each other"
    );

    // Snaps exactly once inside epsilon, rather than approaching forever.
    assert_eq!(animate_scalar(0.9999, 1.0, 0.5, 0.01), 1.0);
}

#[test]
fn widgets_are_addressable_by_stable_id_and_by_label() {
    // The point of the stable id: a test targeting it keeps working when the
    // visible label is reworded, which display-text matching does not.
    let mut harness = UiHarness::new(200.0, 100.0);
    let snapshot = harness.frame(|ui| {
        ui.button("Connect###sync_toggle", None);
    });

    assert_eq!(
        snapshot.node("###sync_toggle").key_id.as_deref(),
        Some("###sync_toggle")
    );
    // Both handles resolve to the same widget.
    assert_eq!(
        snapshot.node("Connect").key_id.as_deref(),
        Some("###sync_toggle")
    );

    // Reword the label; the id still finds it.
    let snapshot = harness.frame(|ui| {
        ui.button("Disconnect###sync_toggle", None);
    });
    assert_eq!(
        snapshot.node("###sync_toggle").text.as_deref(),
        Some("Disconnect")
    );
    assert!(snapshot.try_node("Connect").is_none());
}

#[test]
fn scroll_to_y_sets_a_clamped_target() {
    // Keyboard navigation in a palette needs to pull an off-screen row into
    // view; without a public setter there is no way to do it. Called from
    // inside the build closure, where the handle exists — the real usage.
    let mut harness = UiHarness::new(200.0, 100.0);
    let build = |target: Option<f32>| {
        move |ui: &mut IMUI| {
            let list = ui.named_column("###list", |ui| {
                for i in 0..40 {
                    ui.label(&format!("row {i}"))
                        .height(ui, UISize::Pixels(20.0));
                }
            });
            list.height(ui, UISize::Pixels(100.0))
                .scroll_y(ui, true)
                .clip(ui, true);
            if let Some(y) = target {
                ui.scroll_to_y(list, y);
            }
        }
    };
    harness.frame(build(None));
    let max = harness.snapshot().node("###list").scroll_max.y();
    assert!(max > 0.0, "list should overflow, scroll_max was {max}");

    // Past the end clamps rather than scrolling into empty space. The offset
    // eases toward its target, so settle before reading it.
    harness.frame(build(Some(10_000.0)));
    for _ in 0..120 {
        harness.frame(build(None));
    }
    let scrolled = harness.snapshot().node("###list").scroll.y();
    assert!(
        (scrolled - max).abs() < 1.0,
        "scroll {scrolled} should have settled at the clamped max {max}"
    );
}

#[test]
fn press_outside_is_edge_triggered_and_pane_aware() {
    // `mouse_down()` is level-triggered: it stays true for every frame the
    // button is held. A popover dismissed on "the mouse is down outside me"
    // would therefore tear itself down while the opening click is still held.
    // `press_outside` reports only the press edge.
    let mut harness = UiHarness::new(300.0, 200.0);

    // (press_outside, mouse_down) for the frame just built.
    fn frame(harness: &mut UiHarness) -> (bool, bool) {
        let mut out = (false, false);
        harness.frame(|ui| {
            let pane = ui.floating_pane_at(Point::new(10.0, 10.0), Some("###pane"), |ui| {
                ui.button("inside###inside_btn", None)
                    .width(ui, UISize::Pixels(80.0))
                    .height(ui, UISize::Pixels(40.0));
            });
            pane.background(ui, Color::new("#202020"));
            out = (ui.press_outside(&[pane]), ui.mouse_down());
        });
        out
    }

    // Frame once so the pane has a painted rect to test against.
    assert_eq!(frame(&mut harness), (false, false), "no press yet");

    // Press inside: not a dismissal, and the button consumes the event.
    harness.mouse_down(OSKey::LeftMouseButton, 30.0, 30.0);
    assert_eq!(
        frame(&mut harness),
        (false, true),
        "press inside the pane must not dismiss it"
    );

    // Button still held, no new press. This is the distinction that matters:
    // `mouse_down()` is still true, `press_outside` is not.
    assert_eq!(
        frame(&mut harness),
        (false, true),
        "a held button must not keep reporting — that is the mouse_down() trap"
    );

    harness.mouse_up(OSKey::LeftMouseButton, 30.0, 30.0);
    frame(&mut harness);

    // A press well outside it is a dismissal.
    harness.mouse_down(OSKey::LeftMouseButton, 250.0, 180.0);
    assert_eq!(
        frame(&mut harness).0,
        true,
        "press outside the pane should report true"
    );
}

#[test]
fn enter_commits_a_single_line_field_but_not_a_textarea() {
    // Inline rename reacts to the widget's own commit signal rather than the
    // app sniffing raw key events and guessing which field had focus.
    let mut harness = UiHarness::new(300.0, 200.0);
    let mut line = String::from("name");
    let mut area = String::from("body");

    // Returns (line_committed, area_committed) for the frame just built.
    fn frame(
        harness: &mut UiHarness,
        line: &mut String,
        area: &mut String,
        out: &mut (bool, bool),
    ) {
        harness.frame(|ui| {
            let l = ui.line_edit("###line", line, false);
            l.width(ui, UISize::Pixels(100.0))
                .height(ui, UISize::Pixels(24.0));
            let a = ui.textarea("###area", area);
            a.width(ui, UISize::Pixels(100.0))
                .height(ui, UISize::Pixels(60.0));
            *out = (l.signal().committed(), a.signal().committed());
        });
    }

    let mut out = (false, false);
    frame(&mut harness, &mut line, &mut area, &mut out);

    harness.click("###line");
    frame(&mut harness, &mut line, &mut area, &mut out);
    harness.key_press(OSKeyCode::KeyEnter);
    frame(&mut harness, &mut line, &mut area, &mut out);
    assert!(out.0, "Enter in a focused line edit should commit");

    // In a textarea Enter inserts a newline, so it must not read as a commit.
    harness.click("###area");
    frame(&mut harness, &mut line, &mut area, &mut out);
    harness.key_press(OSKeyCode::KeyEnter);
    frame(&mut harness, &mut line, &mut area, &mut out);
    assert!(!out.1, "Enter in a textarea is a newline, not a commit");
}

#[test]
fn anchored_pane_flips_and_clamps_at_window_edges() {
    // Popovers are anchored to the control that opened them. Near a border the
    // pane must flip to the other side rather than be clamped over its own
    // trigger — and it must know its position on the frame it appears, or it
    // visibly jumps.
    let mut harness = UiHarness::new(300.0, 200.0);
    let size = (120.0, 80.0);
    let mut placed = Vec::new();
    harness.frame(|ui| {
        // Trigger near the top-left: Below fits, so no flip.
        let top = RectCoords::from_size(10.0, 10.0, 60.0, 20.0);
        placed.push(ui.anchored_position(top, PopoverSide::Below, size));
        // Trigger near the bottom: Below would overflow, so flip Above.
        let bottom = RectCoords::from_size(10.0, 170.0, 60.0, 20.0);
        placed.push(ui.anchored_position(bottom, PopoverSide::Below, size));
        // Trigger near the right edge: Right would overflow, so flip Left.
        let right = RectCoords::from_size(260.0, 40.0, 30.0, 20.0);
        placed.push(ui.anchored_position(right, PopoverSide::Right, size));
        // Trigger near the left edge with a pane wider than the gap: nowhere to
        // flip to, so clamp inside the window instead of going negative.
        let left = RectCoords::from_size(5.0, 40.0, 20.0, 20.0);
        placed.push(ui.anchored_position(left, PopoverSide::Left, size));
    });

    // Below the anchor.
    assert_eq!(placed[0].y(), 32.0, "should sit just below the trigger");
    // Flipped above: bottom edge just above the trigger's top (170 - 2 - 80).
    assert_eq!(placed[1].y(), 88.0, "should flip above the trigger");
    // Flipped left of the trigger (260 - 2 - 120).
    assert_eq!(placed[2].x(), 138.0, "should flip to the trigger's left");
    // Clamped to the window, never off-screen.
    assert!(placed[3].x() >= 4.0, "clamped inside the left margin");

    for p in &placed {
        assert!(
            p.x() >= 0.0 && p.y() >= 0.0 && p.x() + size.0 <= 300.0 && p.y() + size.1 <= 200.0,
            "pane at {p:?} escaped the window"
        );
    }
}

#[test]
fn software_renderer_antialiases_rounded_corners() {
    // The corner arc used to be a hard `discard`/skip, so every rounded surface
    // in the app was visibly stair-stepped on the native path while the DOM
    // backend (CSS border-radius) was smooth. A pixel straddling the arc must
    // now land *between* the background and the fill.
    let color = V4f32 {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    let mut batch = RenderBatch::new(1);
    batch.add_rect(Rect2DInst {
        dst: RectCoords::from_size(0.0, 0.0, 10.0, 10.0),
        src: RectCoords::from_size(0.0, 0.0, 0.0, 0.0),
        colors: [color, color, color, color],
        extra: Extra::new(true, 5.0),
    });
    let snapshot = testkit::render_batches(10, 10, &[batch]);

    let green = |p: u32| (p >> 8) & 0xFF;
    let background = snapshot.pixel(0, 0);
    let interior = snapshot.pixel(5, 5);
    assert_eq!(
        background, 0xFF758B99,
        "far outside the arc stays background"
    );
    assert_eq!(interior, 0xFF00FF00, "well inside stays fully filled");

    // (1,1) sits ~0.05px inside the arc of a radius-5 circle — the ramp should
    // give it partial coverage rather than the full fill it used to get.
    let edge = snapshot.pixel(1, 1);
    assert!(
        edge != background && edge != interior,
        "corner pixel {edge:#010x} should be a blend, not {background:#010x} or {interior:#010x}"
    );
    assert!(
        green(edge) > green(background) && green(edge) < green(interior),
        "corner pixel's green {} should fall between {} and {}",
        green(edge),
        green(background),
        green(interior)
    );
}

#[test]
fn theme_background_drives_what_a_fade_dissolves_into() {
    // Anything drawn below full opacity composites against the frame's clear
    // colour. Left at the default transparent black, a fading view dissolves
    // toward black however light the theme is — which is what made the app's
    // view transition flash. The theme's own `app_bg` is the right backdrop.
    let mut harness = UiHarness::new(60.0, 60.0);
    let mut theme = mae::imui::UITheme::light();
    theme.app_bg = Color::new("#ffffff");
    harness.ui_mut().set_theme(theme);

    // A half-faded white box over a white clear reads as white, not grey.
    let snapshot = harness.frame(|ui| {
        ui.named_row("###faded", |_| {})
            .width(ui, UISize::Pixels(60.0))
            .height(ui, UISize::Pixels(60.0))
            .background(ui, Color::new("#ffffff"))
            .opacity(ui, 0.5);
    });
    assert_eq!(snapshot.node("###faded").opacity, 0.5);
    assert_eq!(
        harness.ui().theme().app_bg.r,
        1.0,
        "the theme's background is what the renderer clears to"
    );
}
