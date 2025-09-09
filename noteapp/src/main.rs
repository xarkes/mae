use std::cell::RefCell;
use std::rc::Rc;

use mae::imui::IMUI;
use mae::imui::UISize;
use mae::imui::uibox::Color;
use mae::uisize;

mod noteapp;
use noteapp::NoteApp;

macro_rules! icon {
    ($value:tt) => {
        char::from_u32($value).unwrap().to_string().as_str()
    };
}

struct NoteAppTheme {
    pub color_main: Color,
}
impl NoteAppTheme {
    pub fn default() -> Self {
        NoteAppTheme {
            color_main: Color::new("#1ebc93"),
        }
    }
}

fn main() {
    println!("Starting Mae {}_alpha_0", env!("CARGO_PKG_VERSION"));

    // xarkes: init notes
    let mut noteapp = NoteApp::new();
    println!("Selected default database type: local sqlite");

    // init theme
    let theme = NoteAppTheme::default();

    // xarkes: draw UI
    let mut ui = IMUI::new(1024, 768);

    let mut show_search = false;
    let mut search = Rc::new(RefCell::new(String::from("")));
    let save_interval_seconds = 30.;
    let mut last_save = mae::os::timer_value() as f64 / 1e9;
    ui.eventloop(|ui| {
        ui.row(|ui| {
            ui.column(|ui| {
                if ui
                    .button_icon(icon!(0xe161), Some("Save the database to fileystem."))
                    .borrow()
                    .clicked()
                {
                    noteapp.save();
                }
                if ui
                    .button_icon(icon!(0xefd3), Some("Create a new note"))
                    .borrow()
                    .clicked()
                {
                    noteapp.newnote();
                }
                if ui
                    .button_icon(icon!(0xe8b6), Some("Search notes."))
                    .borrow()
                    .clicked()
                {
                    show_search = true;
                    search = Rc::new(RefCell::new(String::from("")));
                }
                if ui
                    .button_icon(
                        icon!(0xe9fc),
                        Some("Import previous notes to the application."),
                    )
                    .borrow()
                    .clicked()
                {
                    noteapp
                        .import_from_markdown(std::path::Path::new(
                            "/Users/user/Downloads/AnyTypeDB/Anytype.20250720.222959.98",
                        ))
                        .unwrap();
                }
            })
            .width(UISize::ChildrenMax)
            .background(theme.color_main);

            ui.textarea(noteapp.buffer.clone(), "#textarea")
                // .width(UISize::Expand)
                // .height(UISize::Expand);
                .width(uisize!("100%"))
                .height(uisize!("100%"));
        });

        // prompts
        if show_search {
            ui.prompt("#search_prompt", |ui| {
                ui.label("Search for notes");
                ui.line_edit(search.clone(), "#search", show_search);
                let search_filter = search.borrow();
                for note in &noteapp.notes() {
                    if search_filter.len() > 0 {
                        if !fuzzy_search(search_filter.as_str(), note.name.as_str()) {
                            continue;
                        }
                    }
                    if ui
                        .button(
                            format!("{}##button_label_{}", note.name, note.id).as_str(),
                            None,
                        )
                        .borrow()
                        .clicked()
                    {
                        noteapp.open(note.id);
                        show_search = false;
                    }
                }
            });
        };

        // common logic
        let curtime = mae::os::timer_value() as f64 / 1e9;
        if last_save + save_interval_seconds < curtime {
            println!("Auto save...");
            noteapp.save();
            last_save = curtime;
        }
    });

    // TODO New: Deadline Oct 1st
    // - [~] change params() way of setting style (maybe not and just KISS)
    // - [ ] fix current layout
    // - [ ] rework font atlas handling (proper multi-font + performance)
    //   - [ ] fix OGL textures
    // - [ ] have proper prompts (focus, escape, etc.)
    // - [ ] support keybindings
    // - [ ] support themes (at least light and dark)
    // - [ ] make text editor better
    //   - [ ] implement text selection, copy, paste, etc.
    //   - [ ] change cursor when hover
    // - [ ] memory improvement (seems to use a lot of memory)
    // - [ ] implement proper fuzzy search (off-thread)
    //   - [ ] maybe add API to work off-thread
}

fn fuzzy_search(filter: &str, data: &str) -> bool {
    // TODO(xarkes): Implement proper search
    data.contains(filter)
}
