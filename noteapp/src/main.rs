use std::cell::RefCell;
use std::rc::Rc;

use mae::imui::IMUI;
use mae::imui::UISize;
use mae::imui::uibox::UIBoxRef2;
use mae::os::OSEventFlag;
use mae::os::OSKey;
use mae::os::OSKeyCode;

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

    // xarkes: draw UI
    let mut ui = IMUI::new(1024, 768);

    let mut show_search = false;
    let mut search = Rc::new(RefCell::new(String::from("")));
    let save_interval_seconds = 30.;
    let freq = mae::os::timer_init();
    let mut last_save = mae::os::timer_value() as f64 / freq;
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
            .background(ui.theme.color_main);

            ui.textarea(noteapp.buffer.clone(), "#textarea")
                .background(ui.theme.color_bg_popup);
        });

        // prompts
        ui.prompt("#search_prompt", &mut show_search, |ui, show| {
            ui.label("Search for notes");
            let search_input = ui.line_edit(search.clone(), "#search");
            search_input.width(UISize::Expand);
            ui.focus(search_input);
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
        let curtime = mae::os::timer_value() as f64 / freq;
        if last_save + save_interval_seconds < curtime {
            println!("Auto save...");
            noteapp.save();
            last_save = curtime;
        }

        // shortcuts
        if ui.input(OSKey::Keyboard(OSKeyCode::KeyS), Some(OSEventFlag::Control)) {
            noteapp.save();
        }
        if ui.input(OSKey::Keyboard(OSKeyCode::KeyN), Some(OSEventFlag::Control)) {
            noteapp.newnote();
        }
        if ui.input(OSKey::Keyboard(OSKeyCode::KeyG), Some(OSEventFlag::Control)) {
            show_search = true;
            search = Rc::new(RefCell::new(String::from("")));
        }
    });

    // TODO New: Deadline Oct 1st
    // - [~] change params() way of setting style (maybe not and just KISS)
    // - [~] fix current layout
    // - [ ] rework font atlas handling (proper multi-font + performance)
    //   - [ ] fix OGL textures
    // - [ ] have proper prompts (focus, escape, etc.)
    // - [x] support keybindings
    // - [ ] support themes (at least light and dark)
    //   - [x] rename to theme
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
