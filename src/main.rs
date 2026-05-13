use mae::{
    imui::{CrossAxisAlign, IMUI, MainAxisAlign, UIBoxHandle, UISize, UiSignal, uibox::Color},
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
    let mut counter = 0usize;
    let mut lazy_rendering = !ui.render_continuously();
    let mut vsync_enabled = ui.vsync_enabled();
    let mut renderer_menu_open = false;

    ui.eventloop(|ui| {
        if ui.input(OSKey::Keyboard(OSKeyCode::KeyT), Some(OSEventFlag::Control)) {
            show_panel = !show_panel;
        }

        let root = ui.row(|ui| {
            let sidebar = ui.column(|ui| {
                let title = ui.label("Mae");
                ui.text_color(title, Color::new("#ffffff"));
                ui.height(title, UISize::Pixels(34.0));

                let subtitle = ui.label("GUI framework");
                ui.text_color(subtitle, Color::new("#9aa4af"));
                ui.height(subtitle, UISize::Pixels(28.0));

                let nav = ui.named_column("###nav", |ui| {
                    nav_button(ui, "Layout", 0, &mut selected_tab);
                    nav_button(ui, "Widgets", 1, &mut selected_tab);
                    nav_button(ui, "Render", 2, &mut selected_tab);
                });
                ui.gap(nav, 6.0);
            });
            ui.width(sidebar, UISize::Pixels(210.0));
            ui.height(sidebar, UISize::ParentPct(1.0));
            ui.padding_all(sidebar, 16.0);
            ui.gap(sidebar, 8.0);
            ui.background(sidebar, Color::new("#20242b"));

            let content = ui.column(|ui| {
                let header = ui.row(|ui| {
                    let heading = ui.label(match selected_tab {
                        0 => "Layout Review",
                        1 => "Widget Signals",
                        _ => "Render Commands",
                    });
                    ui.text_color(heading, Color::new("#ffffff"));
                    ui.width(heading, UISize::Fill);
                    ui.height(heading, UISize::Pixels(36.0));

                    let fps_text = if ui.fps() > 0.0 {
                        format!("{:.0} fps", ui.fps())
                    } else {
                        "fps --".to_string()
                    };
                    let fps = ui.label(&fps_text);
                    ui.width(fps, UISize::Pixels(64.0));
                    ui.height(fps, UISize::Pixels(30.0));
                    ui.text_color(fps, Color::new("#9aa4af"));

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
                    let renderer_button = ui.button(
                        &format!("Renderer: {}##renderer_dropdown", current_backend.label()),
                        Some("Select the active renderer"),
                    );
                    ui.width(renderer_button, UISize::Pixels(150.0));
                    ui.height(renderer_button, UISize::Pixels(32.0));
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
                                for backend in Backend::available() {
                                    let option = ui.button(
                                        &format!(
                                            "{}{}##renderer_option_{:?}",
                                            if backend == current_backend { "* " } else { "" },
                                            backend.label(),
                                            backend
                                        ),
                                        None,
                                    );
                                    ui.width(option, UISize::Pixels(menu_width));
                                    ui.height(option, UISize::Pixels(30.0));
                                    ui.background(
                                        option,
                                        if backend == current_backend {
                                            Color::new("#2f8f83")
                                        } else {
                                            Color::new("#2a3038")
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
                        ui.padding_all(menu, 6.0);
                        ui.gap(menu, 4.0);
                        ui.background(menu, Color::new("#1b2028"));
                        ui.border_color(menu, Color::new("#48515d"));
                    }
                });
                ui.height(header, UISize::Pixels(46.0));
                ui.align(header, MainAxisAlign::Start, CrossAxisAlign::Center);

                match selected_tab {
                    0 => layout_page(ui, &mut show_panel),
                    1 => widget_page(ui, &mut input, &mut text, &mut counter),
                    _ => render_page(ui),
                }
            });
            ui.width(content, UISize::Fill);
            ui.height(content, UISize::ParentPct(1.0));
            ui.padding_all(content, 14.0);
            ui.gap(content, 12.0);
            ui.background(content, Color::new("#15191f"));
        });
        ui.width(root, UISize::ParentPct(1.0));
        ui.height(root, UISize::ParentPct(1.0));
        ui.padding_all(root, 10.0);
        ui.gap(root, 10.0);
        ui.background(root, Color::new("#101318"));
    });
}

fn toggle_switch(ui: &mut IMUI, label: &str, enabled: bool, tooltip: &str) -> UIBoxHandle {
    let button = ui.button(label, Some(tooltip));
    ui.background(
        button,
        if enabled {
            Color::new("#2f8f83")
        } else {
            Color::new("#39414c")
        },
    );
    ui.height(button, UISize::Pixels(32.0));
    button
}

fn nav_button(ui: &mut IMUI, label: &str, id: usize, selected_tab: &mut usize) {
    let button = ui.button(&format!("{label}##nav_{id}"), None);
    ui.width(button, UISize::ParentPct(1.0));
    ui.height(button, UISize::Pixels(34.0));
    ui.background(
        button,
        if *selected_tab == id {
            Color::new("#2f8f83")
        } else {
            Color::new("#2a3038")
        },
    );
    if button.clicked() {
        *selected_tab = id;
    }
}

fn layout_page(ui: &mut IMUI, show_panel: &mut bool) {
    let page = ui.column(|ui| {
        let controls = ui.row(|ui| {
            let toggle = ui.button("Toggle panel", Some("Ctrl+T"));
            ui.width(toggle, UISize::Pixels(140.0));
            ui.height(toggle, UISize::Pixels(32.0));
            if toggle.clicked() {
                *show_panel = !*show_panel;
            }

            let state = ui.label(if *show_panel {
                "Panel visible"
            } else {
                "Panel hidden"
            });
            ui.height(state, UISize::Pixels(28.0));
            ui.text_color(state, Color::new("#9aa4af"));
        });
        ui.height(controls, UISize::Pixels(36.0));
        ui.gap(controls, 10.0);
        ui.align(controls, MainAxisAlign::Start, CrossAxisAlign::Center);

        let body = ui.row(|ui| {
            let left = ui.column(|ui| {
                section_title(ui, "Sizing");
                metric_row(ui, "Pixels", "Fixed sizes");
                metric_row(ui, "ParentPct", "Relative to parent");
                metric_row(ui, "ChildrenSum", "Content driven");
                metric_row(ui, "Fill", "Remaining space");
            });
            ui.height(left, UISize::ParentPct(1.0));
            ui.padding_all(left, 14.0);
            ui.gap(left, 8.0);
            ui.background(left, Color::new("#242a32"));

            if *show_panel {
                ui.width(left, UISize::ParentPct(0.62));
                let right = ui.column(|ui| {
                    section_title(ui, "Floating-friendly");
                    ui.label("This panel is a normal container in the demo, but it uses the same fixed-position support as floating panes and tooltips.");
                });
                ui.width(right, UISize::Fill);
                ui.height(right, UISize::ParentPct(1.0));
                ui.padding_all(right, 14.0);
                ui.gap(right, 8.0);
                ui.background(right, Color::new("#1d3434"));
            } else {
                ui.width(left, UISize::Fill);
            }
        });
        ui.width(body, UISize::ParentPct(1.0));
        ui.height(body, UISize::Fill);
        ui.gap(body, 12.0);
    });
    ui.width(page, UISize::ParentPct(1.0));
    ui.height(page, UISize::Fill);
    ui.gap(page, 12.0);
}

fn widget_page(ui: &mut IMUI, input: &mut String, text: &mut String, counter: &mut usize) {
    let body = ui.column(|ui| {
        section_title(ui, "Signals");
        let signal_row = ui.row(|ui| {
            let button = ui.button(
                "Click target",
                Some("Reports press/hover/click signal state"),
            );
            ui.width(button, UISize::Pixels(160.0));
            ui.height(button, UISize::Pixels(36.0));
            let report = signal_report(button);
            ui.label(&report);
        });
        ui.height(signal_row, UISize::Pixels(42.0));
        ui.gap(signal_row, 10.0);
        ui.align(signal_row, MainAxisAlign::Start, CrossAxisAlign::Center);

        let counter_row = ui.row(|ui| {
            let counter_label = ui.label(&format!("Counter: {}", *counter));
            ui.height(counter_label, UISize::Pixels(28.0));
            ui.text_color(counter_label, Color::new("#9fc8ff"));

            let plus = ui.button("+", Some("Increment counter"));
            ui.width(plus, UISize::Pixels(36.0));
            ui.height(plus, UISize::Pixels(32.0));
            if plus.clicked() {
                *counter += 1;
            }
        });
        ui.height(counter_row, UISize::Pixels(36.0));
        ui.gap(counter_row, 8.0);
        ui.align(counter_row, MainAxisAlign::Start, CrossAxisAlign::Center);

        section_title(ui, "Text Input");
        let edit = ui.line_edit("###demo_line_edit", input, false);
        ui.height(edit, UISize::Pixels(34.0));

        let area = ui.textarea("###demo_textarea", text);
        ui.height(area, UISize::Fill);
    });
    ui.width(body, UISize::ParentPct(1.0));
    ui.height(body, UISize::Fill);
    ui.padding_all(body, 14.0);
    ui.gap(body, 10.0);
    ui.background(body, Color::new("#242a32"));
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
                let swatch = ui.label(&format!("##swatch_{idx}"));
                ui.width(swatch, UISize::Fill);
                ui.height(swatch, UISize::Pixels(84.0));
                ui.background(swatch, Color::new(color));
            }
        });
        ui.height(swatches, UISize::Pixels(92.0));
        ui.gap(swatches, 8.0);
    });
    ui.width(body, UISize::ParentPct(1.0));
    ui.height(body, UISize::Fill);
    ui.padding_all(body, 14.0);
    ui.gap(body, 12.0);
    ui.background(body, Color::new("#242a32"));
}

fn section_title(ui: &mut IMUI, text: &str) -> UIBoxHandle {
    let title = ui.label(text);
    ui.text_color(title, Color::new("#ffffff"));
    ui.height(title, UISize::Pixels(30.0));
    title
}

fn metric_row(ui: &mut IMUI, name: &str, value: &str) {
    let row = ui.row(|ui| {
        let name = ui.label(name);
        ui.width(name, UISize::Pixels(120.0));
        ui.text_color(name, Color::new("#9fc8ff"));

        let value = ui.label(value);
        ui.width(value, UISize::Fill);
        ui.text_color(value, Color::new("#c8ced6"));
    });
    ui.height(row, UISize::Pixels(30.0));
    ui.align(row, MainAxisAlign::Start, CrossAxisAlign::Center);
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
