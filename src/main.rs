mod app_style;
mod ide_window;

use mae::{
    imui::{
        CrossAxisAlign, IMUI, MainAxisAlign, ThemeKind, UIBoxHandle, UISize, UITheme, UiSignal,
        uibox::Color,
    },
    os::{OSEventFlag, OSKey, OSKeyCode},
    render::Backend,
};

fn main() {
    println!(
        "Starting Mae GUI framework demo {}",
        env!("CARGO_PKG_VERSION")
    );

    let mut ui = IMUI::new(920, 620);
    let mut input = String::from("Edit me");
    let mut text = String::from(
        "Mae is now a GUI framework demo.\n\nThis text area exercises text input, children-sum layout, parent-percent layout, and draw command generation.\n\nClick in here and type.",
    );
    let mut show_panel = true;
    let mut selected_tab = 0usize;
    let mut ide_state = ide_window::IdeViewState::new();
    let mut counter = 0usize;
    let mut lazy_rendering = !ui.render_continuously();
    let mut vsync_enabled = ui.vsync_enabled();
    let mut renderer_menu_open = false;
    let mut theme_kind = ThemeKind::Dark;

    ui.eventloop(|ui| {
        ui.set_theme(UITheme::for_kind(theme_kind));
        if ui.input(OSKey::Keyboard(OSKeyCode::KeyT), Some(OSEventFlag::Control)) {
            show_panel = !show_panel;
        }

        if selected_tab == 3 {
            if ide_window::render(ui, &mut ide_state) {
                selected_tab = 0;
            }
            return;
        }

        let root = ui.row(|ui| {
            let sidebar = ui.column(|ui| {
                let brand = ui.label("Mae");
                app_style::title(ui, brand).height(ui, UISize::Pixels(34.0));

                let subtitle = ui.label("GUI framework");
                app_style::muted(ui, subtitle).height(ui, UISize::Pixels(28.0));

                let nav = ui.named_column("###nav", |ui| {
                    nav_button(ui, "Layout", 0, &mut selected_tab);
                    nav_button(ui, "Widgets", 1, &mut selected_tab);
                    nav_button(ui, "Render", 2, &mut selected_tab);
                    nav_button(ui, "IDE", 3, &mut selected_tab);
                });
                let gap_sm = ui.theme().gap_sm;
                nav.gap(ui, gap_sm);
            });
            app_style::sidebar(ui, sidebar);

            let content = ui.column(|ui| {
                let header = ui.row(|ui| {
                    let title = ui.label(match selected_tab {
                        0 => "Layout Review",
                        1 => "Widget Signals",
                        2 => "Render Commands",
                        _ => "IDE Mock",
                    });
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

                    let current_backend = ui.renderer_backend();
                    let control_h = ui.theme().control_h;
                    let renderer_button = ui
                        .button(
                            &format!("Renderer: {}##renderer_dropdown", current_backend.label()),
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
                                                if backend == current_backend { "* " } else { "" },
                                                backend.label(),
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
                                        renderer_menu_open = false;
                                    }
                                }
                            },
                        );
                        app_style::popover(ui, menu);
                    }
                });
                app_style::toolbar(ui, header).align(
                    ui,
                    MainAxisAlign::Start,
                    CrossAxisAlign::Center,
                );

                match selected_tab {
                    0 => layout_page(ui, &mut show_panel),
                    1 => widget_page(ui, &mut input, &mut text, &mut counter),
                    2 => render_page(ui),
                    _ => {}
                }
            });
            app_style::content(ui, content);
        });
        app_style::app_root(ui, root);
    });
}

fn toggle_switch(ui: &mut IMUI, label: &str, enabled: bool, tooltip: &str) -> UIBoxHandle {
    let button = ui.button(label, Some(tooltip));
    app_style::toggle(ui, button, enabled)
}

fn nav_button(ui: &mut IMUI, label: &str, id: usize, selected_tab: &mut usize) {
    let raw = ui.button(&format!("{label}##nav_{id}"), None);
    let button = app_style::nav_item(ui, raw, *selected_tab == id);
    if button.clicked() {
        *selected_tab = id;
    }
}

fn layout_page(ui: &mut IMUI, show_panel: &mut bool) {
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
                metric_row(ui, "Pixels", "Fixed sizes");
                metric_row(ui, "ParentPct", "Relative to parent");
                metric_row(ui, "ChildrenSum", "Content driven");
                metric_row(ui, "Fill", "Remaining space");
            });
            let surface_bg = ui.theme().surface_bg;
            left.height(ui, UISize::ParentPct(1.0))
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
            .height(ui, UISize::Fill)
            .gap(ui, 12.0);
    });
    page.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Fill)
        .gap(ui, 12.0);
}

fn widget_page(ui: &mut IMUI, input: &mut String, text: &mut String, counter: &mut usize) {
    let body = ui.column(|ui| {
        section_title(ui, "Signals");
        let signal_row = ui.row(|ui| {
            let button = ui
                .button(
                    "Click target",
                    Some("Reports press/hover/click signal state"),
                )
                .width(ui, UISize::Pixels(160.0))
                .height(ui, UISize::Pixels(36.0));
            app_style::button(ui, button);
            let report = signal_report(button);
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

        section_title(ui, "Text Input");
        ui.line_edit("###demo_line_edit", input, false)
            .height(ui, UISize::Pixels(34.0));

        ui.textarea("###demo_textarea", text)
            .height(ui, UISize::Fill);
    });
    let surface_bg = ui.theme().surface_bg;
    body.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Fill)
        .background(ui, surface_bg);
    app_style::panel(ui, body);
}

fn render_page(ui: &mut IMUI) {
    let body = ui.column(|ui| {
        section_title(ui, "Draw Layer");
        ui.label("The UI tree emits rectangles, borders, and text through the draw layer before the renderer backend consumes batches.");

        let swatches = ui.row(|ui| {
            for (idx, color) in ["#d76f6f", "#d7b56f", "#75b878", "#6fa8d7", "#b074d7"]
                .iter()
                .enumerate()
            {
                ui.label(&format!("##swatch_{idx}"))
                    .width(ui, UISize::Fill)
                    .height(ui, UISize::Pixels(84.0))
                    .background(ui, Color::new(color));
            }
        });
        swatches
            .height(ui, UISize::Pixels(92.0))
            .gap(ui, 8.0);
    });
    let surface_bg = ui.theme().surface_bg;
    body.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Fill)
        .background(ui, surface_bg);
    app_style::panel(ui, body);
}

fn section_title(ui: &mut IMUI, text: &str) -> UIBoxHandle {
    let label = ui.label(text);
    app_style::title(ui, label).height(ui, UISize::Pixels(30.0))
}

fn metric_row(ui: &mut IMUI, name: &str, value: &str) {
    let row = ui.row(|ui| {
        let name_label = ui.label(name);
        app_style::accent_text(ui, name_label)
            .width(ui, UISize::Pixels(120.0))
            .height(ui, UISize::Pixels(24.0));

        let value_label = ui.label(value);
        app_style::muted(ui, value_label).width(ui, UISize::Fill);
    });
    row.height(ui, UISize::Pixels(30.0))
        .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center);
}

fn signal_report(handle: UIBoxHandle) -> String {
    let signal: UiSignal = handle.signal();
    format!(
        "pressed={} clicked={} dragging={} hover={}",
        signal.pressed(),
        signal.clicked(),
        signal.dragging(),
        signal.hovering()
    )
}
