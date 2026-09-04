mod app_style;

use mae::{
    imui::{
        CrossAxisAlign, IMUI, MainAxisAlign, MarkdownMode, TextAreaOptions, ThemeKind, UIBoxHandle,
        UISignal, UISize, UITheme, uibox::Color,
    },
    os::{OSEventFlag, OSKey, OSKeyCode},
};
// The renderer-backend dropdown (below) is GL/CPU-specific — there's no
// equivalent concept for the DOM backend, and `Backend` is an uninhabited
// type in a `--no-default-features --features dom` build, so this import
// (and the dropdown code that uses it) only exists for the other targets.
#[cfg(not(target_arch = "wasm32"))]
use mae::render::Backend;

// Material icon codepoints (same font/convention as src/imui/toast.rs).
const ICON_INFO: &str = "\u{e88e}";
const ICON_WARNING: &str = "\u{e002}";
const ICON_DANGER: &str = "\u{e000}";
const ICON_CLOSE: &str = "\u{e5cd}";
const ICON_FOLDER: &str = "\u{e2c7}";

const DEMO_IMAGE_KEY: &str = "demo_gradient";
const DEMO_IMAGE_SIZE: u32 = 96;

/// Registers a small procedural gradient the first time it's needed (checked
/// every frame via `has_image`, which is a cheap HashMap lookup — the actual
/// RGBA buffer is only ever allocated and uploaded once).
fn ensure_demo_image(ui: &mut IMUI) {
    if ui.has_image(DEMO_IMAGE_KEY) {
        return;
    }
    let n = DEMO_IMAGE_SIZE;
    let mut rgba = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let checker = ((x / 12) + (y / 12)) % 2 == 0;
            let r = (x * 255 / n) as u8;
            let g = (y * 255 / n) as u8;
            let b = if checker { 200u8 } else { 90u8 };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    ui.provide_image(DEMO_IMAGE_KEY, n, n, &rgba);
}

fn main() {
    println!(
        "Starting Mae GUI framework demo {}",
        env!("CARGO_PKG_VERSION")
    );

    // The same IMUI and widget code are used for the demo and tests.
    let mut ui = IMUI::new(920, 620, "Mae demo");

    let mut input = String::from("Edit me");
    let mut text = String::from(
        "Mae is now a GUI framework demo.\n\nThis text area exercises text input, children-sum layout, parent-percent layout, and draw command generation.\n\nClick in here and type.",
    );
    // Short, driver-test-friendly seed — `text` above stays long for the
    // showcase (and `www/test_dom_e2e.py` already asserts its exact value),
    // which makes it impractical for `tests/testkit_driver.rs`'s scenario
    // functions to reconstruct the "expected text after this edit" against.
    let mut short_text = String::from("line one\nline two");
    // Seeded with text a browser and mae *count differently*: an accented
    // letter is two UTF-8 bytes and one UTF-16 code unit, an emoji four bytes
    // and two units, while both are a single char to mae. Every offset a
    // browser reports (`selectionStart`, a `Range`) is in UTF-16 units and
    // every offset in mae is a char index, so this is the seed that makes
    // `tests/testkit_driver.rs`'s `emoji_line_edit_text_input_test_all` run
    // the same nine edits over text where the two disagree — on the DOM
    // backend as well as native, which is where they can disagree at all.
    let mut emoji_input = String::from("Caf\u{e9}\u{1F389}me");
    // A `RICH_TEXT_HOST` demo widget (see `widget_section`): the DOM
    // backend hosts this as a `<div contenteditable>` instead of a plain
    // `<textarea>` — exercises that path for CDP-driven testing the same
    // way `text`/`input` above exercise the plain one. `MarkdownMode::
    // Rendered` (below) is global to the `IMUI` instance.
    //
    // Deliberately a *different* string from `short_text` above, same shape
    // (two same-length lines) otherwise: `tests/testkit_driver.rs`'s
    // `UiDriver::click`/`exists` select an element by its current text, so two
    // seed is a real, easy-to-hit test bug: the query matches whichever
    // element happens to come first in DOM order, silently exercising the
    // wrong widget. This is exactly what happened here — every CDP test
    // meant to exercise this box was actually clicking into `short_text`'s
    // plain `<textarea>` instead, until this seed collision was found.
    let mut markdown_text = String::from("note one\nnote two");
    // A rendered-markdown seed carrying the same accent and emoji as
    // `emoji_input` — the rich-text host maps caret offsets between the
    // buffer and the DOM in both directions, so it needs the same coverage
    // over text where char indices and UTF-16 offsets disagree. Distinct
    // text from every other seed, for the reason `markdown_text` above is.
    let mut emoji_markdown_text = String::from("t\u{e9}xt\u{1F389}here");
    // Whether `###demo_markdown_textarea` is `MarkdownMode::Rendered` (hidden
    // markers, a `RICH_TEXT_HOST`) or `::Source` (literal markers, a plain
    // `<textarea>`) — toggled by the button next to it. `markdown_mode` is
    // global to the whole `IMUI` instance (there's only the one markdown
    // textarea here to be affected).
    // Exists so CDP-driven tests can reach `Source` mode at all: the demo's
    // own global mode used to be hardcoded to `Rendered`, with no way for a
    // test driving the real page (as opposed to `NativeDriver`'s own
    // from-scratch widget closure) to ever exercise the other one.
    let mut markdown_rendered = true;
    let mut show_panel = false;
    let mut counter = 0usize;
    let mut right_clicks = 0usize;
    let mut clickable_row_hits = 0usize;
    let mut lazy_rendering = !ui.render_continuously();
    let mut vsync_enabled = ui.vsync_enabled();
    let mut refresh_cap_enabled = ui.cap_fps_to_refresh_rate();
    #[cfg(not(target_arch = "wasm32"))]
    let mut renderer_menu_open = false;
    let mut theme_kind = ThemeKind::Dark;

    // Keep the closure self-contained for the blocking event loop.
    let build = move |ui: &mut IMUI| {
        ui.set_theme(UITheme::for_kind(theme_kind));
        ui.set_markdown_mode(if markdown_rendered {
            MarkdownMode::Rendered
        } else {
            MarkdownMode::Source
        });
        if ui.input(OSKey::Keyboard(OSKeyCode::KeyT), Some(OSEventFlag::Control)) {
            show_panel = !show_panel;
        }

        let root = ui.column(|ui| {
            let content = ui.column(|ui| {
                let header = ui.row(|ui| {
                    let title = ui.label("Mae — GUI framework demo");
                    app_style::title(ui, title)
                        .width(ui, UISize::Fill)
                        .height(ui, UISize::Pixels(36.0));

                    let fps_text = if ui.fps() > 0.0 {
                        format!("{:.0} fps", ui.fps())
                    } else {
                        "fps --".to_string()
                    };
                    let fps = ui.label(&fps_text);
                    app_style::muted(ui, fps)
                        .width(ui, UISize::Pixels(64.0))
                        .height(ui, UISize::Pixels(30.0));

                    let theme_switch = toggle_switch(
                        ui,
                        match theme_kind {
                            ThemeKind::Dark => "Dark##theme_toggle",
                            ThemeKind::Light => "Light##theme_toggle",
                        },
                        theme_kind == ThemeKind::Light,
                        "Toggle light/dark theme",
                    );
                    if theme_switch.clicked() {
                        theme_kind = match theme_kind {
                            ThemeKind::Dark => ThemeKind::Light,
                            ThemeKind::Light => ThemeKind::Dark,
                        };
                        ui.set_theme(UITheme::for_kind(theme_kind));
                    }

                    let lazy_switch = toggle_switch(
                        ui,
                        "Lazy rendering##render_mode_toggle",
                        lazy_rendering,
                        "Toggle frame pacing",
                    );
                    if lazy_switch.clicked() {
                        lazy_rendering = !lazy_rendering;
                        ui.set_render_continuously(!lazy_rendering);
                    }

                    let vsync_switch =
                        toggle_switch(ui, "Vsync##vsync_toggle", vsync_enabled, "Toggle vsync");
                    if vsync_switch.clicked() {
                        vsync_enabled = !vsync_enabled;
                        ui.set_vsync_enabled(vsync_enabled);
                    }

                    let refresh_cap_label =
                        format!("Cap {:.0}Hz##refresh_cap_toggle", ui.refresh_rate_hz());
                    let refresh_cap_switch = toggle_switch(
                        ui,
                        &refresh_cap_label,
                        refresh_cap_enabled,
                        "Cap maximum FPS to the screen refresh rate",
                    );
                    if refresh_cap_switch.clicked() {
                        refresh_cap_enabled = !refresh_cap_enabled;
                        ui.set_cap_fps_to_refresh_rate(refresh_cap_enabled);
                    }

                    // No equivalent to a GL/CPU backend switch exists for the DOM
                    // backend (`ui.renderer_backend()` is uncallable in that build —
                    // `Backend` has zero variants with neither `opengl` nor `cpu`
                    // compiled in), so this dropdown only exists on the other targets.
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let current_backend = ui.renderer_backend();
                        let control_h = ui.theme().control_h;
                        let renderer_button = ui
                            .button(
                                &format!(
                                    "Renderer: {}##renderer_dropdown",
                                    current_backend.as_str()
                                ),
                                Some("Select the active renderer"),
                            )
                            .width(ui, UISize::Pixels(150.0))
                            .height(ui, UISize::Pixels(control_h));
                        if renderer_button.clicked() {
                            renderer_menu_open = !renderer_menu_open;
                        }

                        if renderer_menu_open {
                            let trigger_bounds = ui.bounds(renderer_button);
                            let menu_width = 180.0;
                            let menu = ui.floating_pane_at(
                                mae::imui::Point::new(
                                    trigger_bounds.x1 - menu_width,
                                    trigger_bounds.y1 + 6.0,
                                ),
                                Some("###renderer_menu"),
                                |ui| {
                                    let theme = *ui.theme();
                                    for backend in Backend::available() {
                                        let option = ui
                                            .button(
                                                &format!(
                                                    "{}{}##renderer_option_{:?}",
                                                    if backend == current_backend {
                                                        "* "
                                                    } else {
                                                        ""
                                                    },
                                                    backend.as_str(),
                                                    backend
                                                ),
                                                None,
                                            )
                                            .width(ui, UISize::Pixels(menu_width))
                                            .height(ui, UISize::Pixels(30.0))
                                            .background(
                                                ui,
                                                if backend == current_backend {
                                                    theme.accent
                                                } else {
                                                    theme.surface_bg
                                                },
                                            );
                                        if option.clicked() {
                                            ui.set_renderer_backend(backend);
                                            vsync_enabled = ui.vsync_enabled();
                                            refresh_cap_enabled = ui.cap_fps_to_refresh_rate();
                                            renderer_menu_open = false;
                                        }
                                    }
                                },
                            );
                            app_style::popover(ui, menu);
                        }
                    }
                });
                app_style::toolbar(ui, header)
                    .width(ui, UISize::Fill)
                    .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center);

                // All four sections stacked in one continuously scrollable
                // body (the sidebar/tab-switcher this demo used to have is
                // gone — everything is visible together instead). Needs a
                // stable id: an anonymous `ui.column` gets a brand new,
                // never-before-seen box every single frame (no identity to
                // persist across frames by), so its `scroll`/`scroll_target`
                // would silently reset to zero on every rebuild — wheel
                // input would never accumulate into a visible scroll.
                let body = ui.named_column("###demo_body_scroll", |ui| {
                    section_heading(ui, "Layout");
                    layout_section(ui, &mut show_panel);

                    section_heading(ui, "Widgets");
                    widget_section(
                        ui,
                        &mut input,
                        &mut text,
                        &mut short_text,
                        &mut emoji_input,
                        &mut markdown_text,
                        &mut emoji_markdown_text,
                        &mut markdown_rendered,
                        &mut counter,
                        &mut right_clicks,
                        &mut clickable_row_hits,
                    );

                    section_heading(ui, "Render");
                    render_section(ui);

                    section_heading(ui, "Scroll");
                    scroll_section(ui);
                });
                body.width(ui, UISize::Fill)
                    .height(ui, UISize::Fill)
                    .gap(ui, ui.theme().gap_md)
                    .scroll_y(ui, true)
                    .clip(ui, true);
            });
            app_style::content(ui, content);
        });
        app_style::app_root(ui, root);
    };

    ui.eventloop(build);
}

fn toggle_switch(ui: &mut IMUI, label: &str, enabled: bool, tooltip: &str) -> UIBoxHandle {
    let button = ui.button(label, Some(tooltip));
    app_style::toggle(ui, button, enabled)
}

fn layout_section(ui: &mut IMUI, show_panel: &mut bool) {
    let page = ui.column(|ui| {
        let controls = ui.row(|ui| {
            let control_h = ui.theme().control_h;
            let toggle = ui
                .button("Toggle panel", Some("Ctrl+T"))
                .width(ui, UISize::Pixels(140.0))
                .height(ui, UISize::Pixels(control_h));
            app_style::button(ui, toggle);
            if toggle.clicked() {
                *show_panel = !*show_panel;
            }

            let status = ui.label(if *show_panel {
                "Panel visible"
            } else {
                "Panel hidden"
            });
            app_style::muted(ui, status)
            .height(ui, UISize::Pixels(28.0));
        });
        controls
            .height(ui, UISize::Pixels(36.0))
            .gap(ui, 10.0)
            .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center);

        let body = ui.row(|ui| {
            let left = ui.column(|ui| {
                section_title(ui, "Sizing");
                sizing_demo(ui);
            });
            let surface_bg = ui.theme().surface_bg;
            left.height(ui, UISize::Pixels(340.0))
                .background(ui, surface_bg);
            app_style::panel(ui, left);

            if *show_panel {
                left.width(ui, UISize::ParentPct(0.62));
                let right = ui.column(|ui| {
                    section_title(ui, "Floating-friendly");
                    ui.label("This panel is a normal container in the demo, but it uses the same fixed-position support as floating panes and tooltips.");
                });
                right
                    .width(ui, UISize::Fill)
                    .height(ui, UISize::ParentPct(1.0));
                app_style::panel_alt(ui, right);
            } else {
                left.width(ui, UISize::Fill);
            }
        });
        body.width(ui, UISize::ParentPct(1.0))
            .height(ui, UISize::ChildrenSum)
            .gap(ui, 12.0);
    });
    page.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::ChildrenSum)
        .gap(ui, 12.0);
}

/// One labeled, colored bar per [`UISize`] variant, each actually sized the
/// way its label describes — a visual complement to the API rather than a
/// name/description text table.
fn sizing_demo(ui: &mut IMUI) {
    let radius = ui.theme().radius;
    let track_bg = ui.theme().surface_active;

    let track = ui.row(|ui| {
        let bar = ui.label("Pixels: 150px");
        bar.width(ui, UISize::Pixels(150.0))
            .height(ui, UISize::ParentPct(1.0))
            .background(ui, Color::new("#6fa8d7"))
            .corner_radius(ui, radius)
            .text_center(ui, true);
    });
    track
        .width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Pixels(40.0))
        .padding_all(ui, 4.0)
        .background(ui, track_bg)
        .corner_radius(ui, radius);

    let track = ui.row(|ui| {
        let bar = ui.label("ParentPct: 50% of this track");
        bar.width(ui, UISize::ParentPct(0.5))
            .height(ui, UISize::ParentPct(1.0))
            .background(ui, Color::new("#75b878"))
            .corner_radius(ui, radius)
            .text_center(ui, true);
    });
    track
        .width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Pixels(40.0))
        .padding_all(ui, 4.0)
        .background(ui, track_bg)
        .corner_radius(ui, radius);

    let track = ui.row(|ui| {
        // The bar (a container, not the label itself — a leaf label is
        // already text-sized) hugs exactly its child label plus padding.
        let bar = ui.row(|ui| {
            ui.label("ChildrenSum: hugs its content");
        });
        bar.width(ui, UISize::ChildrenSum)
            .height(ui, UISize::ParentPct(1.0))
            .padding(ui, 0.0, 14.0, 0.0, 14.0)
            .background(ui, Color::new("#d7b56f"))
            .corner_radius(ui, radius)
            .align(ui, MainAxisAlign::Center, CrossAxisAlign::Center);
    });
    track
        .width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Pixels(40.0))
        .padding_all(ui, 4.0)
        .background(ui, track_bg)
        .corner_radius(ui, radius);

    let neutral_bg = ui.theme().surface_bg;
    let track = ui.row(|ui| {
        let anchor = ui.label("Pixels: 90px");
        anchor
            .width(ui, UISize::Pixels(90.0))
            .height(ui, UISize::ParentPct(1.0))
            .background(ui, neutral_bg)
            .corner_radius(ui, radius)
            .text_center(ui, true);

        let bar = ui.label("Fill: remaining space");
        bar.width(ui, UISize::Fill)
            .height(ui, UISize::ParentPct(1.0))
            .background(ui, Color::new("#b074d7"))
            .corner_radius(ui, radius)
            .text_center(ui, true);
    });
    track
        .width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Pixels(40.0))
        .padding_all(ui, 4.0)
        .gap(ui, 6.0)
        .background(ui, track_bg)
        .corner_radius(ui, radius);
}

fn widget_section(
    ui: &mut IMUI,
    input: &mut String,
    text: &mut String,
    short_text: &mut String,
    emoji_input: &mut String,
    markdown_text: &mut String,
    emoji_markdown_text: &mut String,
    markdown_rendered: &mut bool,
    counter: &mut usize,
    right_clicks: &mut usize,
    clickable_row_hits: &mut usize,
) {
    let body = ui.column(|ui| {
        section_title(ui, "Signals");
        let signal_row = ui.row(|ui| {
            let button = ui
                .button(
                    "Click target",
                    Some(
                        "Reports press/hover/click signal state; right-click to test RIGHT_CLICKED",
                    ),
                )
                .width(ui, UISize::Pixels(160.0))
                .height(ui, UISize::Pixels(36.0));
            app_style::button(ui, button);
            if button.right_clicked() {
                *right_clicks += 1;
            }
            let report = format!("{} right_clicks={right_clicks}", signal_report(button));
            ui.label(&report);
        });
        signal_row
            .height(ui, UISize::Pixels(42.0))
            .gap(ui, 10.0)
            .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center);

        let counter_row = ui.row(|ui| {
            let counter_label = ui.label(&format!("Counter: {}", *counter));
            app_style::accent_text(ui, counter_label).height(ui, UISize::Pixels(28.0));

            let plus = ui
                .button("+", Some("Increment counter"))
                .width(ui, UISize::Pixels(36.0))
                .height(ui, UISize::Pixels(32.0));
            app_style::button(ui, plus);
            if plus.clicked() {
                *counter += 1;
            }
        });
        counter_row
            .height(ui, UISize::Pixels(36.0))
            .gap(ui, 8.0)
            .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center);

        // `clickable_row`: a MOUSE_CLICKABLE container with no DRAW_HOT_EFFECTS
        // styling of its own — exercises the DOM backend's pointer-events
        // gating fix (a clickable box that isn't a `<button>`-styled hot
        // element must still be a native hit-test target). The hit count is
        // a separate label built *after* `row.clicked()` is checked, not a
        // child of the row itself — a child's text is built before the row
        // handle (and thus `.clicked()`) is available, which would leave
        // the displayed count one interaction stale.
        let row = ui.clickable_row("###clickable_row_demo", |ui| {
            let label = ui.label("Click anywhere in this row");
            app_style::muted(ui, label).width(ui, UISize::Fill);
        });
        let surface_bg = ui.theme().surface_bg;
        row.width(ui, UISize::ParentPct(1.0))
            .height(ui, UISize::Pixels(32.0))
            .padding_all(ui, 6.0)
            .background(ui, surface_bg)
            .corner_radius(ui, ui.theme().radius)
            .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center);
        if row.clicked() {
            *clickable_row_hits += 1;
        }
        ui.label(&format!("Row hits: {clickable_row_hits}"));

        section_title(ui, "Text Input");
        ui.line_edit("###demo_line_edit", input, false)
            .height(ui, UISize::Pixels(34.0));

        ui.textarea("###demo_textarea", text)
            .height(ui, UISize::Pixels(120.0));

        // Short-seeded plain textarea — see `short_text`'s doc comment.
        ui.textarea("###demo_short_textarea", short_text)
            .height(ui, UISize::Pixels(60.0));

        // Non-BMP/accented seed — see `emoji_input`'s doc comment.
        ui.line_edit("###demo_emoji_line_edit", emoji_input, false)
            .height(ui, UISize::Pixels(34.0));

        // Rendered-markdown textarea (`MarkdownMode::Rendered` by default,
        // toggled below) — in that mode the DOM backend hosts this as a
        // rendered markdown editor, exercised by the CDP scenarios as well as
        // the native test harness.
        let markdown_mode_label = if *markdown_rendered {
            "Markdown: Rendered###markdown_mode_toggle"
        } else {
            "Markdown: Source###markdown_mode_toggle"
        };
        if ui.button(markdown_mode_label, None).clicked() {
            *markdown_rendered = !*markdown_rendered;
        }
        ui.markdown_textarea_with_options(
            "###demo_markdown_textarea",
            markdown_text,
            TextAreaOptions::default(),
        )
        .height(ui, UISize::Pixels(120.0));

        // Non-BMP/accented seed — see `emoji_markdown_text`'s doc comment.
        ui.markdown_textarea_with_options(
            "###demo_emoji_markdown_textarea",
            emoji_markdown_text,
            TextAreaOptions::default(),
        )
        .height(ui, UISize::Pixels(80.0));

        section_title(ui, "Icons");
        let icon_row = ui.row(|ui| {
            for (glyph, id, tooltip) in [
                (ICON_INFO, "icon_info", "Info"),
                (ICON_WARNING, "icon_warning", "Warning"),
                (ICON_DANGER, "icon_danger", "Danger"),
                (ICON_CLOSE, "icon_close", "Close"),
                (ICON_FOLDER, "icon_folder", "Folder"),
            ] {
                ui.button_icon(&format!("{glyph}##{id}"), Some(tooltip));
            }
            // A non-interactive icon glyph, larger and tinted, alongside the
            // clickable ones above.
            let accent = ui.theme().accent;
            ui.icon_label(ICON_INFO)
                .width(ui, UISize::Pixels(40.0))
                .height(ui, UISize::Pixels(40.0))
                .font_size(ui, 32.0)
                .text_color(ui, accent);
        });
        icon_row
            .height(ui, UISize::Pixels(40.0))
            .gap(ui, 8.0)
            .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center);

        section_title(ui, "Image");
        ensure_demo_image(ui);
        ui.image("###demo_image", DEMO_IMAGE_KEY)
            .width(ui, UISize::Pixels(96.0))
            .height(ui, UISize::Pixels(96.0));
    });
    let surface_bg = ui.theme().surface_bg;
    body.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::ChildrenSum)
        .background(ui, surface_bg);
    app_style::panel(ui, body);
}

fn scroll_section(ui: &mut IMUI) {
    let body = ui.named_column("###scroll_section_body", |ui| {
        section_title(ui, "Vertical list (scroll wheel or drag the scrollbar)");
        let list = ui.named_column("###vertical_scroll_list", |ui| {
            for i in 0..40 {
                let row = ui.row(|ui| {
                    let label = ui.label(&format!("Row {i}"));
                    app_style::muted(ui, label).width(ui, UISize::Fill);
                    if i % 7 == 0 {
                        ui.icon_label(ICON_INFO)
                            .width(ui, UISize::Pixels(20.0))
                            .height(ui, UISize::Pixels(20.0));
                    }
                });
                row.width(ui, UISize::Fill)
                    .height(ui, UISize::Pixels(28.0))
                    .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center);
            }
        });
        let surface_bg = ui.theme().surface_bg;
        list.width(ui, UISize::ParentPct(1.0))
            .height(ui, UISize::Pixels(180.0))
            .background(ui, surface_bg)
            .scroll_y(ui, true)
            .clip(ui, true);
        app_style::panel(ui, list);

        // Alt (not Shift) is what `scroll.rs::absorb_pending_scroll_for_box`
        // reroutes a vertical wheel into a horizontal scroll with — see
        // `has_flag(ev.flags, OSEventFlag::Alt)`.
        section_title(ui, "Horizontal strip (alt+wheel or drag the scrollbar)");
        // Enough cards to overflow the strip even on a very wide/large
        // display, so the scrolling behavior stays demonstrable regardless
        // of window size.
        let strip = ui.named_row("###horizontal_scroll_strip", |ui| {
            for i in 0..80 {
                let card = ui.column(|ui| {
                    let label = ui.label(&format!("Card {i}"));
                    app_style::accent_text(ui, label);
                });
                let surface_bg = ui.theme().surface_active;
                card.width(ui, UISize::Pixels(96.0))
                    .height(ui, UISize::ParentPct(1.0))
                    .background(ui, surface_bg)
                    .corner_radius(ui, ui.theme().radius)
                    .padding_all(ui, 10.0);
            }
        });
        let surface_bg = ui.theme().surface_bg;
        strip
            .width(ui, UISize::ParentPct(1.0))
            .height(ui, UISize::Pixels(120.0))
            .gap(ui, 8.0)
            .background(ui, surface_bg)
            .scroll_x(ui, true)
            .clip(ui, true);
        app_style::panel(ui, strip);
    });
    let surface_bg = ui.theme().surface_bg;
    body.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::ChildrenSum)
        .gap(ui, 12.0)
        .background(ui, surface_bg);
    app_style::panel(ui, body);
}

fn render_section(ui: &mut IMUI) {
    let body = ui.column(|ui| {
        section_title(ui, "Draw Layer");
        ui.label("The UI tree emits rectangles, borders, and text through the draw layer before the renderer backend consumes batches.");

        let swatches = ui.row(|ui| {
            for color in ["#d76f6f", "#d7b56f", "#75b878", "#6fa8d7", "#b074d7"] {
                ui.label("")
                    .width(ui, UISize::Fill)
                    .height(ui, UISize::Pixels(84.0))
                    .background(ui, Color::new(color));
            }
        });
        swatches
            .width(ui, UISize::ParentPct(1.0))
            .height(ui, UISize::Pixels(92.0))
            .gap(ui, 8.0);
    });
    let surface_bg = ui.theme().surface_bg;
    body.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::ChildrenSum)
        .background(ui, surface_bg);
    app_style::panel(ui, body);
}

/// A top-level section header (Layout/Widgets/Render/Scroll) — bigger than
/// [`section_title`], which marks sub-sections within one.
fn section_heading(ui: &mut IMUI, text: &str) -> UIBoxHandle {
    let label = ui.label(text);
    app_style::title(ui, label)
        .height(ui, UISize::Pixels(40.0))
        .font_size(ui, 22.0)
}

fn section_title(ui: &mut IMUI, text: &str) -> UIBoxHandle {
    let label = ui.label(text);
    app_style::title(ui, label).height(ui, UISize::Pixels(30.0))
}

fn signal_report(handle: UIBoxHandle) -> String {
    let signal: UISignal = handle.signal();
    format!(
        "pressed={} clicked={} dragging={} hover={}",
        signal.pressed(),
        signal.clicked(),
        signal.dragging(),
        signal.hovering()
    )
}
