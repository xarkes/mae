use mae::{
    imui::{
        CrossAxisAlign, IMUI, MainAxisAlign, TextAreaOptions, ThemeKind, UIBoxHandle, UISize,
        UITheme,
    },
    os::OSCursor,
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

const NEW_NOTE_ICON: &str = "\u{e89c}";
const LIGHT_THEME_ICON: &str = "\u{e518}";
const DARK_THEME_ICON: &str = "\u{e51c}";
const SPLITTER_WIDTH: f32 = 1.0;
const SPLITTER_HIT_PADDING_X: f32 = 5.0;
const SIDEBAR_MIN_WIDTH: f32 = 180.0;
const SIDEBAR_MAX_WIDTH: f32 = 520.0;
const TREE_ROW_HEIGHT: f32 = 26.0;
const TREE_INDENT_WIDTH: f32 = 14.0;

#[derive(Clone, Debug)]
struct TreeEntry {
    depth: usize,
    name: String,
    path_key: String,
    is_dir: bool,
}

pub struct IdeViewState {
    pub side_width: f32,
    pub editor_text: String,
    theme_kind: ThemeKind,
    tree_entries: Vec<TreeEntry>,
    expanded_folders: HashSet<String>,
    splitter_drag_offset: f32,
}

impl IdeViewState {
    pub fn new() -> Self {
        Self {
            side_width: SIDEBAR_MIN_WIDTH,
            editor_text: String::from(
                "# Welcome\n## First note\n\nThis is your first note! Let's try to play with it :)",
            ),
            theme_kind: ThemeKind::Dark,
            tree_entries: Vec::new(),
            expanded_folders: HashSet::from([String::from(".")]),
            splitter_drag_offset: 0.0,
        }
    }
}

pub fn sidebar_button(ui: &mut IMUI, label: &str, tooltip_text: Option<&str>) -> UIBoxHandle {
    let button = ui.button(label, tooltip_text);
    let theme = *ui.theme();
    button
        .width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Pixels(TREE_ROW_HEIGHT))
        .padding_all(ui, 4.0)
        .corner_radius(ui, theme.radius)
        .background(ui, mae::imui::Color::transparent())
        .border_color(ui, mae::imui::Color::transparent())
        .text_color(ui, theme.text)
}

pub fn render(ui: &mut IMUI, state: &mut IdeViewState) -> bool {
    ui.set_theme(UITheme::for_kind(state.theme_kind));
    refresh_tree_entries(state);

    let mut back_to_demo = false;
    let root = ui.column(|ui| {
        let mut splitter_handle: Option<UIBoxHandle> = None;
        let body = ui.row(|ui| {
            let sidebar = ui.named_column("###ide_sidebar", |ui| {
                let toolbar = ui.row(|ui| {
                    ui.button_icon_plain(
                        &format!("{NEW_NOTE_ICON}###ide_new_note"),
                        Some("New note"),
                    );

                    let (theme_icon, theme_tooltip) = match state.theme_kind {
                        ThemeKind::Dark => (LIGHT_THEME_ICON, "Switch to light theme"),
                        ThemeKind::Light => (DARK_THEME_ICON, "Switch to dark theme"),
                    };
                    let theme_button = ui.button_icon_plain(
                        &format!("{theme_icon}###ide_theme"),
                        Some(theme_tooltip),
                    );
                    if theme_button.clicked() {
                        state.theme_kind = match state.theme_kind {
                            ThemeKind::Dark => ThemeKind::Light,
                            ThemeKind::Light => ThemeKind::Dark,
                        };
                        ui.set_theme(UITheme::for_kind(state.theme_kind));
                    }
                });
                toolbar
                    .width(ui, UISize::ParentPct(1.0))
                    .height(ui, UISize::Pixels(ui.theme().toolbar_h))
                    .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center)
                    .gap(ui, ui.theme().gap_sm);

                let separator = ui.label("###ide_sidebar_separator");
                separator
                    .width(ui, UISize::ParentPct(1.0))
                    .height(ui, UISize::Pixels(1.0))
                    .background(ui, ui.theme().border_muted);

                if sidebar_button(ui, "Back to demo", Some("Return to the main demo view"))
                    .clicked()
                {
                    back_to_demo = true;
                }

                render_tree(ui, state);
            });
            sidebar
                .width(ui, UISize::Pixels(state.side_width))
                .height(ui, UISize::ParentPct(1.0))
                .scroll_y(ui, true)
                .clip(ui, true);

            let splitter = ui.button("##ide_splitter", Some("Drag to resize"));
            splitter_handle = Some(splitter);
            let theme = *ui.theme();
            let splitter_color = if splitter.dragging() || splitter.hover() {
                theme.accent_hover
            } else {
                theme.border
            };
            splitter
                .width(ui, UISize::Pixels(SPLITTER_WIDTH))
                .height(ui, UISize::ParentPct(1.0))
                .padding_all(ui, 0.0)
                .corner_radius(ui, 0.0)
                .background(ui, splitter_color)
                .border_color(ui, splitter_color)
                .cursor(ui, OSCursor::ResizeH)
                .hit_padding_x(ui, SPLITTER_HIT_PADDING_X);

            let editor_panel = ui.column(|ui| {
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
                .height(ui, UISize::ParentPct(1.0));
        });
        body.width(ui, UISize::ParentPct(1.0))
            .height(ui, UISize::Fill)
            .gap(ui, 0.0);

        if let Some(splitter) = splitter_handle {
            if splitter.pressed()
                && let (Some(press_pos), body_bounds) =
                    (splitter.signal().left_press_pos, ui.bounds(body))
            {
                let local_press_x = press_pos.x() - body_bounds.x0;
                let splitter_center_x = state.side_width + SPLITTER_WIDTH * 0.5;
                state.splitter_drag_offset = local_press_x - splitter_center_x;
            }

            if splitter.dragging() && ui.mouse_down() {
                if let (Some(mouse), body_bounds) = (ui.mouse_position(), ui.bounds(body)) {
                    let local_mouse_x = mouse.x() - body_bounds.x0;
                    let new_w = (local_mouse_x - state.splitter_drag_offset - SPLITTER_WIDTH * 0.5)
                        .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH);
                    state.side_width = new_w;
                }
            } else {
                state.splitter_drag_offset = 0.0;
            }
        }
    });

    let theme = *ui.theme();
    root.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Fill)
        .gap(ui, theme.gap_md)
        .background(ui, theme.panel_bg);

    back_to_demo
}

fn render_tree(ui: &mut IMUI, state: &mut IdeViewState) {
    let entries = state.tree_entries.clone();
    for entry in entries {
        if !ancestors_are_expanded(state, &entry.path_key) {
            continue;
        }

        let expanded = state.expanded_folders.contains(&entry.path_key);
        let arrow = if entry.is_dir {
            if expanded { "v" } else { ">" }
        } else {
            " "
        };
        let row = ui.row(|ui| {
            ui.label(&format!("###ide_tree_indent_{}", entry.path_key))
                .width(ui, UISize::Pixels(entry.depth as f32 * TREE_INDENT_WIDTH))
                .height(ui, UISize::Pixels(TREE_ROW_HEIGHT));

            let label = format!("{arrow} {}###ide_tree_{}", entry.name, entry.path_key);
            let button = sidebar_button(ui, &label, None).width(ui, UISize::Fill);
            if button.clicked() && entry.is_dir {
                if expanded {
                    state.expanded_folders.remove(&entry.path_key);
                } else {
                    state.expanded_folders.insert(entry.path_key.clone());
                }
            }
        });
        row.width(ui, UISize::ParentPct(1.0))
            .height(ui, UISize::Pixels(TREE_ROW_HEIGHT))
            .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center)
            .gap(ui, 0.0);
    }
}

fn refresh_tree_entries(state: &mut IdeViewState) {
    if !state.tree_entries.is_empty() {
        return;
    }

    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(".")
        .to_string();

    state.tree_entries.push(TreeEntry {
        depth: 0,
        name: root_name,
        path_key: ".".to_string(),
        is_dir: true,
    });
    collect_rs_entries(&root, Path::new("."), 1, &mut state.tree_entries);
}

fn collect_rs_entries(root: &Path, rel: &Path, depth: usize, out: &mut Vec<TreeEntry>) -> bool {
    let dir = root.join(rel);
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    let Ok(read_dir) = fs::read_dir(dir) else {
        return false;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip_tree_name(&name) {
            continue;
        }

        if path.is_dir() {
            dirs.push(name.to_string());
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(name.to_string());
        }
    }

    dirs.sort();
    files.sort();

    let start_len = out.len();
    for dir in dirs {
        let child_rel = rel.join(&dir);
        let path_key = tree_path_key(&child_rel);
        let entry_index = out.len();
        out.push(TreeEntry {
            depth,
            name: dir,
            path_key,
            is_dir: true,
        });
        if !collect_rs_entries(root, &child_rel, depth + 1, out) {
            out.remove(entry_index);
        }
    }

    for file in files {
        let child_rel = rel.join(&file);
        out.push(TreeEntry {
            depth,
            name: file,
            path_key: tree_path_key(&child_rel),
            is_dir: false,
        });
    }

    out.len() > start_len
}

fn ancestors_are_expanded(state: &IdeViewState, path_key: &str) -> bool {
    let mut ancestor = Path::new(path_key).parent();
    while let Some(path) = ancestor {
        let key = tree_path_key(path);
        if !state.expanded_folders.contains(&key) {
            return false;
        }
        ancestor = path.parent();
    }
    true
}

fn tree_path_key(path: &Path) -> String {
    let key = path.to_string_lossy();
    if key.is_empty() {
        ".".to_string()
    } else {
        key.to_string()
    }
}

fn should_skip_tree_name(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "target" | "build" | "dist")
}
