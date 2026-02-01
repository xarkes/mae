use std::cell::RefCell;
use std::rc::Rc;

mod draw;
mod imui;
mod os;
mod render;

use imui::IMUI;
use imui::UISize;
use imui::uibox::{Color, UIBoxRef2};
use imui::{CrossAxisAlign, MainAxisAlign};
use os::OSEventFlag;
use os::OSKey;
use os::OSKeyCode;

mod noteapp;
use noteapp::NoteApp;

macro_rules! icon {
    ($value:tt) => {
        char::from_u32($value).unwrap().to_string().as_str()
    };
}

#[derive(PartialEq)]
enum AppView {
    Login,
    Main,
}

fn open_new_note(noteapp: &mut NoteApp, ui: &mut IMUI) {
    noteapp.new_note();
    ui.reset_text_input_state();
    ui.set_focus_active("#textarea"); // focus textarea on next frame
}

fn main() {
    println!("Starting Mae {}_alpha_0", env!("CARGO_PKG_VERSION"));

    // App state
    let mut current_view = AppView::Login;
    let passphrase = Rc::new(RefCell::new(String::new()));
    let mut login_error: Option<String> = None;

    // Note app (initialized lazily after login)
    let mut noteapp: Option<NoteApp> = None;

    // xarkes: draw UI
    let mut ui = IMUI::new(480, 360);

    // xarkes: define global shortcuts
    let shortcut_save = (OSKey::Keyboard(OSKeyCode::KeyS), Some(OSEventFlag::Control));

    let mut show_search = false;
    let mut search = Rc::new(RefCell::new(String::from("")));
    let save_interval_seconds = 30.;
    let freq = os::timer_init();
    let mut last_save = os::timer_value() as f64 / freq;

    // Sidebar resize state
    let mut sidebar_width: f32 = 220.0;
    let mut resizing_sidebar = false;

    ui.eventloop(|mut ui| {
        match current_view {
            AppView::Login => {
                // Dark background for the whole window with centered content
                ui.column(|ui| {
                    // Login card
                    let card = ui.column(|ui| {
                        // Title
                        let title = ui.label("Remote Vault");
                        UIBoxRef2::new(title).text_color(Color::new("#ffffff"));

                        // Subtitle
                        let subtitle = ui.label("Enter your passphrase to unlock");
                        UIBoxRef2::new(subtitle).text_color(Color::new("#888888"));

                        // Spacer
                        ui.row(|_| {}).height(UISize::Fixed(24.0));

                        // Passphrase input
                        let input = ui.line_edit(passphrase.clone(), "#passphrase", true);
                        input
                            .width(UISize::Fixed(280.0))
                            .height(UISize::Fixed(36.0))
                            .background(Color::new("#3a3a3a"));

                        // Spacer
                        ui.row(|_| {}).height(UISize::Fixed(8.0));

                        // Error message
                        if let Some(ref err) = login_error {
                            let err_label = ui.label(err.as_str());
                            UIBoxRef2::new(err_label).text_color(Color::new("#ff6b6b"));
                            ui.row(|_| {}).height(UISize::Fixed(8.0));
                        }

                        // Connect button
                        let connect_btn = ui.button("Unlock##login_btn", None);
                        UIBoxRef2::new(connect_btn.clone())
                            .width(UISize::Fixed(280.0))
                            .height(UISize::Fixed(40.0))
                            .background(Color::new("#1ebc93"));

                        let enter_pressed = ui.input(OSKey::Keyboard(OSKeyCode::KeyEnter), None);

                        if connect_btn.borrow().clicked() || enter_pressed {
                            let pass = passphrase.borrow();
                            if pass.is_empty() {
                                login_error = Some("Passphrase cannot be empty".to_string());
                            } else {
                                // TODO: Connect to remote server with passphrase
                                println!("Connecting with passphrase...");
                                noteapp = Some(NoteApp::new());
                                current_view = AppView::Main;
                            }
                        }
                    });
                    card.width(UISize::Fixed(320.0))
                        .height(UISize::Fit)
                        .padding_all(16.0)
                        .gap(4.0)
                        .background(Color::new("#2a2a2a"));
                })
                .width(UISize::Grow)
                .height(UISize::Grow)
                .align(MainAxisAlign::Center, CrossAxisAlign::Center)
                .background(Color::new("#1a1a1a"));
            }

            AppView::Main => {
                let mut noteapp = noteapp.as_mut().unwrap();
                let sidebar_bg = Color::new("#1e1e2e");
                let sidebar_header_bg = Color::new("#181825");
                let editor_bg = Color::new("#11111b");
                let note_hover_bg = Color::new("#313244");
                let note_selected_bg = Color::new("#45475a");
                let text_dim = Color::new("#6c7086");

                ui.row(|ui| {
                    // Left sidebar
                    ui.column(|ui| {
                        // Toolbar row
                        ui.row(|ui| {
                            if ui
                                .button_icon(icon!(0xe161), Some("Save (Ctrl+S)"))
                                .borrow()
                                .clicked()
                            {
                                noteapp.save();
                            }
                            if ui
                                .button_icon(icon!(0xefd3), Some("New note (Ctrl+N)"))
                                .borrow()
                                .clicked()
                            {
                                open_new_note(&mut noteapp, ui);
                            }
                            if ui
                                .button_icon(icon!(0xe8b6), Some("Search (Ctrl+G)"))
                                .borrow()
                                .clicked()
                            {
                                show_search = true;
                                search = Rc::new(RefCell::new(String::from("")));
                            }
                        })
                        .padding_all(8.0)
                        .gap(4.0)
                        .background(sidebar_header_bg);

                        // Notes section header
                        let header = ui.label("Notes");
                        UIBoxRef2::new(header)
                            .text_color(text_dim)
                            .padding_all(12.0);

                        // Notes list
                        let notes = noteapp.notes();
                        let current_id = noteapp.current_note_id();
                        for note in &notes {
                            let is_selected = note.id == current_id;
                            let display_name = if note.name.len() > 24 {
                                format!("{}...", &note.name[..24])
                            } else {
                                note.name.clone()
                            };
                            let button = ui.button(
                                format!("{}##note_{}", display_name, note.id).as_str(),
                                None,
                            );
                            let button_ref = UIBoxRef2::new(button.clone());
                            button_ref.width(UISize::Grow).padding_all(10.0).background(
                                if is_selected {
                                    note_selected_bg
                                } else {
                                    note_hover_bg
                                },
                            );
                            if button.borrow().clicked() {
                                noteapp.open(note.id);
                            }
                        }
                    })
                    .width(UISize::Fixed(sidebar_width))
                    .background(sidebar_bg);

                    // Resize handle (using button for built-in click handling)
                    let resize_handle = ui.button("##resize_handle", None);
                    let resize_ref = UIBoxRef2::new(resize_handle.clone());
                    resize_ref
                        .width(UISize::Fixed(6.0))
                        .height(UISize::Grow)
                        .background(Color::new("#313244"));

                    // Check if resize handle is being dragged
                    if resize_handle.borrow().click() {
                        resizing_sidebar = true;
                    }
                    if resizing_sidebar {
                        if let Some(mouse_pos) = ui.mouse_position() {
                            sidebar_width = mouse_pos.x().max(100.0).min(500.0);
                        }
                        if !ui.mouse_down() {
                            resizing_sidebar = false;
                        }
                    }
                    // Highlight handle on hover or while dragging
                    if resize_handle.borrow().hover() || resizing_sidebar {
                        resize_ref.background(Color::new("#45475a"));
                    }

                    // Main editor area
                    ui.column(|ui| {
                        // Editor header with current note info
                        let cur_id = noteapp.current_note_id();
                        let current_note = noteapp.notes().into_iter().find(|n| n.id == cur_id);
                        let note_title = current_note
                            .map(|n| {
                                if n.name.len() > 50 {
                                    format!("{}...", &n.name[..50])
                                } else {
                                    n.name
                                }
                            })
                            .unwrap_or_else(|| "Untitled".to_string());
                        let header = ui.label(note_title.as_str());
                        UIBoxRef2::new(header)
                            .text_color(Color::new("#cdd6f4"))
                            .padding_all(12.0);

                        // Separator line
                        let separator = ui.row(|_| {});
                        separator
                            .height(UISize::Fixed(1.0))
                            .background(Color::new("#313244"));

                        // Text editor with padding
                        ui.textarea(noteapp.buffer.clone(), "#textarea")
                            .padding_all(16.0)
                            .background(Color::new("#1e1e2e"));
                    })
                    .background(editor_bg);
                });

                // prompts
                ui.prompt("#search_prompt", &mut show_search, |ui, show| {
                    ui.label("Search for notes");
                    let search_input = ui.line_edit(search.clone(), "#search", true);
                    search_input.width(UISize::Grow);
                    let search_filter = search.borrow();
                    for note in &noteapp.notes() {
                        if search_filter.len() > 0 {
                            if !fuzzy_search(search_filter.as_str(), note.name.as_str()) {
                                continue;
                            }
                        }
                        let button = ui.button(
                            format!("> {}##button_label_{}", note.name, note.id).as_str(),
                            None,
                        );
                        let buttonr = UIBoxRef2::new(button.clone());
                        buttonr.background(ui.theme.color_bg_popup);
                        if button.borrow().clicked() {
                            *show = false;
                            noteapp.open(note.id);
                        }
                    }
                });

                // common logic
                let curtime = os::timer_value() as f64 / freq;
                if last_save + save_interval_seconds < curtime {
                    println!("Auto save...");
                    noteapp.save();
                    last_save = curtime;
                }

                // shortcuts
                if ui.input(shortcut_save.0, shortcut_save.1) {
                    noteapp.save();
                }
                if ui.input(OSKey::Keyboard(OSKeyCode::KeyN), Some(OSEventFlag::Control)) {
                    open_new_note(&mut noteapp, &mut ui);
                }
                if !show_search
                    && ui.input(OSKey::Keyboard(OSKeyCode::KeyG), Some(OSEventFlag::Control))
                {
                    show_search = true;
                    search = Rc::new(RefCell::new(String::from("")));
                }
            }
        }
    });
}

fn fuzzy_search(filter: &str, data: &str) -> bool {
    // TODO(xarkes): Implement proper search
    data.contains(filter)
}
