#![cfg(feature = "testkit")]

use mae::{
    imui::{Color, TextAreaOptions, UISize},
    os::{OSEventFlag, OSKeyCode},
    render::{Extra, Rect2DInst, RectCoords, RenderBatch, V4f32},
    testkit::{self, UiHarness},
};

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
    assert!(editor.text_input);
    assert!(editor.scroll_x);
    assert!(editor.scroll_y);
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
