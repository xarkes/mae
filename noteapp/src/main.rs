use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use mae::imui;
use mae::imui::IMUI;
use mae::imui::UISize;
use mae::imui::color_rgb;
use mae::uisize;

#[cfg(target_os = "macos")]
const HOME_FOLDER: &str = "/Users/user/notes";
#[cfg(target_os = "linux")]
const HOME_FOLDER: &str = "/home/user/notes";

struct Note {
    filename: String,
    filepath: std::path::PathBuf,
    buffer: Option<Rc<RefCell<String>>>,
}
struct NoteApp {
    dir: String,
    notes: Vec<Note>,
}
impl NoteApp {
    pub fn new(dir: &str) -> Self {
        // xarkes: app starts, access the current notes
        if !std::fs::exists(dir).is_ok_and(|x| x == true) {
            std::fs::create_dir(dir).unwrap();
        }

        // xarkes: retrieve all notes from file system
        let mut notes = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                // TODO
            } else {
                println!("Loading note: {}", path.as_os_str().to_str().unwrap());
                notes.push(Note {
                    filename: String::from(path.file_name().unwrap().to_str().unwrap()),
                    filepath: path,
                    buffer: None,
                });
            }
        }

        NoteApp {
            dir: String::from(dir),
            notes,
        }
    }

    pub fn get_buffer(&mut self) -> Option<Rc<RefCell<String>>> {
        if self.notes.is_empty() {
            None
        } else {
            let curnote = self.note();
            let buf = curnote.buffer.as_ref();
            if buf.is_none() {
                curnote.buffer = Some(Rc::new(RefCell::new(
                    std::fs::read_to_string(&curnote.filepath).unwrap(),
                )));
            }
            Some(curnote.buffer.as_ref().unwrap().clone())
        }
    }

    pub fn new_buffer(&mut self) {
        self.notes.push(Note {
            filename: String::from("newfile"),
            filepath: PathBuf::new(),
            buffer: Some(Rc::new(RefCell::new(String::new()))),
        });
    }

    pub fn note(&mut self) -> &mut Note {
        self.notes.last_mut().unwrap()
    }
}

fn main() {
    // xarkes: init notes
    let mut noteapp = NoteApp::new(HOME_FOLDER);

    // xarkes: draw UI
    let mut ui = IMUI::new(1024, 768);
    let mut changecount = 0;

    ui.eventloop(|ui| {
        ui.params()
            .width(uisize!("100%"))
            .height(uisize!("100%"))
            .position((uisize!("0px"), uisize!("0px")))
            .background_color(color_rgb(10, 20, 30));
        ui.textarea(noteapp.get_buffer().unwrap().clone(), "#textarea");

        ui.params()
            // .width(UISize::TextContent)
            // .height(UISize::TextContent)
            .width(uisize!("100px"))
            .height(uisize!("40px"))
            .position((uisize!("90%"), uisize!("90%")));
        if ui.button(Some("New note")).borrow().clicked() {
            println!("Click on two");
        };

        // ui.vertical(|ui| {
        //     ui.horizontal(|ui| {
        //         ui.label(noteapp.note().filename.as_str());
        //         if ui.text_input_changecount().unwrap_or(0) != changecount {
        //             ui.label("   dirty...")
        //         } else {
        //             ui.label("   ok!")
        //         }
        //     });
        //     let area = ui.textarea(noteapp.get_buffer().unwrap().clone(), "maintextarea");
        //     area
        // });

        // // TODO:
        // // Important stuff:
        // // 1. proper implementation for event handling
        // // 2. animation support
        // // 3. shortcut support
        // // 4. scrollable textarea

        // // floating add button
        // ui.params()
        //     .position((UISize::Percents(0.9), UISize::Percents(0.9)));
        // if ui.button(Some("New note")).borrow().clicked() {
        //     println!("I AM CLICKED THANK YOU");
        //     noteapp.new_buffer();
        // }
    });
}
