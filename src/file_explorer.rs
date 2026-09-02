//! A modal folder-selection widget ("file explorer") for Mae.
//!
//! Mae is immediate-mode, so the widget keeps no global state: the caller owns
//! one [`FileExplorer`] (typically an `Option<FileExplorer>`), calls
//! [`FileExplorer::show`] every frame while it is open, and reacts to the
//! returned [`FileExplorerOutcome`] — dropping the explorer on `Picked`/
//! `Cancelled`.
//!
//! It lists only directories (it is a *folder* picker): clicking a row descends
//! into it, the `..` row climbs to the parent, and the confirm button selects
//! whichever directory is currently open.

use std::path::{Path, PathBuf};

use crate::imui::{Color, CrossAxisAlign, IMUI, MainAxisAlign, Point, UIBoxHandle, UISize};

/// One directory shown in the listing.
struct DirRow {
    name: String,
    path: PathBuf,
}

/// An optional labelled checkbox shown above the footer (e.g. an import option).
struct ToggleOption {
    label: String,
    value: bool,
}

/// What [`FileExplorer::show`] reports for the current frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileExplorerOutcome {
    /// Still open — keep showing it next frame.
    Browsing,
    /// Dismissed without choosing anything.
    Cancelled,
    /// The user confirmed this folder.
    Picked(PathBuf),
}

/// A centered, modal folder picker.
pub struct FileExplorer {
    cwd: PathBuf,
    rows: Vec<DirRow>,
    /// Set when the current directory could not be read; the previous listing
    /// stays visible so the user can navigate back out.
    error: Option<String>,
    title: String,
    confirm_label: String,
    /// Optional checkbox the caller can read back via [`FileExplorer::toggle_value`].
    toggle: Option<ToggleOption>,
}

impl FileExplorer {
    /// Open a folder picker starting at `start`. If `start` is not an existing
    /// directory it falls back to the nearest existing ancestor, then the
    /// process working directory, then the filesystem root.
    pub fn folder_picker(start: impl AsRef<Path>) -> Self {
        let mut explorer = Self {
            cwd: resolve_start_dir(start.as_ref()),
            rows: Vec::new(),
            error: None,
            title: "Select a folder".to_string(),
            confirm_label: "Select".to_string(),
            toggle: None,
        };
        explorer.reload();
        explorer
    }

    /// Override the window title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Override the confirm button's label (e.g. "Import this folder").
    pub fn confirm_label(mut self, label: impl Into<String>) -> Self {
        self.confirm_label = label.into();
        self
    }

    /// Add a labelled checkbox shown above the footer. Read its state with
    /// [`FileExplorer::toggle_value`] once the user confirms a folder.
    pub fn with_toggle(mut self, label: impl Into<String>, initial: bool) -> Self {
        self.toggle = Some(ToggleOption {
            label: label.into(),
            value: initial,
        });
        self
    }

    /// Current state of the optional checkbox (`false` when there is none).
    pub fn toggle_value(&self) -> bool {
        self.toggle.as_ref().is_some_and(|t| t.value)
    }

    /// The directory currently being browsed — what confirming would pick.
    pub fn current_dir(&self) -> &Path {
        &self.cwd
    }

    /// Re-read the current directory's subfolders.
    fn reload(&mut self) {
        match read_subdirs(&self.cwd) {
            Ok(rows) => {
                self.rows = rows;
                self.error = None;
            }
            Err(err) => self.error = Some(err),
        }
    }

    fn navigate_to(&mut self, dir: PathBuf) {
        self.cwd = dir;
        self.reload();
    }

    fn go_up(&mut self) {
        if let Some(parent) = self.cwd.parent().map(Path::to_path_buf) {
            self.navigate_to(parent);
        }
    }

    /// Render the explorer for this frame and report the outcome.
    pub fn show(&mut self, ui: &mut IMUI) -> FileExplorerOutcome {
        let theme = *ui.theme();
        let (screen_w, screen_h) = ui.window_size();
        let width = 460.0_f32.min(screen_w - 32.0).max(280.0);
        let height = 440.0_f32.min(screen_h - 32.0).max(240.0);
        let pos = Point::new(
            ((screen_w - width) * 0.5).max(0.0),
            ((screen_h - height) * 0.5).max(0.0),
        );

        let mut outcome = FileExplorerOutcome::Browsing;

        let pane = ui.floating_pane_at(pos, Some("###mae_file_explorer"), |ui| {
            ui.label(&self.title)
                .width(ui, UISize::ParentPct(1.0))
                .height(ui, UISize::Pixels(28.0))
                .text_color(ui, theme.text)
                .font_size(ui, theme.size_text + 3.0);

            ui.label(&self.cwd.display().to_string())
                .width(ui, UISize::ParentPct(1.0))
                .height(ui, UISize::Pixels(20.0))
                .text_color(ui, theme.text_muted)
                .font_size(ui, theme.size_text - 1.0);

            if let Some(err) = self.error.clone() {
                ui.label(&err)
                    .width(ui, UISize::ParentPct(1.0))
                    .text_color(ui, Color::new("#e05252"))
                    .font_size(ui, theme.size_text - 1.0);
            }

            // Collected here and applied after the listing so we don't mutate
            // `self` while iterating its rows.
            let mut descend_into: Option<PathBuf> = None;
            let mut go_up = false;

            let list = ui.named_column("###mae_fe_list", |ui| {
                if self.cwd.parent().is_some()
                    && folder_row(ui, &theme, "###mae_fe_up", "..").clicked()
                {
                    go_up = true;
                }
                if self.rows.is_empty() && self.error.is_none() {
                    ui.label("No subfolders here")
                        .width(ui, UISize::ParentPct(1.0))
                        .text_color(ui, theme.text_muted)
                        .font_size(ui, theme.size_text - 1.0);
                }
                for (i, row) in self.rows.iter().enumerate() {
                    if folder_row(ui, &theme, &format!("###mae_fe_row_{i}"), &row.name).clicked() {
                        descend_into = Some(row.path.clone());
                    }
                }
            });
            list.width(ui, UISize::ParentPct(1.0))
                .height(ui, UISize::Fill)
                .gap(ui, 2.0)
                .scroll_y(ui, true)
                .clip(ui, true)
                .background(ui, theme.input_bg)
                .border_color(ui, theme.border)
                .corner_radius(ui, theme.radius)
                .padding_all(ui, theme.pad_sm);

            // Optional checkbox (e.g. "Import as a new space"), above the footer.
            let toggle_info = self.toggle.as_ref().map(|t| (t.label.clone(), t.value));
            if let Some((label, value)) = toggle_info {
                if checkbox_row(ui, &theme, "###mae_fe_toggle", &label, value).clicked() {
                    if let Some(toggle) = self.toggle.as_mut() {
                        toggle.value = !value;
                    }
                }
            }

            let footer = ui.row(|ui| {
                let confirm = ui
                    .button(&format!("{}###mae_fe_confirm", self.confirm_label), None)
                    .width(ui, UISize::Fill)
                    .height(ui, UISize::Pixels(theme.control_h))
                    .background(ui, theme.accent)
                    .text_color(ui, Color::new("#ffffff"));
                if confirm.clicked() {
                    outcome = FileExplorerOutcome::Picked(self.cwd.clone());
                }
                if ui
                    .button("Cancel###mae_fe_cancel", None)
                    .width(ui, UISize::Fill)
                    .height(ui, UISize::Pixels(theme.control_h))
                    .clicked()
                {
                    outcome = FileExplorerOutcome::Cancelled;
                }
            });
            footer
                .width(ui, UISize::ParentPct(1.0))
                .height(ui, UISize::Pixels(theme.control_h + 4.0))
                .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center)
                .gap(ui, theme.gap_md);

            if go_up {
                self.go_up();
            } else if let Some(dir) = descend_into {
                self.navigate_to(dir);
            }
        });

        pane.width(ui, UISize::Pixels(width))
            .height(ui, UISize::Pixels(height))
            .padding_all(ui, theme.pad_lg)
            .gap(ui, theme.gap_sm)
            .background(ui, theme.popover_bg)
            .border_color(ui, theme.border)
            .corner_radius(ui, theme.radius);

        outcome
    }
}

/// One folder row: a hover-highlighted clickable line.
fn folder_row(ui: &mut IMUI, theme: &crate::imui::UITheme, id: &str, name: &str) -> UIBoxHandle {
    let row = ui.clickable_row(id, |ui| {
        ui.label(name)
            .width(ui, UISize::Fill)
            .text_color(ui, theme.text)
            .font_size(ui, theme.size_text);
    });
    let bg = if row.hover() {
        theme.surface_hover
    } else {
        Color::transparent()
    };
    row.background(ui, bg)
        .width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Pixels(26.0))
        .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center)
        .corner_radius(ui, theme.radius)
        .padding(ui, 0.0, theme.pad_sm, 0.0, theme.pad_sm);
    row
}

/// A checkbox + label row. The box is filled with the accent colour when on.
fn checkbox_row(
    ui: &mut IMUI,
    theme: &crate::imui::UITheme,
    id: &str,
    label: &str,
    checked: bool,
) -> UIBoxHandle {
    let box_id = format!("{id}_box");
    let row = ui.clickable_row(id, |ui| {
        let mark = ui.named_row(&box_id, |_ui| {});
        let fill = if checked {
            theme.accent
        } else {
            theme.input_bg
        };
        mark.width(ui, UISize::Pixels(16.0))
            .height(ui, UISize::Pixels(16.0))
            .background(ui, fill)
            .border_color(ui, theme.border)
            .corner_radius(ui, 3.0);
        ui.label(label)
            .width(ui, UISize::Fill)
            .text_color(ui, theme.text)
            .font_size(ui, theme.size_text);
    });
    row.width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::Pixels(26.0))
        .align(ui, MainAxisAlign::Start, CrossAxisAlign::Center)
        .gap(ui, theme.gap_sm)
        .padding(ui, 0.0, theme.pad_sm, 0.0, theme.pad_sm);
    row
}

/// Resolve a requested start path to an existing directory to browse.
fn resolve_start_dir(start: &Path) -> PathBuf {
    let candidate = if start.is_dir() {
        start.to_path_buf()
    } else {
        // A file, or a path that doesn't exist: walk up to the first real dir.
        start
            .ancestors()
            .find(|ancestor| ancestor.is_dir())
            .map(Path::to_path_buf)
            .unwrap_or_else(default_dir)
    };
    // Clean the path where possible; an unreadable/again-missing path falls back.
    candidate.canonicalize().unwrap_or(candidate)
}

fn default_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
}

/// Subdirectories of `dir`, sorted case-insensitively, hidden ones omitted.
/// Symlinks pointing at directories are included (we follow via [`Path::is_dir`]).
fn read_subdirs(dir: &Path) -> Result<Vec<DirRow>, String> {
    let read =
        std::fs::read_dir(dir).map_err(|err| format!("Can't open {}: {err}", dir.display()))?;
    let mut rows: Vec<DirRow> = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        rows.push(DirRow { name, path });
    }
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a small tree under a unique temp dir; returns its root.
    fn temp_tree() -> PathBuf {
        let root = std::env::temp_dir().join(format!("mae_fe_test_{}", uuid_like()));
        std::fs::create_dir_all(root.join("Alpha")).unwrap();
        std::fs::create_dir_all(root.join("beta")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("note.txt"), b"x").unwrap();
        root
    }

    fn uuid_like() -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    #[test]
    fn lists_only_visible_subdirectories_sorted() {
        let root = temp_tree();
        let fe = FileExplorer::folder_picker(&root);
        let names: Vec<&str> = fe.rows.iter().map(|r| r.name.as_str()).collect();
        // Files and dotfolders are excluded; sort is case-insensitive.
        assert_eq!(names, vec!["Alpha", "beta"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn descend_and_go_up_navigates_the_tree() {
        let root = temp_tree();
        let mut fe = FileExplorer::folder_picker(&root);
        let alpha = fe
            .rows
            .iter()
            .find(|r| r.name == "Alpha")
            .unwrap()
            .path
            .clone();

        fe.navigate_to(alpha.clone());
        assert_eq!(fe.current_dir(), alpha.as_path());
        assert!(fe.rows.is_empty()); // Alpha has no subfolders

        fe.go_up();
        // Back where we started (canonicalized on both sides).
        assert_eq!(
            fe.current_dir().canonicalize().unwrap(),
            root.canonicalize().unwrap()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_start_path_falls_back_to_existing_ancestor() {
        let root = temp_tree();
        let missing = root.join("Alpha").join("does").join("not").join("exist");
        let fe = FileExplorer::folder_picker(&missing);
        // Resolves to the nearest real directory (Alpha), never a bogus path.
        assert!(fe.current_dir().is_dir());
        assert!(fe.current_dir().ends_with("Alpha"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn confirm_label_and_title_are_overridable() {
        let root = temp_tree();
        let fe = FileExplorer::folder_picker(&root)
            .title("Import notes")
            .confirm_label("Import this folder");
        assert_eq!(fe.title, "Import notes");
        assert_eq!(fe.confirm_label, "Import this folder");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn toggle_defaults_to_off_and_reports_initial_state() {
        let root = temp_tree();
        let plain = FileExplorer::folder_picker(&root);
        assert!(!plain.toggle_value(), "no toggle reads as off");

        let with_on = FileExplorer::folder_picker(&root).with_toggle("As a new space", true);
        assert!(with_on.toggle_value());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn renders_a_frame_without_panicking() {
        let root = temp_tree();
        // Include a toggle so the checkbox path is exercised too.
        let mut fe = FileExplorer::folder_picker(&root).with_toggle("Import as a new space", false);
        let mut ui = IMUI::new_for_test(800.0, 600.0);
        ui.begin_frame();
        // Building the whole widget tree (title, path, listing, checkbox,
        // footer) must succeed, and with no input the explorer stays open.
        let outcome = fe.show(&mut ui);
        ui.end_frame();
        assert_eq!(outcome, FileExplorerOutcome::Browsing);
        let _ = std::fs::remove_dir_all(root);
    }
}
