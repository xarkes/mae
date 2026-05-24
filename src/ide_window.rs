use crate::app_style;
use mae::{
    imui::{CrossAxisAlign, IMUI, MainAxisAlign, TextAreaOptions, UIBoxHandle, UISize},
    os::OSCursor,
};

const SPLITTER_WIDTH: f32 = 1.0;
const SPLITTER_HIT_PADDING_X: f32 = 5.0;
const SIDEBAR_MIN_WIDTH: f32 = 180.0;
const SIDEBAR_MAX_WIDTH: f32 = 520.0;

#[derive(Clone, Debug)]
struct TreeEntry {
    depth: usize,
    name: String,
}

pub struct IdeViewState {
    pub side_width: f32,
    pub editor_text: String,
    tree_entries: Vec<TreeEntry>,
    splitter_drag_offset: f32,
}

impl IdeViewState {
    pub fn new() -> Self {
        let mut s = Self {
            side_width: 280.0,
            editor_text: String::from(
                "// IDE mock view\nfn main() {\n    println!(\"Hello from Mae IDE view\");\n}\n",
            ),
            tree_entries: Vec::new(),
            splitter_drag_offset: 0.0,
        };
        s
    }
}

pub fn render(ui: &mut IMUI, state: &mut IdeViewState) -> bool {
    let mut back_to_demo = false;
    let root = ui.column(|ui| {
        let topbar = ui.row(|ui| {
            let explorer = ui.label("Explorer");
            app_style::title(ui, explorer).height(ui, UISize::Pixels(28.0));
            let back = ui
                .button("Back to demo", Some("Return to the main demo view"))
                .height(ui, UISize::Pixels(28.0));
            app_style::button(ui, back);
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
                ui.button("HELLO", None);
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
                let file_title = ui.label("src/main.rs");
                app_style::accent_text(ui, file_title).height(ui, UISize::Pixels(26.0));

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
        .padding_all(ui, theme.pad_lg)
        .gap(ui, theme.gap_md)
        .background(ui, theme.panel_bg);

    back_to_demo
}
