use mae::{
    imui::{
        CrossAxisAlign, IMUI, MainAxisAlign, TextAreaOptions, ThemeKind, UIBoxHandle, UISize,
        UITheme,
    },
    os::OSCursor,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

const NEW_NOTE_ICON: &str = "\u{e89c}";
const LIGHT_THEME_ICON: &str = "\u{e518}";
const DARK_THEME_ICON: &str = "\u{e51c}";
const TREE_CHEVRON_RIGHT_ICON: &str = "\u{e5cc}";
const TREE_EXPAND_MORE_ICON: &str = "\u{e5cf}";
const SPLITTER_WIDTH: f32 = 1.0;
const SPLITTER_HIT_PADDING_X: f32 = 5.0;
const SIDEBAR_MIN_WIDTH: f32 = 180.0;
const SIDEBAR_MAX_WIDTH: f32 = 520.0;
const TREE_ROW_HEIGHT: f32 = 26.0;
const TREE_INDENT_WIDTH: f32 = 14.0;
const TREE_ICON_WIDTH: f32 = 32.0;
const TREE_ANIMATION_RATE: f32 = 18.0;
const TREE_ANIMATION_EPSILON: f32 = 0.01;

#[derive(Clone, Debug)]
struct TreeEntry {
    depth: usize,
    name: String,
    path_key: String,
    is_dir: bool,
}

#[derive(Clone, Copy, Debug)]
struct TreeAnimation {
    progress: f32,
    target: f32,
}

pub struct IdeViewState {
    pub side_width: f32,
    pub editor_text: String,
    theme_kind: ThemeKind,
    tree_entries: Vec<TreeEntry>,
    expanded_folders: HashSet<String>,
    tree_animations: HashMap<String, TreeAnimation>,
    last_tree_animation_tick: Instant,
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
            tree_animations: HashMap::new(),
            last_tree_animation_tick: Instant::now(),
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
                let toolbar_h = ui.theme().toolbar_h;
                let toolbar_pad = ((toolbar_h - TREE_ICON_WIDTH) * 0.5).max(0.0);
                toolbar
                    .width(ui, UISize::ParentPct(1.0))
                    .height(ui, UISize::Pixels(toolbar_h))
                    .padding_all(ui, toolbar_pad)
                    .align(ui, MainAxisAlign::Center, CrossAxisAlign::Center)
                    .gap(ui, ui.theme().gap_sm);

                let separator = ui.named_column("###ide_sidebar_separator", |_| {});
                let separator_color = match ui.theme().kind {
                    ThemeKind::Dark => ui.theme().border,
                    ThemeKind::Light => ui.theme().text_muted,
                };
                separator
                    .width(ui, UISize::ParentPct(1.0))
                    .height(ui, UISize::Pixels(1.0))
                    .background(ui, separator_color);

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
    update_tree_animations(ui, state);

    let entries = state.tree_entries.clone();
    for entry in entries {
        let visibility = tree_entry_visibility(state, &entry.path_key);
        if visibility <= TREE_ANIMATION_EPSILON {
            continue;
        }

        let expanded = state.expanded_folders.contains(&entry.path_key);
        let row_h = TREE_ROW_HEIGHT * visibility;
        let row = ui.named_row(&format!("###ide_tree_row_{}", entry.path_key), |ui| {
            ui.named_column(&format!("###ide_tree_indent_{}", entry.path_key), |_| {})
                .width(ui, UISize::Pixels(entry.depth as f32 * TREE_INDENT_WIDTH))
                .height(ui, UISize::Pixels(row_h));

            let mut clicked = false;
            if entry.is_dir {
                let icon = if expanded {
                    TREE_EXPAND_MORE_ICON
                } else {
                    TREE_CHEVRON_RIGHT_ICON
                };
                let icon = ui
                    .button_icon_plain(&format!("{icon}###ide_tree_icon_{}", entry.path_key), None)
                    .width(ui, UISize::Pixels(TREE_ICON_WIDTH))
                    .height(ui, UISize::Pixels(row_h))
                    .padding_all(ui, 0.0);
                clicked |= icon.clicked();
            } else {
                ui.named_column(
                    &format!("###ide_tree_icon_spacer_{}", entry.path_key),
                    |_| {},
                )
                .width(ui, UISize::Pixels(TREE_ICON_WIDTH))
                .height(ui, UISize::Pixels(row_h));
            }

            let label = format!("{}###ide_tree_label_{}", entry.name, entry.path_key);
            let button = sidebar_button(ui, &label, None)
                .width(ui, UISize::Fill)
                .height(ui, UISize::Pixels(row_h));
            clicked |= button.clicked();

            if clicked && entry.is_dir {
                toggle_tree_entry(ui, state, &entry.path_key, expanded);
            }
        });
        row.width(ui, UISize::ParentPct(1.0))
            .height(ui, UISize::Pixels(row_h))
            .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center)
            .gap(ui, 0.0)
            .clip(ui, true);
    }
}

fn toggle_tree_entry(ui: &mut IMUI, state: &mut IdeViewState, path_key: &str, expanded: bool) {
    if expanded {
        state.expanded_folders.remove(path_key);
        state.tree_animations.insert(
            path_key.to_string(),
            TreeAnimation {
                progress: 1.0,
                target: 0.0,
            },
        );
    } else {
        state.expanded_folders.insert(path_key.to_string());
        state.tree_animations.insert(
            path_key.to_string(),
            TreeAnimation {
                progress: 0.0,
                target: 1.0,
            },
        );
    }
    state.last_tree_animation_tick = Instant::now();
    ui.request_repaint();
}

fn update_tree_animations(ui: &mut IMUI, state: &mut IdeViewState) {
    let now = Instant::now();
    let dt = now
        .duration_since(state.last_tree_animation_tick)
        .as_secs_f32()
        .clamp(1.0 / 240.0, 1.0 / 15.0);
    state.last_tree_animation_tick = now;

    if state.tree_animations.is_empty() {
        return;
    }

    let rate = (1.0 - 2.0_f32.powf(-TREE_ANIMATION_RATE * dt)).clamp(0.0, 1.0);
    let mut finished = Vec::new();
    for (path_key, animation) in state.tree_animations.iter_mut() {
        animation.progress += (animation.target - animation.progress) * rate;
        if (animation.target - animation.progress).abs() <= TREE_ANIMATION_EPSILON {
            animation.progress = animation.target;
            finished.push(path_key.clone());
        }
    }

    for path_key in finished {
        state.tree_animations.remove(&path_key);
    }

    if !state.tree_animations.is_empty() {
        ui.request_repaint();
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

fn tree_entry_visibility(state: &IdeViewState, path_key: &str) -> f32 {
    if path_key == "." {
        return 1.0;
    }

    let mut visibility: f32 = 1.0;
    let mut ancestor = Path::new(path_key).parent();
    while let Some(path) = ancestor {
        let key = tree_path_key(path);
        let animation_progress = state
            .tree_animations
            .get(&key)
            .map(|animation| animation.progress);
        let ancestor_visibility = if state.expanded_folders.contains(&key) {
            animation_progress.unwrap_or(1.0)
        } else {
            animation_progress.unwrap_or(0.0)
        };
        visibility *= ancestor_visibility;
        if visibility <= TREE_ANIMATION_EPSILON {
            return 0.0;
        }
        ancestor = path.parent();
    }
    visibility
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
