use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Error;
use std::io::ErrorKind;
use std::rc::Rc;

#[cfg(target_os = "macos")]
const HOME_FOLDER: &str = "/Users/user/notes";
#[cfg(target_os = "linux")]
const HOME_FOLDER: &str = "/home/user/notes";
#[cfg(target_os = "windows")]
const HOME_FOLDER: &str = "W:\\notes";

use rusqlite::{Connection, Result};

#[derive(Clone)]
pub struct Note {
    pub id: i64,
    pub name: String,
    pub content: String,
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
            "CREATE TABLE IF NOT EXISTS note (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, content TEXT NOT NULL)",
            (),
        )
        .unwrap();
    }

    fn conform_note(&self, note: &mut Note, writing: bool) {
        // xarkes: replace empty title with the beginning of the document
        // XXX: currently note name is useless
        let mut len = std::cmp::min(note.content.len(), 80);
        let mut pos = usize::MAX;
        while pos == usize::MAX && len > 0 {
            pos = match note.content.char_indices().nth(len) {
                Some(idx) => idx.0,
                None => usize::MAX,
            };
            len -= 1;
        }
        if pos != usize::MAX {
            let content_slice = &note.content[..pos];
            note.name = String::from(content_slice.replace('\n', ""));
        }
        if note.name.len() == 0 && !writing {
            note.name = String::from("(empty note)");
        }
    }

    pub fn all_notes(&self) -> HashMap<i64, Note> {
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

    fn add_note(&self, note: &Note) {
        self.conn
            .execute(
                "INSERT INTO note (name, content) VALUES (?1, ?2)",
                (&note.name, &note.content),
            )
            .unwrap();
    }

    fn save_note(&self, note: &Note) {
        self.conn
            .execute(
                "REPLACE INTO note (id, name, content) VALUES (?1, ?2, ?3)",
                (&note.id, &note.name, &note.content),
            )
            .unwrap();
    }

    pub fn save_all(&self, notes: &mut HashMap<i64, Note>) {
        for note in notes {
            let mut note = note.1;
            self.conform_note(&mut note, true);
            self.save_note(&note);
        }
    }
}

pub(crate) struct NoteApp {
    db: Database,
    notes: HashMap<i64, Note>,
    curnote: i64,
    pub buffer: Rc<RefCell<String>>,
}
impl NoteApp {
    pub fn new() -> Self {
        // xarkes: app starts, access the current notes
        if !std::fs::exists(HOME_FOLDER).is_ok_and(|x| x == true) {
            std::fs::create_dir(HOME_FOLDER).unwrap();
        }

        let db_file = std::path::Path::new(HOME_FOLDER).join("data.db");
        let db = Database::new(db_file.as_path());
        db.init();

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
        let first_note = notes.values().nth(0).unwrap();
        let id = first_note.id;
        let buffer = Rc::new(RefCell::new(first_note.content.clone()));
        NoteApp {
            db,
            notes,
            curnote: id,
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

    pub fn open(&mut self, id: i64) {
        self.save_current();
        self.curnote = id;
        self.buffer = Rc::new(RefCell::new(self.notes.get(&id).unwrap().content.clone()));
    }

    pub fn new_note(&mut self) {
        // do nothing if current note is empty
        if self.buffer.borrow().is_empty() {
            return;
        }

        // xarkes: save current buffer to note
        self.save_current();

        // get highest id
        let mut id: i64 = 0;
        for k in self.notes.keys() {
            if k > &id {
                id = *k;
            }
        }

        // create new note, and switch to it
        let id: i64 = id + 1;
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

    pub fn current_note_id(&self) -> i64 {
        self.curnote
    }

    pub fn import_from_markdown(&self, folder: &std::path::Path) -> Result<(), Error> {
        if !std::fs::exists(folder).is_ok_and(|exists| exists == true) {
            return Err(Error::new(ErrorKind::NotFound, "Markdown folder not found"));
        }

        for file in std::fs::read_dir(folder).unwrap() {
            let file = file.unwrap();
            if file.file_type().unwrap().is_file()
                && file.file_name().into_string().unwrap().ends_with(".md")
            {
                // import single markdown file
                let content = std::fs::read_to_string(file.path()).unwrap();
                let mut new_content = String::new();
                for line in content.lines() {
                    new_content.push_str(line.trim_end());
                    new_content.push('\n');
                }
                let mut note = Note {
                    id: 0,
                    name: String::from(""),
                    content: new_content,
                };
                self.db.conform_note(&mut note, true);
                self.db.add_note(&note);
            }
        }

        Ok(())
    }
}
