use std::cell::RefCell;
use std::rc::Rc;

use mae::imui::IMUI;
use mae::imui::UISize;
use mae::uisize;

mod noteapp;
use noteapp::NoteApp;

macro_rules! icon {
    ($value:tt) => {
        char::from_u32($value).unwrap().to_string().as_str()
    };
}

fn main() {
    println!("Starting Mae {}_alpha_0", env!("CARGO_PKG_VERSION"));

    // xarkes: init notes
    let mut noteapp = NoteApp::new();
    println!("Selected default database type: local sqlite");

    // xarkes: draw UI
    let mut ui = IMUI::new(1024, 768);

    let mut show_search = false;
    let mut search = Rc::new(RefCell::new(String::from("")));
    let save_interval_seconds = 30.;
    let mut last_save = mae::os::timer_value() as f64 / 1e9;
    ui.eventloop(|ui| {
        // main content
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
            .width(uisize!("25px"));
            ui.textarea(noteapp.buffer.clone(), "#textarea")
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
    // - [ ] rework font atlas handling
    // - [ ] have proper prompts (focus, escape, etc.)
    // - [ ] support keybindings
    // - [ ] support styling
    // - [ ] make text editor better
    //   - [ ] implement text selection, copy, paste, etc.

    // TODO:
    //
    // 1. rewrite/rethink event handling (need something like chain but idk it changes the whole shit)
    //   -> check how egui does it
    // 2. rethink theming and use it (and support dark mode)
    //   -> also have to rewrite the general styling API (theme vs per component style vs ... ? its really not clear atm)
    // 3. implement clipping
    //   -> text clipping + box clipping in general
    // 4. rework fonts - soon we'll want to optimize it so before that we should be able to handle multiple fonts
    // 5. change os cursor when hovering text fields etc.
    //
    //
    //
    // Implement search/go to button
    // --> dropdown menu a la command palette
    // --> fix event handling and consuming, it is still fucking wrong
    // --> (re-implement dragging)
    // --> implement shortcuts
    //
    // --> must be smooth af
    // --> implement animation
}

fn fuzzy_search(filter: &str, data: &str) -> bool {
    // TODO(xarkes): Implement proper search
    data.contains(filter)
}
