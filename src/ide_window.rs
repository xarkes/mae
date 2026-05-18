use mae::imui::{
    CrossAxisAlign, IMUI, MainAxisAlign, TextAreaOptions, UIBoxHandle, UISize, uibox::Color,
};

#[derive(Clone, Debug)]
struct TreeEntry {
    depth: usize,
    name: String,
}

pub struct IdeViewState {
    pub side_width: f32,
    pub editor_text: String,
    tree_entries: Vec<TreeEntry>,
}

impl IdeViewState {
    pub fn new() -> Self {
        let mut s = Self {
            side_width: 280.0,
            editor_text: String::from(
                "// IDE mock view\nfn main() {\n    println!(\"Hello from Mae IDE view\");\n}\n",
            ),
            tree_entries: Vec::new(),
        };
        s.refresh_tree();
        s
    }

    fn refresh_tree(&mut self) {
        self.tree_entries.clear();
        let root = std::path::Path::new(".");
        collect_tree(root, 0, 3, 250, &mut self.tree_entries);
    }
}

fn collect_tree(
    dir: &std::path::Path,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    out: &mut Vec<TreeEntry>,
) {
    if depth > max_depth || out.len() >= max_entries {
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            dirs.push((name, path));
        } else {
            files.push(name);
        }
    }
    dirs.sort_by(|a, b| a.0.cmp(&b.0));
    files.sort();

    for (name, path) in dirs {
        if out.len() >= max_entries {
            break;
        }
        out.push(TreeEntry {
            depth,
            name: format!("▸ {name}"),
        });
        collect_tree(&path, depth + 1, max_depth, max_entries, out);
    }
    for name in files {
        if out.len() >= max_entries {
            break;
        }
        out.push(TreeEntry {
            depth,
            name: format!("  {name}"),
        });
    }
}

pub fn render(ui: &mut IMUI, state: &mut IdeViewState) -> bool {
    let mut back_to_demo = false;
    let root = ui.column(|ui| {
        let topbar = ui.row(|ui| {
            ui.label("Explorer")
                .text_color(ui, Color::new("#d6deeb"))
                .height(ui, UISize::Pixels(28.0));

            let refresh = ui
                .button("Refresh tree", Some("Reload local files"))
                .height(ui, UISize::Pixels(28.0))
                .corner_radius(ui, 6.0);
            if refresh.clicked() {
                state.refresh_tree();
            }

            let back = ui
                .button("Back to demo", Some("Return to the main demo view"))
                .height(ui, UISize::Pixels(28.0))
                .corner_radius(ui, 6.0);
            if back.clicked() {
                back_to_demo = true;
            }
        });
        topbar.height(ui, UISize::Pixels(34.0)).gap(ui, 10.0).align(
            ui,
            MainAxisAlign::Start,
            CrossAxisAlign::Center,
        );

        let mut splitter_handle: Option<UIBoxHandle> = None;
        let body = ui.row(|ui| {
            let sidebar = ui.named_column("###ide_sidebar", |ui| {
                for entry in &state.tree_entries {
                    ui.label(&format!("{}{}", "  ".repeat(entry.depth), entry.name))
                        .height(ui, UISize::Pixels(22.0))
                        .text_color(ui, Color::new("#b7c5d3"));
                }
            });
            sidebar
                .width(ui, UISize::Pixels(state.side_width))
                .height(ui, UISize::ParentPct(1.0))
                .padding_all(ui, 10.0)
                .gap(ui, 2.0)
                .scroll_y(ui, true)
                .clip(ui, true)
                .background(ui, Color::new("#1f2933"))
                .border_color(ui, Color::new("#31404f"));

            let splitter = ui.button("##ide_splitter", Some("Drag to resize"));
            splitter_handle = Some(splitter);
            splitter
                .width(ui, UISize::Pixels(6.0))
                .height(ui, UISize::ParentPct(1.0))
                .background(
                    ui,
                    if splitter.dragging() || splitter.hover() {
                        Color::new("#4f6a84")
                    } else {
                        Color::new("#2d3a47")
                    },
                )
                .corner_radius(ui, 3.0);

            let editor_panel = ui.column(|ui| {
                ui.label("src/main.rs")
                    .height(ui, UISize::Pixels(26.0))
                    .text_color(ui, Color::new("#8fb5ff"));

                let editor = ui.textarea_with_options(
                    "###ide_editor",
                    &mut state.editor_text,
                    TextAreaOptions::new()
                        .wrap_x(false)
                        .scroll_x(true)
                        .scroll_y(true),
                );
                editor.height(ui, UISize::Fill);
            });
            editor_panel
                .width(ui, UISize::Fill)
                .height(ui, UISize::ParentPct(1.0))
                .padding_all(ui, 10.0)
                .gap(ui, 8.0)
                .background(ui, Color::new("#111821"))
                .border_color(ui, Color::new("#2a3542"));
        });
        body.width(ui, UISize::ParentPct(1.0))
            .height(ui, UISize::Fill)
            .gap(ui, 8.0);

        if let Some(splitter) = splitter_handle {
            if splitter.dragging() && ui.mouse_down() {
                if let (Some(mouse), body_bounds) = (ui.mouse_position(), ui.bounds(body)) {
                    let new_w = (mouse.x() - body_bounds.x0 - 3.0).clamp(180.0, 520.0);
                    state.side_width = new_w;
                }
            }
        }
    });

    root.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Fill)
        .padding_all(ui, 12.0)
        .gap(ui, 10.0)
        .background(ui, Color::new("#0f141a"));

    back_to_demo
}
