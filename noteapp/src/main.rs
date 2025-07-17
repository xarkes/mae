use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use mae::imui;
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

    pub fn new_buffer(&mut self) {
        self.notes.push(Note {
            filename: String::from("newfile"),
            filepath: PathBuf::new(),
            buffer: Some(Rc::new(RefCell::new(String::new()))),
        });
    }
}

fn main() {
    // xarkes: init notes
    let mut noteapp = NoteApp::new(HOME_FOLDER);

    // xarkes: draw UI
    let mut ui = IMUI::new(1024, 768);

    ui.eventloop(|ui| {
        // top label
        // ui.params()
        //     .size(UISize::Percents(1.), UISize::Pixels(40.))
        //     .text_align(mae::imui::UITextAlign::Center);
        // ui.horizontal(|ui| {
        //     ui.label(noteapp.notes.last().unwrap().filename.as_str());
        //     ui.label("right")
        // });

        // textarea
        // ui.params()
        //     .size(UISize::Percents(1.), UISize::Percents(0.9));
        let tx = ui.textarea(noteapp.get_buffer().unwrap().clone(), "maintextarea");
        if tx.borrow().clicked() {
            // println!("tx clicked");
        }
        // new note button
        // ui.params()
        //     .parent(tx)
        //     .size(UISize::DPixels(30.), UISize::DPixels(30.))
        // .position(imui::UIPosition::Fixed(
        //     UISize::Percents(0.5),
        //     UISize::Percents(1.),
        // ))
        // .color(imui::Color::from_text("#4466ff"));
        // let but = ui.button(None);
        // if but.borrow().clicked() {
        //     // TODO(xarkes): We have to properly handle the events hierarchy (depth, who handles the click?)
        //     // same problem as with the layout: how do you know if there is a widget above you catching the click event if you are handling the events while being drawn?
        //     println!("clickme");
        //     noteapp.new_buffer();
        // }
    });
}
