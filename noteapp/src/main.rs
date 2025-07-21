use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Error;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::rc::Rc;

use mae::imui::IMUI;
use mae::imui::UISize;
use mae::imui::color_rgb;
use mae::uisize;

#[cfg(target_os = "macos")]
const HOME_FOLDER: &str = "/Users/user/notes";
#[cfg(target_os = "linux")]
const HOME_FOLDER: &str = "/home/user/notes";

use rusqlite::{Connection, Result};

#[derive(Clone)]
struct Note {
    id: u64,
    name: String,
    content: String,
}

struct Database {
    conn: Connection,
}
impl Database {
    pub fn new(dbfile: &std::path::Path) -> Self {
        let conn = Connection::open(dbfile).unwrap();
        Database { conn }
    }

    pub fn init(&self) {
        self.conn.execute(
            "CREATE TABLE note (id INTEGER PRIMARY KEY, name TEXT NOT NULL, content TEXT NOT NULL)",
            (),
        )
        .unwrap();
    }

    pub fn import_from_markdown(&self, folder: &std::path::Path) -> Result<(), Error> {
        if !std::fs::exists(folder).is_ok_and(|exists| exists == true) {
            return Err(Error::new(ErrorKind::NotFound, "Markdown folder not found"));
        }

        for file in std::fs::read_dir(folder) {
            // TODO(xarkes) -> read all and import into database
        }

        Ok(())
    }

    fn conform_note(&self, note: &mut Note, writing: bool) {
        // xarkes: always add newline at the end, this helps our textarea
        if note.content.len() == 0 || note.content.chars().last().unwrap() != '\n' {
            note.content.push_str("\n");
        }

        // xarkes: replace empty title with the beginning of the document
        // XXX: currently note name is useless
        let len = std::cmp::min(note.content.len(), 30);
        let content_slice = &note.content[..len];
        note.name = String::from(content_slice.replace('\n', ""));
        if note.name.len() == 0 && !writing {
            note.name = String::from("(empty note)");
        }
    }

    pub fn all_notes(&self) -> HashMap<u64, Note> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, content FROM note")
            .unwrap();
        let note_iter = stmt
            .query_map([], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    content: row.get(2)?,
                })
            })
            .unwrap();
        let mut notes = HashMap::new();
        for note in note_iter {
            let mut note = note.unwrap();
            self.conform_note(&mut note, false);
            notes.insert(note.id, note);
        }
        notes
    }

    pub fn save_all(&self, notes: &mut HashMap<u64, Note>) {
        for note in notes {
            let mut note = note.1;
            self.conform_note(&mut note, true);
            self.conn
                .execute(
                    "REPLACE INTO note (id, name, content) VALUES (?1, ?2, ?3)",
                    (&note.id, &note.name, &note.content),
                )
                .unwrap();
        }
    }
}

struct NoteApp {
    db: Database,
    notes: HashMap<u64, Note>,
    curnote: u64,
    buffer: Rc<RefCell<String>>,
}
impl NoteApp {
    pub fn new(dir: &str) -> Self {
        // xarkes: app starts, access the current notes
        if !std::fs::exists(dir).is_ok_and(|x| x == true) {
            std::fs::create_dir(dir).unwrap();
        }

        let db_file = std::path::Path::new(dir).join("data.db");
        let db = match std::fs::exists(&db_file).unwrap_or(false) {
            true => Database::new(db_file.as_path()),
            false => {
                let db = Database::new(db_file.as_path());
                db.init();
                db
            }
        };

        let mut notes = db.all_notes();
        if notes.len() == 0 {
            notes.insert(
                0,
                Note {
                    id: 0,
                    name: String::from(""),
                    content: String::from(""),
                },
            );
        }
        let buffer = Rc::new(RefCell::new(notes.get(&0).unwrap().content.clone()));
        NoteApp {
            db,
            notes,
            curnote: 0,
            buffer,
        }
    }

    fn save_current(&mut self) {
        self.notes.get_mut(&self.curnote).unwrap().content = self.buffer.borrow().clone();
    }

    pub fn save(&mut self) {
        self.save_current();
        self.db.save_all(&mut self.notes);
    }

    pub fn open(&mut self, id: u64) {
        self.save_current();
        self.curnote = id;
        self.buffer = Rc::new(RefCell::new(self.notes.get(&id).unwrap().content.clone()));
    }

    pub fn newnote(&mut self) {
        // xarkes: save current buffer to note
        self.save_current();

        // get highest id
        let mut id = 0;
        for k in self.notes.keys() {
            if k > &id {
                id = *k;
            }
        }

        // create new note, and switch to it
        let id = id + 1;
        self.notes.insert(
            id,
            Note {
                id,
                name: String::from(""),
                content: String::from(""),
            },
        );
        self.curnote = id;
        self.buffer = Rc::new(RefCell::new(String::from("")));
    }

    pub fn notes(&self) -> Vec<Note> {
        Vec::from_iter(self.notes.values().cloned())
    }
}

fn main() {
    // xarkes: init notes
    let mut noteapp = NoteApp::new(HOME_FOLDER);

    // xarkes: draw UI
    let mut ui = IMUI::new(1024, 768);

    let mut show_search = false;
    let mut search = Rc::new(RefCell::new(String::from("")));
    ui.eventloop(|ui| {
        ui.params()
            .width(uisize!("100%"))
            .height(uisize!("100%"))
            .position((uisize!("0px"), uisize!("0px")))
            .background_color(color_rgb(10, 20, 30));
        ui.textarea(noteapp.buffer.clone(), "#textarea");

        ui.params()
            // .width(UISize::TextContent)
            // .height(UISize::TextContent)
            .width(uisize!("100px"))
            .height(uisize!("40px"))
            .position((uisize!("90%"), uisize!("90%")));
        if ui.button("Save").borrow().clicked() {
            noteapp.save();
        };

        ui.params()
            .width(uisize!("100px"))
            .height(uisize!("40px"))
            .position((uisize!("80%"), uisize!("90%")));
        if ui.button("New").borrow().clicked() {
            noteapp.newnote();
        };

        ui.params()
            .width(uisize!("100px"))
            .height(uisize!("40px"))
            .position((uisize!("70%"), uisize!("90%")));
        if ui.button("Search").borrow().clicked() {
            show_search = true;
            search = Rc::new(RefCell::new(String::from("")));
        }

        // Prompts
        if show_search {
            ui.params()
                .width(uisize!("50%"))
                .height(uisize!("50%"))
                .position((uisize!("25%"), uisize!("40px")));
            ui.floating_box(|ui| {
                ui.params().width(uisize!("100%")).height(uisize!("20px"));
                ui.line_edit(search.clone(), "#search");
                ui.params().width(uisize!("100%")).height(uisize!("20px"));
                let search_filter = search.borrow();
                for note in &noteapp.notes() {
                    if search_filter.len() > 0 {
                        if !fuzzy_search(search_filter.as_str(), note.name.as_str()) {
                            continue;
                        }
                    }
                    if ui
                        .button(format!("{}##button_label_{}", note.name, note.id).as_str())
                        .borrow()
                        .clicked()
                    {
                        noteapp.open(note.id);
                        show_search = false;
                    }
                }
            });
        };
    });

    // TODO:
    // Implement search/go to button
    // --> dropdown menu a la command palette
    // --> implement animation
    // --> add shortcuts
}

fn fuzzy_search(filter: &str, data: &str) -> bool {
    // TODO(xarkes): Implement proper search
    data.contains(filter)
}
