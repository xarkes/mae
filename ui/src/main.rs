mod draw;
mod imui;
mod os;
mod render;

use render::V4f32;

use imui::{IMUI, UISize};
use notify::{RecursiveMode, Watcher, event::DataChange};
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

type Color = V4f32;
impl Color {
    pub fn from_text(text: &str) -> Self {
        if text.len() < 4 {
            Color {
                r: 1.,
                g: 1.,
                b: 1.,
                a: 1.,
            }
        } else if text.len() == 4 && text.as_bytes()[0] == b'#' {
            let bytes = text.as_bytes();
            let mut vals: [f32; 3] = [0., 0., 0.];
            for i in 0..3 {
                let b = bytes[1 + i];
                let mut val = 0;
                if b >= b'0' && b <= b'9' {
                    val = b - b'0';
                } else if b >= b'a' && b <= b'f' {
                    val = b - b'a' + 10;
                } else if b >= b'A' && b <= b'F' {
                    val = b - b'A' + 10;
                }
                vals[i] = val as f32 / 16.;
            }
            Color {
                r: vals[0],
                g: vals[1],
                b: vals[2],
                a: 1.,
            }
        } else {
            Color {
                r: 1.,
                g: 1.,
                b: 1.,
                a: 1.,
            }
        }
    }
}

// TODO: Maybe use a DeriveMacro that will apply to each enum value
// and allow you to use it at runtime as well as string with e.g. implementing two different functions
// basically the macro should take the enum values name and params and maybe we can have some logic to just put it all together
// like ui.<val_name>(...)
// using a properly designed drawing API
// use mae_macros::RACEnum;
// #[derive(RACEnum)]
enum XMLTag {
    Label(String, Option<Color>),
    Button(
        Option<String>,
        Option<Color>,
        Option<UISize>,
        Option<UISize>,
    ),
    Widget,
}
impl XMLTag {
    pub fn to_imui(&self, ui: &mut IMUI) {
        match self {
            XMLTag::Label(label, color) => {
                if let Some(color) = color {
                    ui.rparams().color(*color);
                }
                ui.label(&label);
            }
            XMLTag::Button(label, color, width, height) => {
                if let Some(color) = color {
                    ui.rparams().color(*color);
                }
                let label = match label {
                    Some(txt) => Some(txt.as_str()),
                    None => None,
                };
                if let Some(width) = width {
                    ui.rparams().width(*width);
                }
                if let Some(height) = height {
                    ui.rparams().height(*height);
                }
                ui.button(label);
            }
            _ => {}
        }
    }

    pub fn to_rust(&self) -> String {
        let mut output = String::new();
        match self {
            XMLTag::Label(label, color) => {
                if let Some(color) = color {
                    output.push_str(
                        format!(
                            "ui.rparams().color(V4f32{{r: {}, g: {}, b: {}, a: {}}});\n",
                            color.r, color.g, color.b, color.a
                        )
                        .as_str(),
                    );
                }
                output.push_str(format!("ui.label(\"{}\");\n", label.as_str()).as_str());
            }
            _ => {}
        }
        output
    }
}

struct UIFile {
    filename: String,
    tags: Vec<XMLTag>,
}

impl UIFile {
    pub fn new(filename: &str) -> Self {
        UIFile {
            filename: String::from(filename),
            tags: Vec::new(),
        }
    }
    pub fn parse(&mut self) {
        self.tags.clear();
        let data = fs::read_to_string(&self.filename).expect("Cannot read file");
        match roxmltree::Document::parse(&data) {
            Ok(doc) => {
                for node in doc.descendants() {
                    if !node.is_element() {
                        continue;
                    }
                    if node.has_tag_name("label") {
                        if let Some(text) = node.text() {
                            self.tags.push(XMLTag::Label(
                                String::from(text),
                                match node.attribute("color") {
                                    Some(color) => Some(Color::from_text(color)),
                                    None => None,
                                },
                            ));
                        }
                    }
                    if node.has_tag_name("button") {
                        if let Some(text) = node.text() {}
                        let width = match node.attribute("width") {
                            Some(width) => Some(UISize::from_str(width)),
                            None => None,
                        };
                        let height = match node.attribute("height") {
                            Some(height) => Some(UISize::from_str(height)),
                            None => None,
                        };
                        self.tags.push(XMLTag::Button(
                            match node.text() {
                                Some(text) => Some(String::from(text)),
                                None => None,
                            },
                            match node.attribute("color") {
                                Some(color) => Some(Color::from_text(color)),
                                None => None,
                            },
                            width,
                            height,
                        ));
                    }
                }
            }
            Err(e) => {
                println!("Error: {}", e);
            }
        };
        println!("{}", self.to_rust());
    }
    pub fn to_imui(&self, ui: &mut IMUI) {
        let root = ui.root.clone();
        ui.params().parent(root);

        for tag in &self.tags {
            tag.to_imui(ui);
        }
    }

    pub fn to_rust(&self) -> String {
        let start = "
use imui::IMUI;
use imui::render::V4f32;

fn main() {
  let mut ui = IMUI::new(1024, 768);
  ui.eventloop(|ui| {
    let root = ui.root.clone();
    ui.params().parent(root);\n\n";
        let mut content = String::from(start);
        for tag in &self.tags {
            content.push_str(tag.to_rust().as_str());
        }
        content.push_str("\n  }\n}");
        content
    }
}

//
// Petit recap 04/07
// - faire un systeme auto generatif, le code utilise au runtime doit savoir se representer en string a l'identique
//   -> le but est d'avoir aucune divergence entre le code du runtime dev, et le code genere et bundle pour l'utilisateur final
// - etudier et implementer un systeme de layout utile et performant
//   -> idealement il devrait permettre d'obtenir un resultat d'une seule et unique facon, sinon j'imagine que ca signifie que tu as de la redondance ? (peut-etre pas e.g. margin et padding peuvent permettre d'obtenir le meme resultat dans certains cas, mais sont-ils redondants ? pas vraiment)
// - implementer un style par defaut
// - TODO: Important: continuer de jouer avec le bundler d'interface pour voir les limitations sur le systeme auto generatif, etc. je pense en particulier au code dynamique
// - TODO: Important: etudier la question de la compilation a la volee pour le code dynamique
// - TODO: Problematiques d'API sur la facon dont on passe les arguments pour le style de chaque widget (comment etre simple d'utilisation et performant ?)
// - TODO: Portage mobile Android iOS, quels challenges ?
//   - ANDROID
//     - utilisation de game-activity_static.a et MainActivity extends GameActivity -> sucks car il manque des symboles C++ lors du runtime
//     - utilisation de
//
//
// Objectif final: Pouvoir faire une demo de creation d'interface belle (style par defaut) et rapide (systeme de layout simple + systeme auto generatif)

fn main() {
    // xarkes: parse file
    let filename = "./ui.xml";
    let uifile = Arc::new(Mutex::new(UIFile::new(filename)));
    uifile.lock().unwrap().parse();
    let cloned_uifile = Arc::clone(&uifile);

    // xarkes: activate io watcher
    let mut watcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            match res {
                Ok(event) => {
                    // println!("event: {:?}", event);
                    match event.kind {
                        notify::EventKind::Modify(notify::event::ModifyKind::Data(
                            DataChange::Content,
                        )) => {
                            cloned_uifile.lock().unwrap().parse();
                        }
                        _ => {}
                    }
                }
                Err(e) => println!("watch error: {:?}", e),
            }
        })
        .expect("Could not create watcher");
    watcher
        .watch(Path::new(filename), RecursiveMode::Recursive)
        .expect("Watch failed");

    // xarkes: initialize UI and print as parsing happens
    let mut ui = IMUI::new(1024, 768);
    ui.eventloop(|ui| {
        ui.params();
        uifile.lock().unwrap().to_imui(ui);
    });
}
