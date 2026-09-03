use super::*;

// Shadows the built-in `#[test]` attribute macro with `wasm_bindgen_test`'s
// version when targeting wasm32, so every existing `#[test]` fn below runs
// under `wasm-bindgen-test-runner` (in a real browser) with no per-function
// changes. See Cargo.toml's wasm32 dev-dependencies comment.
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test as test;
#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

fn push_test_event(ui: &mut IMUI, ev: OSEvent) {
    ui.apply_event_side_effects(&ev);
    ui.events.push(ev);
}

fn build_vertical_scroll_pane(ui: &mut IMUI) -> UIBoxHandle {
    let pane = ui.named_column("###vertical_scroll_pane", |ui| {
        for idx in 0..10 {
            let label = ui.label(&format!("Row {idx}"));
            ui.height(label, UISize::Pixels(24.0));
        }
    });
    ui.width(pane, UISize::Pixels(120.0));
    ui.height(pane, UISize::Pixels(72.0));
    ui.scroll_y(pane, true);
    pane
}

fn build_horizontal_scroll_pane(ui: &mut IMUI) -> UIBoxHandle {
    let pane = ui.named_row("###horizontal_scroll_pane", |ui| {
        for idx in 0..6 {
            let label = ui.label(&format!("Column {idx}"));
            ui.width(label, UISize::Pixels(72.0));
            ui.height(label, UISize::Pixels(24.0));
        }
    });
    ui.width(pane, UISize::Pixels(140.0));
    ui.height(pane, UISize::Pixels(48.0));
    ui.scroll_x(pane, true);
    pane
}

#[test]
fn built_in_themes_expose_distinct_light_and_dark_tokens() {
    let dark = UITheme::dark();
    let light = UITheme::light();

    assert_eq!(dark.kind, ThemeKind::Dark);
    assert_eq!(light.kind, ThemeKind::Light);
    assert!(color_distance(dark.app_bg, light.app_bg) > 0.2);
    assert!(dark.text.a > 0.0);
    assert!(light.text.a > 0.0);
    assert!(dark.accent.a > 0.0);
    assert!(light.accent.a > 0.0);
    assert_eq!(UITheme::for_kind(ThemeKind::Dark).kind, ThemeKind::Dark);
    assert_eq!(UITheme::for_kind(ThemeKind::Light).kind, ThemeKind::Light);
}

#[test]
fn retained_box_hover_state_animates_toward_signal_target() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let first = ui.button("Hover###hover_button", None);
    ui.width(first, UISize::Pixels(120.0));
    ui.height(first, UISize::Pixels(40.0));
    ui.end_frame();
    assert_eq!(ui.boxes[first.idx()].hot_t, 0.0);

    ui.repaint_requested = false;
    ui.mouse = Some(Point::new(20.0, 20.0));
    ui.begin_frame();
    let second = ui.button("Hover###hover_button", None);
    ui.width(second, UISize::Pixels(120.0));
    ui.height(second, UISize::Pixels(40.0));
    ui.end_frame();

    let hot_t = ui.boxes[second.idx()].hot_t;
    assert!(hot_t > 0.0);
    assert!(hot_t < 1.0);
    assert!(ui.repaint_requested);
}

#[test]
fn plain_icon_button_draws_only_clickable_icon_text() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let icon = ui.button_icon_plain("\u{e89c}###plain_icon", None);
    ui.end_frame();

    let icon_box = &ui.boxes[icon.idx()];
    assert!(icon_box.flags.contains(UIBoxFlags::CLICKABLE));
    assert!(icon_box.flags.contains(UIBoxFlags::DRAW_TEXT));
    assert!(!icon_box.flags.contains(UIBoxFlags::DRAW_BACKGROUND));
    assert!(!icon_box.flags.contains(UIBoxFlags::DRAW_BORDER));
    assert!(icon_box.style.font_icon);
    assert!(color_distance(icon_box.style.text_color, ui.theme.text_muted) < 0.001);
}

#[test]
fn plain_icon_button_highlights_with_text_color_on_hover() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let first = ui.button_icon_plain("\u{e89c}###plain_icon_hover", None);
    ui.end_frame();
    assert!(color_distance(ui.boxes[first.idx()].style.text_color, ui.theme.text_muted) < 0.001);

    ui.mouse = Some(Point::new(8.0, 8.0));
    ui.begin_frame();
    let second = ui.button_icon_plain("\u{e89c}###plain_icon_hover", None);

    assert!(second.hover());
    assert!(
        color_distance(
            ui.boxes[second.idx()].style.text_color,
            ui.theme.accent_hover
        ) < 0.001
    );
    assert!(
        !ui.boxes[second.idx()]
            .flags
            .contains(UIBoxFlags::DRAW_BACKGROUND)
    );
    assert!(
        !ui.boxes[second.idx()]
            .flags
            .contains(UIBoxFlags::DRAW_BORDER)
    );
    ui.end_frame();
}

#[test]
fn plain_icon_button_remains_clickable() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    ui.button_icon_plain("\u{e89c}###plain_icon_click", None);
    ui.end_frame();

    push_test_event(
        &mut ui,
        OSEvent::press(OSKey::LeftMouseButton, Some(Point::new(8.0, 8.0))),
    );
    push_test_event(
        &mut ui,
        OSEvent::release(OSKey::LeftMouseButton, Some(Point::new(8.0, 8.0))),
    );
    ui.begin_frame();
    let clicked = ui.button_icon_plain("\u{e89c}###plain_icon_click", None);

    assert!(clicked.clicked());
    assert!(
        color_distance(
            ui.boxes[clicked.idx()].style.text_color,
            ui.theme.accent_active
        ) < 0.001
    );
    ui.end_frame();
}

#[test]
fn tooltip_near_window_corner_stays_on_screen() {
    let mut ui = IMUI::new_for_test(200.0, 120.0);
    // Hover a full-window button with the pointer near the bottom-right corner, where the
    // default down-right tooltip placement would overflow the window.
    ui.mouse = Some(Point::new(188.0, 104.0));

    let build = |ui: &mut IMUI| {
        let button = ui.button("Hit###hover_btn", Some("Tip"));
        ui.width(button, UISize::Pixels(200.0));
        ui.height(button, UISize::Pixels(120.0));
        button
    };

    // Exactly two frames: hover is discovered on the 2nd frame and the tooltip spawns
    // then. The tooltip is measured synchronously, so it must already be flipped on this
    // very first visible frame — no intermediate frame at the overflowing anchor (which
    // was the flicker this guards against).
    for _ in 0..2 {
        ui.begin_frame();
        build(&mut ui);
        ui.end_frame();
    }

    // The floating tooltip pane carries the "#tooltip" id as its display string.
    let pane_idx = ui
        .boxes
        .iter()
        .position(|b| b.display_string.as_deref() == Some("#tooltip"))
        .expect("tooltip pane should be present while hovering");
    let rect = ui.boxes[pane_idx].rect;

    assert!(rect.width() > 0.0, "tooltip should have been laid out");
    // Fully on-screen: the naive anchor (mouse + 12) would push x0 to 200 and overflow.
    assert!(
        rect.x0 >= 0.0 && rect.x1 <= 200.0,
        "tooltip overflows horizontally: x0={}, x1={}",
        rect.x0,
        rect.x1
    );
    assert!(
        rect.y0 >= 0.0 && rect.y1 <= 120.0,
        "tooltip overflows vertically: y0={}, y1={}",
        rect.y0,
        rect.y1
    );
    // Small tooltip near the corner flips above-left so it does not cover the cursor.
    assert!(
        rect.x1 <= 188.0 && rect.y1 <= 104.0,
        "tooltip should flip clear of the cursor: rect={rect:?}"
    );
}

#[test]
fn newly_hovered_plain_icon_requests_followup_repaint() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    ui.mouse = Some(Point::new(8.0, 8.0));

    // First frame establishes geometry; inline hover is computed against the
    // previous frame, so the hover is only discovered on the second frame.
    ui.begin_frame();
    ui.button_icon_plain("\u{e5cd}###new_plain_icon_hover", Some("Close"));
    ui.end_frame();

    ui.repaint_requested = false;
    ui.begin_frame();
    ui.button_icon_plain("\u{e5cd}###new_plain_icon_hover", Some("Close"));
    ui.end_frame();

    assert!(
        ui.repaint_requested,
        "newly discovered hover must schedule another frame so tooltip creation can run"
    );
}

#[test]
fn consumed_click_requests_followup_repaint() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let first = ui.button("Open###open_button", None);
    ui.width(first, UISize::Pixels(120.0));
    ui.height(first, UISize::Pixels(40.0));
    ui.end_frame();

    ui.repaint_requested = false;
    push_test_event(
        &mut ui,
        OSEvent::press(OSKey::LeftMouseButton, Some(Point::new(20.0, 20.0))),
    );
    push_test_event(
        &mut ui,
        OSEvent::release(OSKey::LeftMouseButton, Some(Point::new(20.0, 20.0))),
    );

    ui.begin_frame();
    let clicked = ui.button("Open###open_button", None);
    ui.width(clicked, UISize::Pixels(120.0));
    ui.height(clicked, UISize::Pixels(40.0));

    assert!(clicked.clicked());
    assert!(
        ui.repaint_requested,
        "consuming input must request another frame so click-driven state changes are drawn"
    );
    ui.end_frame();
}

#[test]
fn setting_same_theme_does_not_request_repaint() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.repaint_requested = false;
    ui.set_theme(UITheme::dark());
    assert!(!ui.repaint_requested);

    ui.set_theme(UITheme::light());
    assert!(ui.repaint_requested);
}

#[test]
fn static_retained_frame_does_not_keep_lazy_rendering_awake() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let first = ui.button("Idle###idle_button", None);
    ui.width(first, UISize::Pixels(120.0));
    ui.height(first, UISize::Pixels(40.0));
    ui.end_frame();

    ui.repaint_requested = false;
    ui.begin_frame();
    let second = ui.button("Idle###idle_button", None);
    ui.width(second, UISize::Pixels(120.0));
    ui.height(second, UISize::Pixels(40.0));
    ui.end_frame();

    assert!(!ui.repaint_requested);
}

#[test]
fn transient_visual_box_does_not_keep_lazy_rendering_awake() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    ui.floating_pane_at(Point::new(20.0, 20.0), None, |ui| {
        let label = ui.label("Transient");
        ui.padding_all(label, 4.0);
    });
    ui.end_frame();

    ui.repaint_requested = false;
    ui.begin_frame();
    let pane = ui.floating_pane_at(Point::new(20.0, 20.0), None, |ui| {
        let label = ui.label("Transient");
        ui.padding_all(label, 4.0);
    });
    ui.end_frame();

    assert!(pane.key().is_zero());
    assert!(ui.free_boxes.contains(&pane.idx()));
    assert!(!ui.repaint_requested);
}

#[test]
fn key_string_display_and_hash_parts_match_rad_style() {
    assert_eq!(display_part_from_key_string("Save##toolbar"), "Save");
    assert_eq!(hash_part_from_key_string("Save##toolbar"), "Save##toolbar");
    assert_eq!(display_part_from_key_string("Save###stable"), "Save");
    assert_eq!(hash_part_from_key_string("Save###stable"), "###stable");
}

#[test]
fn layout_resolves_children_sum_and_parent_pct() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    ui.begin_frame();
    let root_child = ui.column(|ui| {
        let a = ui.label("abc");
        ui.width(a, UISize::Pixels(100.0));
        ui.height(a, UISize::Pixels(20.0));
        let b = ui.label("def");
        ui.width(b, UISize::ParentPct(0.5));
        ui.height(b, UISize::Pixels(20.0));
    });
    ui.width(root_child, UISize::ParentPct(1.0));
    ui.height(root_child, UISize::ParentPct(1.0));
    ui.layout_root(ui.root);
    let children = ui.boxes[root_child.idx].children.clone();
    assert_eq!(ui.boxes[children[0]].computed_size.width, 100.0);
    assert_eq!(ui.boxes[children[1]].computed_size.width, 200.0);
}

#[test]
fn text_content_size_reserves_draw_margin() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    ui.begin_frame();
    let label = ui.label("abc");
    ui.layout_root(ui.root);

    let (text_width, text_height) = ui.text_size(ui.boxes[label.idx()].style.font_size, "abc");
    let margin = ui.boxes[label.idx()].style.margin;

    assert!(ui.boxes[label.idx()].computed_size.width >= text_width + margin * 2.0);
    assert!(ui.boxes[label.idx()].computed_size.height >= text_height + margin * 2.0);
}

#[test]
fn floating_boxes_do_not_affect_parent_flow_size_or_gaps() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let row = ui.row(|ui| {
        let a = ui.label("a");
        ui.width(a, UISize::Pixels(40.0));
        ui.height(a, UISize::Pixels(20.0));

        let floating = ui.floating_pane_at(Point::new(100.0, 100.0), Some("###float"), |ui| {
            let child = ui.label("floating");
            ui.width(child, UISize::Pixels(80.0));
            ui.height(child, UISize::Pixels(20.0));
        });
        ui.width(floating, UISize::Pixels(80.0));
        ui.height(floating, UISize::Pixels(20.0));

        let b = ui.label("b");
        ui.width(b, UISize::Pixels(50.0));
        ui.height(b, UISize::Pixels(20.0));
    });
    ui.gap(row, 10.0);
    ui.layout_root(ui.root);

    assert_eq!(ui.boxes[row.idx()].computed_size.width, 100.0);
}

#[test]
fn fill_and_parent_pct_share_remaining_width_without_overflow() {
    let mut ui = IMUI::new_for_test(1000.0, 300.0);
    ui.begin_frame();
    let row = ui.row(|ui| {
        let left = ui.label("left");
        ui.width(left, UISize::Fill);
        ui.height(left, UISize::Pixels(20.0));

        let right = ui.label("right");
        ui.width(right, UISize::ParentPct(0.34));
        ui.height(right, UISize::Pixels(20.0));
    });
    ui.width(row, UISize::ParentPct(1.0));
    ui.height(row, UISize::Pixels(30.0));
    ui.padding_all(row, 10.0);
    ui.gap(row, 12.0);
    ui.layout_root(ui.root);

    let children = ui.boxes[row.idx()].children.clone();
    let left_w = ui.boxes[children[0]].computed_size.width;
    let right_w = ui.boxes[children[1]].computed_size.width;
    let available =
        ui.boxes[row.idx()].computed_size.width - ui.boxes[row.idx()].padding.horizontal();
    let used = left_w + right_w + ui.boxes[row.idx()].child_gap;

    assert!(
        used <= available + 0.01,
        "used={used} available={available}"
    );
    assert!(left_w > 0.0);
}

#[test]
fn keyed_boxes_are_reused_across_consecutive_frames() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let first = ui.named_column("###stable", |_| {});
    ui.end_frame();

    ui.begin_frame();
    let second = ui.named_column("###stable", |_| {});
    ui.end_frame();

    assert_eq!(first.idx(), second.idx());
    assert_eq!(ui.box_table.get(&first.key()), Some(&first.idx()));
}

#[test]
fn keyed_boxes_are_pruned_when_missing_for_a_frame() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let first = ui.named_column("###stable", |_| {});
    ui.end_frame();
    assert!(ui.box_table.contains_key(&first.key()));

    ui.begin_frame();
    ui.end_frame();

    assert!(!ui.box_table.contains_key(&first.key()));
    assert!(ui.free_boxes.contains(&first.idx()));
}

#[test]
fn retained_button_consumes_press_and_release_events() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let first = ui.button("Click###button", None);
    ui.width(first, UISize::Pixels(120.0));
    ui.height(first, UISize::Pixels(40.0));
    ui.end_frame();

    ui.mouse = Some(Point::new(20.0, 20.0));
    ui.events = vec![
        OSEvent {
            ty: OSEventType::Press,
            key: OSKey::LeftMouseButton,
            pos: Some(Point::new(20.0, 20.0)),
            chars: None,
            deltax: 0.0,
            deltay: 0.0,
            flags: None,
        },
        OSEvent {
            ty: OSEventType::Release,
            key: OSKey::LeftMouseButton,
            pos: Some(Point::new(20.0, 20.0)),
            chars: None,
            deltax: 0.0,
            deltay: 0.0,
            flags: None,
        },
    ];

    ui.begin_frame();
    let second = ui.button("Click###button", None);
    ui.width(second, UISize::Pixels(120.0));
    ui.height(second, UISize::Pixels(40.0));

    assert!(second.clicked());
    assert!(ui.events.is_empty());
}

#[test]
fn borderless_textarea_has_no_border_even_when_focused() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = "hello".to_string();
    let opts = TextAreaOptions::new().border(false);

    ui.begin_frame();
    let editor = ui.textarea_with_options("###chromeless", &mut buffer, opts);
    ui.end_frame();

    // Focus it (as a click would) and rebuild: the focus ring is painted from the
    // DRAW_BORDER flag, so a borderless editor must never carry that flag.
    ui.focus_key = Some(editor.key());
    ui.begin_frame();
    let editor = ui.textarea_with_options("###chromeless", &mut buffer, opts);
    ui.end_frame();

    let flags = ui.boxes[editor.idx()].flags;
    assert!(!flags.contains(UIBoxFlags::DRAW_BORDER));
    // Identity (and thus the caret/selection drawing) must not depend on the border.
    assert!(flags.contains(UIBoxFlags::MULTILINE));
}

#[test]
fn textarea_copy_paste_round_trips_through_clipboard() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = "abc".to_string();

    let build = |ui: &mut IMUI, buffer: &mut String| {
        let edit = ui.textarea("Text###text", buffer);
        ui.width(edit, UISize::Pixels(240.0));
        ui.height(edit, UISize::Pixels(80.0));
        edit
    };

    ui.begin_frame();
    let edit = build(&mut ui, &mut buffer);
    ui.end_frame();

    ui.focus_key = Some(edit.key());

    // The "primary" modifier is Cmd on macOS, Ctrl elsewhere (see `primary_modifier`).
    #[cfg(target_os = "macos")]
    let primary = OSEventFlag::Super;
    #[cfg(not(target_os = "macos"))]
    let primary = OSEventFlag::Control;

    // Select all, copy, collapse the selection to the end, then paste -> text duplicated.
    ui.events = vec![
        OSEvent::press_with_flags(OSKey::Keyboard(OSKeyCode::KeyA), None, Some(primary)),
        OSEvent::press_with_flags(OSKey::Keyboard(OSKeyCode::KeyC), None, Some(primary)),
        OSEvent::press(OSKey::Keyboard(OSKeyCode::KeyRightArrow), None),
        OSEvent::press_with_flags(OSKey::Keyboard(OSKeyCode::KeyV), None, Some(primary)),
    ];

    ui.begin_frame();
    build(&mut ui, &mut buffer);
    ui.end_frame();

    assert_eq!(buffer, "abcabc");
    // Headless test run has no drawer, so the copy lands in the in-app fallback.
    assert_eq!(ui.clipboard, "abc");
}

/// The "primary" modifier (Cmd on macOS, Ctrl elsewhere), and the redo chord.
#[cfg(target_os = "macos")]
const PRIMARY: OSEventFlag = OSEventFlag::Super;
#[cfg(not(target_os = "macos"))]
const PRIMARY: OSEventFlag = OSEventFlag::Control;
#[cfg(target_os = "macos")]
const PRIMARY_SHIFT: OSEventFlag = OSEventFlag::ShiftSuper;
#[cfg(not(target_os = "macos"))]
const PRIMARY_SHIFT: OSEventFlag = OSEventFlag::ControlShift;

fn drive_textarea(ui: &mut IMUI, buffer: &mut String, edit: UIBoxHandle, events: Vec<OSEvent>) {
    ui.focus_key = Some(edit.key());
    ui.events = events;
    ui.begin_frame();
    let e = ui.textarea("Text###text", buffer);
    ui.width(e, UISize::Pixels(240.0));
    ui.height(e, UISize::Pixels(80.0));
    ui.end_frame();
}

#[test]
fn textarea_undo_redo_restores_typed_run() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = String::new();

    ui.begin_frame();
    let edit = ui.textarea("Text###text", &mut buffer);
    ui.width(edit, UISize::Pixels(240.0));
    ui.height(edit, UISize::Pixels(80.0));
    ui.end_frame();

    // A run of typing with no cursor move coalesces into one undo step.
    drive_textarea(
        &mut ui,
        &mut buffer,
        edit,
        vec![OSEvent::text('h'), OSEvent::text('i')],
    );
    assert_eq!(buffer, "hi");

    // One undo removes the whole run.
    drive_textarea(
        &mut ui,
        &mut buffer,
        edit,
        vec![OSEvent::press_with_flags(
            OSKey::Keyboard(OSKeyCode::KeyZ),
            None,
            Some(PRIMARY),
        )],
    );
    assert_eq!(buffer, "");

    // Redo reinstates it.
    drive_textarea(
        &mut ui,
        &mut buffer,
        edit,
        vec![OSEvent::press_with_flags(
            OSKey::Keyboard(OSKeyCode::KeyZ),
            None,
            Some(PRIMARY_SHIFT),
        )],
    );
    assert_eq!(buffer, "hi");
}

#[test]
fn textarea_undo_steps_word_by_word() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = String::new();

    ui.begin_frame();
    let edit = ui.textarea("Text###text", &mut buffer);
    ui.width(edit, UISize::Pixels(240.0));
    ui.height(edit, UISize::Pixels(80.0));
    ui.end_frame();

    drive_textarea(
        &mut ui,
        &mut buffer,
        edit,
        "foo bar".chars().map(OSEvent::text).collect(),
    );
    assert_eq!(buffer, "foo bar");

    // The whitespace closes the first word, so one undo only removes the second word.
    drive_textarea(
        &mut ui,
        &mut buffer,
        edit,
        vec![OSEvent::press_with_flags(
            OSKey::Keyboard(OSKeyCode::KeyZ),
            None,
            Some(PRIMARY),
        )],
    );
    assert_eq!(buffer, "foo ");

    // A second undo removes the first word too.
    drive_textarea(
        &mut ui,
        &mut buffer,
        edit,
        vec![OSEvent::press_with_flags(
            OSKey::Keyboard(OSKeyCode::KeyZ),
            None,
            Some(PRIMARY),
        )],
    );
    assert_eq!(buffer, "");
}

#[test]
fn read_only_textarea_consumes_typing_without_editing() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = "abc".to_string();
    let opts = TextAreaOptions::new().read_only(true);

    ui.begin_frame();
    let edit = ui.textarea_with_options("Text###text", &mut buffer, opts);
    ui.width(edit, UISize::Pixels(240.0));
    ui.height(edit, UISize::Pixels(80.0));
    ui.end_frame();

    ui.focus_key = Some(edit.key());
    ui.events = vec![OSEvent {
        ty: OSEventType::Press,
        key: OSKey::Keyboard(OSKeyCode::KeyZ),
        pos: None,
        chars: Some('z'),
        deltax: 0.0,
        deltay: 0.0,
        flags: None,
    }];

    ui.begin_frame();
    ui.textarea_with_options("Text###text", &mut buffer, opts);

    assert_eq!(buffer, "abc");
    assert!(ui.events.is_empty());
}

#[test]
fn wrapped_read_only_textarea_selects_to_bottom_right() {
    let mut ui = IMUI::new_for_test(520.0, 180.0);
    let mut buffer = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\
         0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        .replace(' ', "");
    let opts = TextAreaOptions::new()
        .wrap_x(true)
        .scroll_x(false)
        .scroll_y(false)
        .read_only(true)
        .font_size(9.0)
        .padding(Padding::all(4.0));

    let mut build = |ui: &mut IMUI| {
        let edit = ui.textarea_with_options("Key###key", &mut buffer, opts);
        ui.width(edit, UISize::Pixels(440.0));
        ui.height(edit, UISize::Pixels(36.0));
        edit
    };

    ui.begin_frame();
    let edit = build(&mut ui);
    ui.end_frame();

    let rect = ui.boxes[edit.idx()].rect;
    let start = Point::new(rect.x0 + 1.0, rect.y0 + 5.0);
    let end = Point::new(rect.x1 - 4.0, rect.y1 - 4.0);

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(start)));
    ui.begin_frame();
    build(&mut ui);
    ui.end_frame();

    push_test_event(&mut ui, OSEvent::mouse_move(end));
    ui.begin_frame();
    let edit = build(&mut ui);
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.selection_range(), Some((0, char_count(&buffer))));
}

/// Wrapped lines must honour the configured right padding, not just the left. The
/// wrap width is computed while the textarea emits its lines, so the inset has to be
/// known then (i.e. supplied through the options, not a post-hoc `.padding` builder);
/// an asymmetric inset makes a calc that drops the right pad overflow the box visibly.
#[test]
fn wrapped_lines_respect_right_padding() {
    let mut ui = IMUI::new_for_test(520.0, 180.0);
    // A long, space-free run forces wrapping regardless of word boundaries.
    let mut buffer = "0123456789abcdef".repeat(8);
    let pad = Padding {
        top: 6.0,
        right: 60.0,
        bottom: 6.0,
        left: 12.0,
    };
    let opts = TextAreaOptions::new()
        .wrap_x(true)
        .scroll_x(false)
        .scroll_y(false)
        .read_only(true)
        .font_size(9.0)
        .padding(pad);

    let mut build = |ui: &mut IMUI| {
        let edit = ui.textarea_with_options("Key###key", &mut buffer, opts);
        ui.width(edit, UISize::Pixels(440.0));
        ui.height(edit, UISize::Pixels(120.0));
        edit
    };

    // Two frames: the wrap width is derived from the previous frame's rect, so the
    // first frame has no width to wrap against yet.
    ui.begin_frame();
    build(&mut ui);
    ui.end_frame();
    ui.begin_frame();
    let edit = build(&mut ui);
    ui.end_frame();

    let area = ui.boxes[edit.idx()].rect;
    let right_limit = area.x1 - pad.right;
    let lines = ui.boxes[edit.idx()].children.clone();
    // The text is long enough that it must wrap onto several visual lines.
    assert!(
        lines.len() > 1,
        "expected wrapped lines, got {}",
        lines.len()
    );
    for &line in &lines {
        let line_x1 = ui.boxes[line].rect.x1;
        assert!(
            line_x1 <= right_limit + 0.5,
            "line right edge {line_x1} exceeds right-padding limit {right_limit}"
        );
    }
}

#[test]
fn blocking_overlay_suppresses_editor_cursor_under_it() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = "underlay".to_string();
    ui.mouse = Some(Point::new(40.0, 40.0));

    let mut build = |ui: &mut IMUI| {
        let editor = ui.textarea("Text###text", &mut buffer);
        ui.width(editor, UISize::Pixels(400.0));
        ui.height(editor, UISize::Pixels(200.0));

        let pane = ui.floating_pane_at(Point::new(20.0, 20.0), Some("###overlay"), |ui| {
            let label = ui.label("Overlay");
            ui.width(label, UISize::Pixels(120.0));
            ui.height(label, UISize::Pixels(24.0));
        });
        ui.width(pane, UISize::Pixels(160.0));
        ui.height(pane, UISize::Pixels(80.0));
        editor
    };

    // First frame establishes geometry (and the overlay's blacklist rect);
    // hover/cursor are computed inline against the previous frame, so the
    // suppression is only observable from the second frame on.
    ui.begin_frame();
    build(&mut ui);
    ui.end_frame();

    ui.begin_frame();
    let editor = build(&mut ui);
    ui.end_frame();

    assert!(ui.boxes[editor.idx()].signal.mouse_over());
    assert!(!ui.boxes[editor.idx()].signal.hovering());
    assert_eq!(ui.cursor, OSCursor::Arrow);
}

#[test]
fn focused_line_edit_consumes_text_events() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = String::new();

    ui.begin_frame();
    let edit = ui.line_edit("Edit###edit", &mut buffer, false);
    ui.width(edit, UISize::Pixels(120.0));
    ui.height(edit, UISize::Pixels(32.0));
    ui.end_frame();

    ui.focus_key = Some(edit.key());
    ui.events = vec![OSEvent {
        ty: OSEventType::Press,
        key: OSKey::Keyboard(OSKeyCode::KeyA),
        pos: None,
        chars: Some('a'),
        deltax: 0.0,
        deltay: 0.0,
        flags: None,
    }];

    ui.begin_frame();
    ui.line_edit("Edit###edit", &mut buffer, false);

    assert_eq!(buffer, "a");
    assert!(ui.events.is_empty());
}

#[test]
fn line_edit_scrolls_to_follow_caret() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = String::new();

    let build = |ui: &mut IMUI, buffer: &mut String| {
        let edit = ui.line_edit("Edit###edit", buffer, false);
        ui.width(edit, UISize::Pixels(60.0));
        ui.height(edit, UISize::Pixels(32.0));
        edit
    };

    ui.begin_frame();
    let edit = build(&mut ui, &mut buffer);
    ui.end_frame();
    ui.focus_key = Some(edit.key());

    // Type past the right edge of the narrow field.
    ui.events = "abcdefghijklmnopqrstuvwxyz"
        .chars()
        .map(OSEvent::text)
        .collect();
    ui.begin_frame();
    let edit = build(&mut ui, &mut buffer);
    ui.end_frame();

    assert_eq!(buffer, "abcdefghijklmnopqrstuvwxyz");
    let scrolled = ui.boxes[edit.idx()].scroll_target.x;
    assert!(
        scrolled > 0.0,
        "line edit should scroll right to keep the caret visible, got {scrolled}"
    );

    // Jumping back to the start scrolls the view fully left again.
    ui.events = vec![OSEvent::press(OSKey::Keyboard(OSKeyCode::KeyHome), None)];
    ui.begin_frame();
    let edit = build(&mut ui, &mut buffer);
    ui.end_frame();
    assert_eq!(ui.boxes[edit.idx()].scroll_target.x, 0.0);
}

#[test]
fn line_edit_caret_x_survives_a_collapsed_content_box() {
    // An editable label sized to hug its text collapses its content area to zero
    // (or, with padding, negative) width when the text is empty, so `content_x1`
    // dips below `content_x0`. The caret math used to feed that straight into
    // `f32::clamp`, panicking with `min > max` (observed: 570.2385 > 569.2385).
    let content_x0 = 570.2385;
    let content_x1 = 570.2385; // `content_x1 - 1.0` is below `content_x0`
    let caret = super::paint::line_edit_caret_x(content_x0, content_x1, 0.0, 0.0);
    assert!(caret.is_finite());
    assert_eq!(caret, content_x0);

    // The normal (roomy) case still clamps the caret to just inside the box.
    let caret = super::paint::line_edit_caret_x(10.0, 100.0, 1000.0, 0.0);
    assert_eq!(caret, 99.0);
}

#[test]
fn line_edit_selects_text_with_mouse_drag() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = "abcdef".to_string();

    ui.begin_frame();
    let edit = ui.line_edit("Edit###edit", &mut buffer, false);
    ui.width(edit, UISize::Pixels(220.0));
    ui.height(edit, UISize::Pixels(32.0));
    ui.end_frame();

    let rect = ui.boxes[edit.idx()].rect;
    let padding = ui.boxes[edit.idx()].padding;
    let style = ui.boxes[edit.idx()].style;
    let char_w = style.font_size * 0.6;
    let content_x = rect.x0 + padding.left + style.margin;
    let y = rect.y0 + rect.height() * 0.5;
    let start = Point::new(content_x + char_w * 1.2, y);
    let end = Point::new(content_x + char_w * 4.2, y);

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(start)));
    ui.begin_frame();
    let edit = ui.line_edit("Edit###edit", &mut buffer, false);
    ui.width(edit, UISize::Pixels(220.0));
    ui.height(edit, UISize::Pixels(32.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.cursor, 1);
    assert_eq!(state.selection_range(), None);
    assert_eq!(ui.focus_key, Some(edit.key()));

    push_test_event(&mut ui, OSEvent::mouse_move(end));
    ui.begin_frame();
    let edit = ui.line_edit("Edit###edit", &mut buffer, false);
    ui.width(edit, UISize::Pixels(220.0));
    ui.height(edit, UISize::Pixels(32.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.cursor, 4);
    assert_eq!(state.selection_range(), Some((1, 4)));

    push_test_event(&mut ui, OSEvent::release(OSKey::LeftMouseButton, Some(end)));
    ui.begin_frame();
    let edit = ui.line_edit("Edit###edit", &mut buffer, false);
    ui.width(edit, UISize::Pixels(220.0));
    ui.height(edit, UISize::Pixels(32.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.selection_range(), Some((1, 4)));
}

#[test]
fn line_edit_double_click_selects_current_word() {
    let mut ui = IMUI::new_for_test(400.0, 120.0);
    let mut buffer = "alpha beta gamma".to_string();

    ui.begin_frame();
    let edit = ui.line_edit("Edit###edit", &mut buffer, false);
    ui.width(edit, UISize::Pixels(260.0));
    ui.height(edit, UISize::Pixels(32.0));
    ui.end_frame();

    let rect = ui.boxes[edit.idx()].rect;
    let padding = ui.boxes[edit.idx()].padding;
    let style = ui.boxes[edit.idx()].style;
    let char_w = style.font_size * 0.6;
    let point = Point::new(
        rect.x0 + padding.left + style.margin + char_w * 8.2,
        rect.y0 + rect.height() * 0.5,
    );

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(point)));
    ui.begin_frame();
    let edit = ui.line_edit("Edit###edit", &mut buffer, false);
    ui.width(edit, UISize::Pixels(260.0));
    ui.height(edit, UISize::Pixels(32.0));
    ui.end_frame();

    push_test_event(
        &mut ui,
        OSEvent::release(OSKey::LeftMouseButton, Some(point)),
    );
    ui.begin_frame();
    let edit = ui.line_edit("Edit###edit", &mut buffer, false);
    ui.width(edit, UISize::Pixels(260.0));
    ui.height(edit, UISize::Pixels(32.0));
    ui.end_frame();

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(point)));
    ui.begin_frame();
    let edit = ui.line_edit("Edit###edit", &mut buffer, false);
    ui.width(edit, UISize::Pixels(260.0));
    ui.height(edit, UISize::Pixels(32.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.selection_range(), Some((6, 10)));
}

#[test]
fn textarea_click_maps_to_letter_with_custom_padding() {
    // The caller sets a large left padding after the widget is built (as the note app
    // does). The click hit-test must use that final padding, not the widget default,
    // so clicking a glyph lands the caret on that glyph.
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = "hello world".to_string();
    let pad_left = 48.0;
    let pad_top = 28.0;

    let mut build = |ui: &mut IMUI| {
        let edit = ui.textarea("Text###text", &mut buffer);
        ui.width(edit, UISize::Pixels(320.0));
        ui.height(edit, UISize::Pixels(120.0));
        ui.padding(edit, pad_top, 36.0, 28.0, pad_left);
        edit
    };

    ui.begin_frame();
    let edit = build(&mut ui);
    ui.end_frame();

    // Click just past the start of the 7th character ('w') on the first line.
    let rect = ui.boxes[edit.idx()].rect;
    let style = ui.boxes[edit.idx()].style;
    let char_w = style.font_size * 0.6;
    let click = Point::new(
        rect.x0 + pad_left + char_w * 6.4,
        rect.y0 + pad_top + style.font_size * 0.5,
    );

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(click)));
    ui.begin_frame();
    let edit = build(&mut ui);
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.cursor, 6, "caret should land on the clicked glyph");
}

#[test]
fn textarea_selects_text_with_mouse_drag_across_lines() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut buffer = "abc\ndef".to_string();

    ui.begin_frame();
    let edit = ui.textarea("Text###text", &mut buffer);
    ui.width(edit, UISize::Pixels(220.0));
    ui.height(edit, UISize::Pixels(120.0));
    ui.end_frame();

    let rect = ui.boxes[edit.idx()].rect;
    let padding = ui.boxes[edit.idx()].padding;
    let style = ui.boxes[edit.idx()].style;
    let char_w = style.font_size * 0.6;
    let line_h = ui.theme.size_text + 6.0;
    let content_x = rect.x0 + padding.left + style.margin;
    let content_y = rect.y0 + padding.top + style.margin;
    let start = Point::new(content_x + char_w * 1.2, content_y + line_h * 0.5);
    let end = Point::new(content_x + char_w * 2.2, content_y + line_h * 1.5);

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(start)));
    ui.begin_frame();
    let edit = ui.textarea("Text###text", &mut buffer);
    ui.width(edit, UISize::Pixels(220.0));
    ui.height(edit, UISize::Pixels(120.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.cursor, 1);
    assert_eq!(state.selection_range(), None);
    assert_eq!(ui.focus_key, Some(edit.key()));

    push_test_event(&mut ui, OSEvent::mouse_move(end));
    ui.begin_frame();
    let edit = ui.textarea("Text###text", &mut buffer);
    ui.width(edit, UISize::Pixels(220.0));
    ui.height(edit, UISize::Pixels(120.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.cursor, 6);
    assert_eq!(state.selection_range(), Some((1, 6)));

    push_test_event(&mut ui, OSEvent::release(OSKey::LeftMouseButton, Some(end)));
    ui.begin_frame();
    let edit = ui.textarea("Text###text", &mut buffer);
    ui.width(edit, UISize::Pixels(220.0));
    ui.height(edit, UISize::Pixels(120.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.selection_range(), Some((1, 6)));
}

#[test]
fn textarea_triple_click_selects_current_line() {
    let mut ui = IMUI::new_for_test(500.0, 220.0);
    let mut buffer = "one two\nthree four\n\nfive".to_string();

    ui.begin_frame();
    let edit = ui.textarea("Text###text", &mut buffer);
    ui.width(edit, UISize::Pixels(320.0));
    ui.height(edit, UISize::Pixels(150.0));
    ui.end_frame();

    let rect = ui.boxes[edit.idx()].rect;
    let padding = ui.boxes[edit.idx()].padding;
    let style = ui.boxes[edit.idx()].style;
    let char_w = style.font_size * 0.6;
    let line_h = ui.theme.size_text + 6.0;
    let point = Point::new(
        rect.x0 + padding.left + style.margin + char_w * 3.0,
        rect.y0 + padding.top + style.margin + line_h * 1.5,
    );

    for _ in 0..2 {
        push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(point)));
        ui.begin_frame();
        let edit = ui.textarea("Text###text", &mut buffer);
        ui.width(edit, UISize::Pixels(320.0));
        ui.height(edit, UISize::Pixels(150.0));
        ui.end_frame();

        push_test_event(
            &mut ui,
            OSEvent::release(OSKey::LeftMouseButton, Some(point)),
        );
        ui.begin_frame();
        let edit = ui.textarea("Text###text", &mut buffer);
        ui.width(edit, UISize::Pixels(320.0));
        ui.height(edit, UISize::Pixels(150.0));
        ui.end_frame();
    }

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(point)));
    ui.begin_frame();
    let edit = ui.textarea("Text###text", &mut buffer);
    ui.width(edit, UISize::Pixels(320.0));
    ui.height(edit, UISize::Pixels(150.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.selection_range(), Some((8, 18)));
}

#[test]
fn markdown_rendered_mode_hides_markers_but_preserves_offsets() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let lines = ui.build_editor_layout(
        "**bold**",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Rendered,
    );
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    // The raw range and per-char cum_x cover every raw char, markers included.
    assert_eq!((line.raw_start, line.raw_end), (0, 8));
    assert_eq!(line.cum_x.len(), 9);
    // Only the inner content is drawn — `spans` still carries the hidden
    // `**` markers too (as zero-width placeholder text, so the DOM backend
    // has somewhere to land a caret inside them — see `LayoutSpan`'s doc
    // comment), so exclude those to get what's actually shown.
    let rendered: String = line
        .spans
        .iter()
        .filter(|s| !s.hidden)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(rendered, "bold");
    // Leading "**" markers are zero-width so cursor offsets still resolve.
    let cw = 10.0 * 0.6;
    assert_eq!(line.cum_x[0], 0.0);
    assert_eq!(line.cum_x[2], 0.0);
    assert!((line.cum_x[6] - 4.0 * cw).abs() < 1e-3);
    // Trailing "**" markers add no width.
    assert!((line.cum_x[8] - 4.0 * cw).abs() < 1e-3);
}

#[test]
fn markdown_source_mode_keeps_markers_visible() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let lines = ui.build_editor_layout(
        "# Title",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Source,
    );
    let line = &lines[0];
    let rendered: String = line.spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(rendered, "# Title");
    // Source view leaves every line at the base size. Scaling belongs to the
    // rendered view: here the `#` markers are text under the caret, and
    // resizing the line as they are typed reflows it mid-keystroke.
    assert_eq!(
        line.font_size, 10.0,
        "source view should not resize a heading"
    );
    assert_eq!(line.cum_x.len(), 8);
}

#[test]
fn markdown_rendered_mode_hides_heading_markers_but_preserves_offsets() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let lines = ui.build_editor_layout(
        "## Title",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Rendered,
    );
    let line = &lines[0];
    let rendered: String = line
        .spans
        .iter()
        .filter(|s| !s.hidden)
        .map(|s| s.text.as_str())
        .collect();

    assert_eq!(rendered, "Title");
    assert_eq!((line.raw_start, line.raw_end), (0, 8));
    assert_eq!(line.cum_x.len(), 9);
    assert_eq!(line.cum_x[3], 0.0);
    assert!(line.font_size > 10.0, "heading should still scale font up");
}

#[test]
fn markdown_fenced_rust_code_highlights_tokens() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let lines = ui.build_editor_layout(
        "```rust\nfn main() {\n    let value = \"hi\"; // greet\n}\n```",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Source,
    );

    assert_eq!(lines.len(), 5);
    assert_span(&lines[1], "fn", ui.theme.accent);
    assert_span(&lines[2], "let", ui.theme.accent);
    assert_span(&lines[2], "\"hi\"", ui.theme.accent_active);
    assert_span(&lines[2], "// greet", ui.theme.text_muted);
    assert_eq!(lines[1].cum_x.len(), "fn main() {".chars().count() + 1);
}

#[test]
fn markdown_fenced_python_code_highlights_hash_comments() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let lines = ui.build_editor_layout(
        "```python\nif value:\n    return \"ok\" # done\n```",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Source,
    );

    assert_span(&lines[1], "if", ui.theme.accent);
    assert_span(&lines[2], "return", ui.theme.accent);
    assert_span(&lines[2], "\"ok\"", ui.theme.accent_active);
    assert_span(&lines[2], "# done", ui.theme.text_muted);
}

#[test]
fn markdown_rendered_mode_hides_heading_markers_and_keeps_quote_prefix_visible() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let lines = ui.build_editor_layout(
        "# Title\n> quoted\n```rust\nlet x = 1;\n```",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Rendered,
    );

    let heading: String = lines[0]
        .spans
        .iter()
        .filter(|s| !s.hidden)
        .map(|s| s.text.as_str())
        .collect();
    let quote: String = lines[1]
        .spans
        .iter()
        .filter(|s| !s.hidden)
        .map(|s| s.text.as_str())
        .collect();
    let code: String = lines[2]
        .spans
        .iter()
        .filter(|s| !s.hidden)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(heading, "Title");
    assert_eq!(quote, "> quoted");
    assert_eq!(code, "let x = 1;");
    assert!(lines[1].spans.first().is_some_and(
        |span| span.text.starts_with("> ") && colors_eq(span.color, ui.theme.text_muted)
    ));
}

#[test]
fn markdown_rendered_mode_reveals_fence_prefix_on_caret_line() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let text = "# Title\n> quoted\n```rust\nlet x = 1;\n```";
    let fence_line_start = "# Title\n> quoted\n".chars().count();
    let layout = ui.build_editor_layout_revealing_line(
        text,
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Rendered,
        Some(fence_line_start),
    );
    let lines = &layout.lines;

    assert_eq!(lines.len(), 5);
    let fence: String = lines[2].spans.iter().map(|s| s.text.as_str()).collect();
    let closing: String = lines[4].spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(fence, "```rust");
    assert_eq!(closing, "```");
    assert_span(&lines[2], "```rust", ui.theme.text_muted);
    assert_span(&lines[4], "```", ui.theme.text_muted);
}

#[test]
fn markdown_rendered_mode_reveals_fences_when_caret_is_inside_code_block() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let text = "# Title\n```rust\nlet x = 1;\n```";
    let code_line_start = "# Title\n```rust\n".chars().count();
    let layout = ui.build_editor_layout_revealing_line(
        text,
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Rendered,
        Some(code_line_start),
    );
    let lines = &layout.lines;

    assert_eq!(lines.len(), 4);
    let opening: String = lines[1].spans.iter().map(|s| s.text.as_str()).collect();
    let code: String = lines[2].spans.iter().map(|s| s.text.as_str()).collect();
    let closing: String = lines[3].spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(opening, "```rust");
    assert_eq!(code, "let x = 1;");
    assert_eq!(closing, "```");
    assert_span(&lines[1], "```rust", ui.theme.text_muted);
    assert_span(&lines[3], "```", ui.theme.text_muted);
}

#[test]
fn markdown_clicking_focused_code_block_does_not_select_hidden_fences() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    ui.set_markdown_mode(MarkdownMode::Rendered);
    let mut buffer = "outside\n```rust\nlet x = 1;\nlet y = 2;\n```".to_string();

    ui.begin_frame();
    let edit = ui.markdown_textarea_with_options(
        "Editor###editor",
        &mut buffer,
        TextAreaOptions::new()
            .wrap_x(true)
            .scroll_x(false)
            .scroll_y(false),
    );
    ui.width(edit, UISize::Pixels(360.0));
    ui.height(edit, UISize::Pixels(160.0));
    ui.end_frame();

    assert_eq!(ui.focus_key, None);
    let outside_row = ui.boxes[edit.idx()].children[0];
    let row = &ui.boxes[outside_row];
    let char_w = row.style.font_size * 0.6;
    let outside_click = Point::new(
        row.rect.x0 + row.padding.left + char_w * 3.5,
        (row.rect.y0 + row.rect.y1) * 0.5,
    );

    push_test_event(
        &mut ui,
        OSEvent::press(OSKey::LeftMouseButton, Some(outside_click)),
    );
    ui.begin_frame();
    let edit = ui.markdown_textarea_with_options(
        "Editor###editor",
        &mut buffer,
        TextAreaOptions::new()
            .wrap_x(true)
            .scroll_x(false)
            .scroll_y(false),
    );
    ui.width(edit, UISize::Pixels(360.0));
    ui.height(edit, UISize::Pixels(160.0));
    ui.end_frame();

    push_test_event(
        &mut ui,
        OSEvent::release(OSKey::LeftMouseButton, Some(outside_click)),
    );
    ui.begin_frame();
    let edit = ui.markdown_textarea_with_options(
        "Editor###editor",
        &mut buffer,
        TextAreaOptions::new()
            .wrap_x(true)
            .scroll_x(false)
            .scroll_y(false),
    );
    ui.width(edit, UISize::Pixels(360.0));
    ui.height(edit, UISize::Pixels(160.0));
    ui.end_frame();

    assert_eq!(ui.focus_key, Some(edit.key()));
    let code_row = ui.boxes[edit.idx()].children[1];
    let row = &ui.boxes[code_row];
    let char_w = row.style.font_size * 0.6;
    let click = Point::new(
        row.rect.x0 + row.padding.left + char_w * 4.5,
        (row.rect.y0 + row.rect.y1) * 0.5,
    );

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(click)));
    ui.begin_frame();
    let edit = ui.markdown_textarea_with_options(
        "Editor###editor",
        &mut buffer,
        TextAreaOptions::new()
            .wrap_x(true)
            .scroll_x(false)
            .scroll_y(false),
    );
    ui.width(edit, UISize::Pixels(360.0));
    ui.height(edit, UISize::Pixels(160.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.selection_range(), None);
    assert!(state.cursor >= "outside\n```rust\n".chars().count());

    ui.begin_frame();
    let edit = ui.markdown_textarea_with_options(
        "Editor###editor",
        &mut buffer,
        TextAreaOptions::new()
            .wrap_x(true)
            .scroll_x(false)
            .scroll_y(false),
    );
    ui.width(edit, UISize::Pixels(360.0));
    ui.height(edit, UISize::Pixels(160.0));
    ui.end_frame();

    let state = ui.text_edit_states.get(&edit.key()).unwrap();
    assert_eq!(state.selection_range(), None);
}

#[test]
fn markdown_quote_and_code_lines_have_block_background_padding() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let layout = ui.build_editor_layout_revealing_line(
        "> quoted\n```rust\nlet x = 1;\n```",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Source,
        None,
    );
    let lines = &layout.lines;

    assert!(lines[0].padding.left > 0.0);
    assert!(lines[1].padding.left > lines[0].padding.left - 0.1);
    assert_eq!(layout.blocks.len(), 2);
    assert_eq!(layout.blocks[0].kind, MarkdownBlockKind::Quote);
    assert_eq!(layout.blocks[0].first_visual_line, 0);
    assert_eq!(layout.blocks[0].last_visual_line, 0);
    assert_eq!(layout.blocks[1].kind, MarkdownBlockKind::Code);
    assert_eq!(layout.blocks[1].first_visual_line, 1);
    assert_eq!(layout.blocks[1].last_visual_line, 3);
    assert_eq!(layout.blocks[1].label, Some("Rust"));

    let plain = ui.build_editor_layout_revealing_line(
        "```\nplain\n```",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Source,
        None,
    );
    assert_eq!(plain.blocks[0].label, Some("Plain text"));
}

#[test]
fn markdown_block_padding_is_reflected_in_emitted_rows() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    ui.set_markdown_mode(MarkdownMode::Source);
    let mut buffer = "> quoted\n```rust\nlet x = 1;\n```".to_string();

    ui.begin_frame();
    let edit = ui.markdown_textarea_with_options(
        "Editor###editor",
        &mut buffer,
        TextAreaOptions::new()
            .wrap_x(true)
            .scroll_x(false)
            .scroll_y(false),
    );
    ui.width(edit, UISize::Pixels(360.0));
    ui.height(edit, UISize::Pixels(140.0));
    ui.end_frame();

    let rows = ui.boxes[edit.idx()].children.clone();
    assert_eq!(rows.len(), 4);
    for &row in &rows {
        assert!(!ui.boxes[row].flags.contains(UIBoxFlags::DRAW_BACKGROUND));
        assert!(ui.boxes[row].padding.left > 0.0);
    }
    assert_eq!(ui.boxes[edit.idx()].child_gap, 2.0);
    let layout = ui.editor_layouts.get(&edit.key()).unwrap();
    assert_eq!(layout.blocks[1].first_visual_line, 1);
    assert_eq!(layout.blocks[1].last_visual_line, 3);
}

#[test]
fn markdown_enter_after_opening_fence_closes_block_and_places_caret_inside() {
    for opening in ["```", "```js"] {
        let mut ui = IMUI::new_for_test(400.0, 200.0);
        let mut buffer = opening.to_string();

        ui.begin_frame();
        let edit = ui.markdown_textarea_with_options(
            "Editor###editor",
            &mut buffer,
            TextAreaOptions::new()
                .wrap_x(true)
                .scroll_x(false)
                .scroll_y(false),
        );
        ui.width(edit, UISize::Pixels(360.0));
        ui.height(edit, UISize::Pixels(140.0));
        ui.end_frame();

        ui.focus_key = Some(edit.key());
        ui.text_edit_states.entry(edit.key()).or_default().cursor = char_count(&buffer);
        ui.events = vec![OSEvent::press(OSKey::Keyboard(OSKeyCode::KeyEnter), None)];

        ui.begin_frame();
        let edit = ui.markdown_textarea_with_options(
            "Editor###editor",
            &mut buffer,
            TextAreaOptions::new()
                .wrap_x(true)
                .scroll_x(false)
                .scroll_y(false),
        );
        ui.width(edit, UISize::Pixels(360.0));
        ui.height(edit, UISize::Pixels(140.0));
        ui.end_frame();

        assert_eq!(buffer, format!("{opening}\n\n```"));
        assert_eq!(
            ui.text_edit_states.get(&edit.key()).unwrap().cursor,
            char_count(opening) + 1
        );
    }
}

fn drive_markdown_textarea_key(buffer: &mut String, cursor: usize, ev: OSEvent) -> usize {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let edit = ui.markdown_textarea_with_options(
        "Editor###editor",
        buffer,
        TextAreaOptions::new()
            .wrap_x(true)
            .scroll_x(false)
            .scroll_y(false),
    );
    ui.width(edit, UISize::Pixels(360.0));
    ui.height(edit, UISize::Pixels(140.0));
    ui.end_frame();

    ui.focus_key = Some(edit.key());
    ui.text_edit_states.entry(edit.key()).or_default().cursor = cursor;
    ui.events = vec![ev];

    ui.begin_frame();
    let edit = ui.markdown_textarea_with_options(
        "Editor###editor",
        buffer,
        TextAreaOptions::new()
            .wrap_x(true)
            .scroll_x(false)
            .scroll_y(false),
    );
    ui.width(edit, UISize::Pixels(360.0));
    ui.height(edit, UISize::Pixels(140.0));
    ui.end_frame();

    ui.text_edit_states.get(&edit.key()).unwrap().cursor
}

#[test]
fn markdown_enter_after_fence_inside_code_block_does_not_auto_close() {
    let mut buffer = "```js\n```".to_string();
    let cursor = drive_markdown_textarea_key(
        &mut buffer,
        char_count("```js\n```"),
        OSEvent::press(OSKey::Keyboard(OSKeyCode::KeyEnter), None),
    );

    assert_eq!(buffer, "```js\n```\n");
    assert_eq!(cursor, char_count(&buffer));
}

#[test]
fn markdown_arrow_down_at_bottom_of_code_block_exits_and_inserts_line() {
    let mut buffer = "```js\nlet x = 1;\n```".to_string();
    let cursor = drive_markdown_textarea_key(
        &mut buffer,
        char_count("```js\nlet x = 1;"),
        OSEvent::press(OSKey::Keyboard(OSKeyCode::KeyDownArrow), None),
    );

    assert_eq!(buffer, "```js\nlet x = 1;\n```\n");
    assert_eq!(cursor, char_count(&buffer));
}

#[test]
fn markdown_shift_enter_at_bottom_of_code_block_exits_and_inserts_line() {
    let mut buffer = "```js\nlet x = 1;\n```".to_string();
    let cursor = drive_markdown_textarea_key(
        &mut buffer,
        char_count("```js\nlet x = 1;"),
        OSEvent::press_with_flags(
            OSKey::Keyboard(OSKeyCode::KeyEnter),
            None,
            Some(OSEventFlag::Shift),
        ),
    );

    assert_eq!(buffer, "```js\nlet x = 1;\n```\n");
    assert_eq!(cursor, char_count(&buffer));
}

#[test]
fn markdown_arrow_down_at_bottom_of_code_block_uses_existing_line() {
    let mut buffer = "```js\nlet x = 1;\n```\nafter".to_string();
    let cursor = drive_markdown_textarea_key(
        &mut buffer,
        char_count("```js\nlet x = 1;"),
        OSEvent::press(OSKey::Keyboard(OSKeyCode::KeyDownArrow), None),
    );

    assert_eq!(buffer, "```js\nlet x = 1;\n```\nafter");
    assert_eq!(cursor, char_count("```js\nlet x = 1;\n```\n"));
}

#[test]
fn editor_layout_wraps_on_width() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let cw = 10.0 * 0.6;
    let lines = ui.build_editor_layout(
        "aaaaaa",
        cw * 3.5,
        10.0,
        TextAreaLineStyle::Plain,
        MarkdownMode::Source,
    );
    assert_eq!(lines.len(), 2);
    assert_eq!((lines[0].raw_start, lines[0].raw_end), (0, 3));
    assert_eq!((lines[1].raw_start, lines[1].raw_end), (3, 6));
}

#[test]
fn editor_layout_prefers_word_boundary_when_wrapping() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let cw = 10.0 * 0.6;
    let lines = ui.build_editor_layout(
        "aaaa bbbb cccc",
        cw * 11.5,
        10.0,
        TextAreaLineStyle::Plain,
        MarkdownMode::Source,
    );
    assert_eq!(lines.len(), 2);
    assert_eq!((lines[0].raw_start, lines[0].raw_end), (0, 10));
    assert_eq!((lines[1].raw_start, lines[1].raw_end), (10, 14));
}

#[test]
fn editor_layout_cache_skips_rebuild_for_unchanged_text() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let key = UiKey(42);
    ui.ensure_editor_layout(
        key,
        "hello",
        0.0,
        10.0,
        TextAreaLineStyle::Plain,
        MarkdownMode::Source,
    );
    let first = ui.editor_layouts.get(&key).unwrap().hash;
    ui.ensure_editor_layout(
        key,
        "hello",
        0.0,
        10.0,
        TextAreaLineStyle::Plain,
        MarkdownMode::Source,
    );
    // Same content -> same cached entry (no rebuild path taken).
    assert_eq!(ui.editor_layouts.get(&key).unwrap().hash, first);
    ui.ensure_editor_layout(
        key,
        "hello world",
        0.0,
        10.0,
        TextAreaLineStyle::Plain,
        MarkdownMode::Source,
    );
    assert_ne!(ui.editor_layouts.get(&key).unwrap().hash, first);
}

fn assert_span(line: &LayoutLine, text: &str, color: Color) {
    assert!(
        line.spans
            .iter()
            .any(|span| span.text == text && colors_eq(span.color, color)),
        "expected span {text:?} with color {color:?}"
    );
}

#[test]
fn theme_switch_restyles_cached_editor_layout() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    ui.set_theme(UITheme::dark());
    let key = UiKey(7);
    ui.ensure_editor_layout(
        key,
        "hello",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Source,
    );
    let dark_color = ui.editor_layouts.get(&key).unwrap().lines[0].spans[0].color;
    assert!(colors_eq(dark_color, UITheme::dark().text));

    // Switching themes must invalidate the cached colors, not keep dark-theme
    // near-white text that would be invisible on the light background.
    ui.set_theme(UITheme::light());
    assert!(ui.editor_layouts.get(&key).is_none());
    ui.ensure_editor_layout(
        key,
        "hello",
        0.0,
        10.0,
        TextAreaLineStyle::Markdown,
        MarkdownMode::Source,
    );
    let light_color = ui.editor_layouts.get(&key).unwrap().lines[0].spans[0].color;
    assert!(colors_eq(light_color, UITheme::light().text));
    assert!(!colors_eq(light_color, dark_color));
}

#[test]
fn vertical_scrollbar_width_animates_on_hover() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let pane = build_vertical_scroll_pane(&mut ui);
    ui.end_frame();

    assert_eq!(
        ui.scrollbar_thickness(pane.idx(), Axis::Y),
        SCROLLBAR_THICKNESS
    );
    let thumb = ui
        .scrollbar_thumb_rect(pane.idx(), Axis::Y, SCROLLBAR_THICKNESS)
        .unwrap();
    ui.mouse = Some(Point::new(
        thumb.x0 + thumb.width() * 0.5,
        thumb.y0 + thumb.height() * 0.5,
    ));
    ui.repaint_requested = false;

    ui.begin_frame();
    let pane = build_vertical_scroll_pane(&mut ui);
    ui.end_frame();

    let thickness = ui.scrollbar_thickness(pane.idx(), Axis::Y);
    assert!(thickness > SCROLLBAR_THICKNESS);
    assert!(thickness < SCROLLBAR_HOVER_THICKNESS);
    assert!(ui.repaint_requested);

    for _ in 0..120 {
        ui.repaint_requested = false;
        ui.begin_frame();
        build_vertical_scroll_pane(&mut ui);
        ui.end_frame();
        if !ui.repaint_requested {
            break;
        }
    }
    assert_eq!(
        ui.scrollbar_thickness(pane.idx(), Axis::Y),
        SCROLLBAR_HOVER_THICKNESS
    );

    ui.mouse = Some(Point::new(300.0, 180.0));
    ui.repaint_requested = false;
    ui.begin_frame();
    let pane = build_vertical_scroll_pane(&mut ui);
    ui.end_frame();

    let thickness = ui.scrollbar_thickness(pane.idx(), Axis::Y);
    assert!(thickness > SCROLLBAR_THICKNESS);
    assert!(thickness < SCROLLBAR_HOVER_THICKNESS);
    assert!(ui.repaint_requested);
}

#[test]
fn horizontal_scrollbar_height_animates_on_hover() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let pane = build_horizontal_scroll_pane(&mut ui);
    ui.end_frame();

    assert_eq!(
        ui.scrollbar_thickness(pane.idx(), Axis::X),
        SCROLLBAR_THICKNESS
    );
    let thumb = ui
        .scrollbar_thumb_rect(pane.idx(), Axis::X, SCROLLBAR_THICKNESS)
        .unwrap();
    ui.mouse = Some(Point::new(
        thumb.x0 + thumb.width() * 0.5,
        thumb.y0 + thumb.height() * 0.5,
    ));
    ui.repaint_requested = false;

    ui.begin_frame();
    let pane = build_horizontal_scroll_pane(&mut ui);
    ui.end_frame();

    let thickness = ui.scrollbar_thickness(pane.idx(), Axis::X);
    assert!(thickness > SCROLLBAR_THICKNESS);
    assert!(thickness < SCROLLBAR_HOVER_THICKNESS);
    assert!(ui.repaint_requested);

    for _ in 0..120 {
        ui.repaint_requested = false;
        ui.begin_frame();
        build_horizontal_scroll_pane(&mut ui);
        ui.end_frame();
        if !ui.repaint_requested {
            break;
        }
    }
    assert_eq!(
        ui.scrollbar_thickness(pane.idx(), Axis::X),
        SCROLLBAR_HOVER_THICKNESS
    );

    ui.mouse = Some(Point::new(300.0, 180.0));
    ui.repaint_requested = false;
    ui.begin_frame();
    let pane = build_horizontal_scroll_pane(&mut ui);
    ui.end_frame();

    let thickness = ui.scrollbar_thickness(pane.idx(), Axis::X);
    assert!(thickness > SCROLLBAR_THICKNESS);
    assert!(thickness < SCROLLBAR_HOVER_THICKNESS);
    assert!(ui.repaint_requested);
}

#[test]
fn vertical_scrollbar_click_and_drag_updates_scroll() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let pane = build_vertical_scroll_pane(&mut ui);
    ui.end_frame();

    let thumb = ui
        .scrollbar_thumb_rect(pane.idx(), Axis::Y, SCROLLBAR_HOVER_THICKNESS)
        .unwrap();
    let start = Point::new(
        thumb.x0 + thumb.width() * 0.5,
        thumb.y0 + thumb.height() * 0.5,
    );
    let end = Point::new(start.x(), start.y() + 24.0);

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(start)));
    ui.begin_frame();
    let _pane = build_vertical_scroll_pane(&mut ui);
    ui.end_frame();
    assert!(ui.active_scrollbar.is_some());
    assert!(ui.events.is_empty());

    push_test_event(&mut ui, OSEvent::mouse_move(end));
    ui.begin_frame();
    let pane = build_vertical_scroll_pane(&mut ui);
    ui.end_frame();
    assert!(ui.boxes[pane.idx()].scroll.y > 0.0);
    assert_eq!(
        ui.boxes[pane.idx()].scroll.y,
        ui.boxes[pane.idx()].scroll_target.y
    );

    push_test_event(&mut ui, OSEvent::release(OSKey::LeftMouseButton, Some(end)));
    ui.begin_frame();
    build_vertical_scroll_pane(&mut ui);
    ui.end_frame();
    assert!(ui.active_scrollbar.is_none());
    assert!(ui.events.is_empty());
}

#[test]
fn vertical_scrollbar_track_click_updates_scroll() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    ui.begin_frame();
    let pane = build_vertical_scroll_pane(&mut ui);
    ui.end_frame();

    let thumb = ui
        .scrollbar_thumb_rect(pane.idx(), Axis::Y, SCROLLBAR_HOVER_THICKNESS)
        .unwrap();
    let rect = ui.boxes[pane.idx()].rect;
    let click = Point::new(
        thumb.x0 + thumb.width() * 0.5,
        thumb.y1 + (rect.y1 - thumb.y1) * 0.5,
    );

    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(click)));
    ui.begin_frame();
    let pane = build_vertical_scroll_pane(&mut ui);
    ui.end_frame();

    assert!(ui.boxes[pane.idx()].scroll.y > 0.0);
    assert_eq!(
        ui.boxes[pane.idx()].scroll.y,
        ui.boxes[pane.idx()].scroll_target.y
    );
    assert!(ui.events.is_empty());
}

#[test]
fn vertical_scrollbar_press_wins_over_child_button() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    let mut child_clicked = false;

    let build = |ui: &mut IMUI, child_clicked: &mut bool| {
        let pane = ui.named_column("###scroll_buttons", |ui| {
            for idx in 0..8 {
                let button = ui.button(&format!("Row {idx}###scroll_button_{idx}"), None);
                ui.width(button, UISize::ParentPct(1.0));
                ui.height(button, UISize::Pixels(32.0));
                if button.clicked() {
                    *child_clicked = true;
                }
            }
        });
        ui.width(pane, UISize::Pixels(140.0));
        ui.height(pane, UISize::Pixels(72.0));
        ui.scroll_y(pane, true);
        pane
    };

    ui.begin_frame();
    let pane = build(&mut ui, &mut child_clicked);
    ui.end_frame();

    let track = ui.scrollbar_track_rect(pane.idx(), Axis::Y).unwrap();
    let press = Point::new(track.x0 + track.width() * 0.5, track.y0 + 18.0);
    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(press)));

    ui.begin_frame();
    build(&mut ui, &mut child_clicked);
    ui.end_frame();

    assert!(
        !child_clicked,
        "child button must not consume scrollbar press"
    );
    assert!(ui.active_scrollbar.is_some());
    assert!(ui.events.is_empty());
}

#[test]
fn active_scrollbar_drag_suppresses_child_button_hover() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);

    let build = |ui: &mut IMUI| {
        let mut first_button = None;
        let pane = ui.named_column("###scroll_buttons_hover", |ui| {
            for idx in 0..8 {
                let button = ui.button(&format!("Row {idx}###scroll_hover_button_{idx}"), None);
                ui.width(button, UISize::ParentPct(1.0));
                ui.height(button, UISize::Pixels(32.0));
                if idx == 0 {
                    first_button = Some(button);
                }
            }
        });
        ui.width(pane, UISize::Pixels(140.0));
        ui.height(pane, UISize::Pixels(72.0));
        ui.scroll_y(pane, true);
        (pane, first_button.unwrap())
    };

    ui.begin_frame();
    let (pane, _) = build(&mut ui);
    ui.end_frame();

    let track = ui.scrollbar_track_rect(pane.idx(), Axis::Y).unwrap();
    let press = Point::new(track.x0 + track.width() * 0.5, track.y0 + 18.0);
    push_test_event(&mut ui, OSEvent::press(OSKey::LeftMouseButton, Some(press)));
    ui.begin_frame();
    build(&mut ui);
    ui.end_frame();
    assert!(ui.active_scrollbar.is_some());

    push_test_event(&mut ui, OSEvent::mouse_move(Point::new(24.0, 24.0)));
    ui.begin_frame();
    let (_, first_button) = build(&mut ui);
    assert!(
        !first_button.hover(),
        "child button must not report build-time hover while scrollbar owns the drag"
    );
    ui.end_frame();

    assert!(
        !ui.boxes[first_button.idx()].signal.hovering(),
        "child button must not hover while scrollbar owns the drag"
    );
}

#[test]
fn topmost_box_wins_hovering_after_layout() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    ui.mouse = Some(Point::new(20.0, 20.0));

    // Base is declared *before* the overlay that covers it. The overlay still
    // wins hover: its footprint becomes a pointer-blacklist rect (captured at
    // begin_frame from the prior frame), which suppresses the base's inline
    // hover regardless of build order — observable from the second frame on.
    let build = |ui: &mut IMUI| {
        let base = ui.button("Base###base", None);
        ui.width(base, UISize::Pixels(120.0));
        ui.height(base, UISize::Pixels(40.0));

        let mut overlay_button = None;
        let overlay = ui.floating_pane_at(Point::new(0.0, 0.0), Some("###overlay"), |ui| {
            let button = ui.button("Overlay###overlay_button", None);
            ui.width(button, UISize::Pixels(120.0));
            ui.height(button, UISize::Pixels(40.0));
            overlay_button = Some(button);
        });
        ui.padding_all(overlay, 0.0);
        ui.gap(overlay, 0.0);
        (base, overlay_button.unwrap())
    };

    ui.begin_frame();
    build(&mut ui);
    ui.end_frame();

    ui.begin_frame();
    let (base, overlay_button) = build(&mut ui);
    ui.end_frame();

    assert!(ui.boxes[base.idx()].signal.mouse_over());
    assert!(!ui.boxes[base.idx()].signal.hovering());
    assert!(ui.boxes[overlay_button.idx()].signal.hovering());
}

#[test]
fn overlay_box_wins_hovering_even_when_declared_before_normal_box() {
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    ui.mouse = Some(Point::new(20.0, 20.0));

    // Overlay declared *before* the base box: the overlay claims hover via build
    // order, and the base is additionally suppressed by the overlay's blacklist
    // rect. Run two frames so the inline hover has prior-frame geometry to test.
    let build = |ui: &mut IMUI| {
        let mut overlay_button = None;
        let overlay = ui.floating_pane_at(Point::new(0.0, 0.0), Some("###overlay"), |ui| {
            let button = ui.button("Overlay###overlay_button", None);
            ui.width(button, UISize::Pixels(120.0));
            ui.height(button, UISize::Pixels(40.0));
            overlay_button = Some(button);
        });
        ui.padding_all(overlay, 0.0);
        ui.gap(overlay, 0.0);

        let base = ui.button("Base###base", None);
        ui.width(base, UISize::Pixels(120.0));
        ui.height(base, UISize::Pixels(40.0));
        (base, overlay_button.unwrap())
    };

    ui.begin_frame();
    build(&mut ui);
    ui.end_frame();

    ui.begin_frame();
    let (base, overlay_button) = build(&mut ui);
    ui.end_frame();

    assert!(ui.boxes[base.idx()].signal.mouse_over());
    assert!(!ui.boxes[base.idx()].signal.hovering());
    assert!(ui.boxes[overlay_button.idx()].signal.hovering());
}

/// Heap address of a box's text, to tell a reused buffer from a fresh one.
fn text_buffer_ptr(ui: &IMUI, idx: usize) -> *const u8 {
    ui.boxes[idx]
        .display_string
        .as_ref()
        .expect("box has text")
        .as_ptr()
}

#[test]
fn a_retained_box_reuses_its_text_buffer_between_frames() {
    let mut ui = IMUI::new_for_test(200.0, 100.0);
    ui.begin_frame();
    let first = ui.button("Save###save_btn", None);
    let before = text_buffer_ptr(&ui, first.idx);
    ui.end_frame();

    ui.begin_frame();
    let again = ui.button("Save###save_btn", None);
    let after = text_buffer_ptr(&ui, again.idx);
    ui.end_frame();

    assert_eq!(again.idx, first.idx, "the keyed box should be retained");
    assert_eq!(
        after, before,
        "a rebuild must write over the box's existing text buffer, not hand \
         it a freshly allocated one"
    );
}

#[test]
fn an_anonymous_boxs_text_buffer_comes_back_from_the_pool() {
    // `ui.label` has no `###id`, so its box is transient: released at the end
    // of every frame and built afresh in the next. The text buffer must come
    // back through `StringPool` rather than the allocator.
    let mut ui = IMUI::new_for_test(200.0, 100.0);
    ui.begin_frame();
    let first = ui.label("Ready");
    // Read inside the frame: the box is gone by the time `end_frame` returns.
    let before = text_buffer_ptr(&ui, first.idx);
    ui.end_frame();

    ui.begin_frame();
    let again = ui.label("Ready");
    let after = text_buffer_ptr(&ui, again.idx);
    ui.end_frame();

    assert_eq!(
        after, before,
        "the released box's buffer should have been recycled"
    );
}

#[test]
fn changing_a_boxs_text_keeps_the_same_buffer() {
    let mut ui = IMUI::new_for_test(200.0, 100.0);
    ui.begin_frame();
    let first = ui.button("Save###save_btn", None);
    let before = text_buffer_ptr(&ui, first.idx);
    ui.end_frame();

    ui.begin_frame();
    let again = ui.button("Send###save_btn", None);
    let after = text_buffer_ptr(&ui, again.idx);
    ui.end_frame();

    assert_eq!(ui.boxes[again.idx].display_string.as_deref(), Some("Send"));
    assert_eq!(after, before);
}

#[test]
fn a_display_string_containing_hashes_is_not_truncated_by_the_key_split() {
    // A markdown heading in source mode is a line whose *text* starts with
    // `##`. It reaches `alloc_box_parts` as display and id separately, so
    // there is no `Display###id` string for the split to cut in the wrong
    // place — which it used to, leaving the row addressable only by its
    // `string` and re-measuring its text every frame.
    let mut ui = IMUI::new_for_test(400.0, 200.0);
    ui.set_markdown_mode(MarkdownMode::Source);
    let mut text = "## Heading\nbody".to_string();
    ui.begin_frame();
    let edit = ui.markdown_textarea_with_options(
        "###editor",
        &mut text,
        TextAreaOptions::new().scroll_y(true),
    );
    ui.end_frame();

    let row = ui.boxes[edit.idx].children[0];
    assert_eq!(ui.boxes[row].display_string.as_deref(), Some("## Heading"));
    assert_eq!(ui.boxes[row].debug_label.as_deref(), Some("## Heading"));
}
