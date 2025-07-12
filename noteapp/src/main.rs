use std::cell::RefCell;
use std::rc::Rc;

use mae::imui::IMUI;
use mae::imui::UISize;

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
            let curnote = self.notes.last_mut().unwrap();
            let buf = curnote.buffer.as_ref();
            if buf.is_none() {
                curnote.buffer = Some(Rc::new(RefCell::new(
                    std::fs::read_to_string(&curnote.filepath).unwrap(),
                )));
            }
            Some(curnote.buffer.as_ref().unwrap().clone())
        }
    }
}

fn main() {
    // xarkes: init notes
    let mut noteapp = NoteApp::new(HOME_FOLDER);

    // xarkes: draw UI
    let default_buffer = noteapp.get_buffer().unwrap();
    let mut ui = IMUI::new(1024, 768);
    ui.eventloop(|ui| {
        ui.params().size(UISize::Percents(1.), UISize::Percents(1.));
        ui.textarea(default_buffer.clone(), "maintextarea");
    });
}
