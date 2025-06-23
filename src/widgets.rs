use std::{cell::RefCell, rc::Rc};

use crate::draw::Drawer;

pub trait Draw {
    fn draw(&self, drawer: &Drawer);
}

pub struct Label {
    pub text: String,
}
impl Label {
    pub fn new(text: String) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Label { text }))
    }
}
impl Draw for Label {
    fn draw(&self, drawer: &Drawer) {
        let x = 0;
        let y = 0;
        drawer.draw_text(x, y, 12, self.text.as_str());
    }
}

pub struct TextArea {
    text: String,
}
impl TextArea {
    pub fn new(text: String) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(TextArea { text }))
    }
}
impl Draw for TextArea {
    fn draw(&self, drawer: &Drawer) {
        let (winx, winy) = drawer.renderer.borrow_mut().size();
        let x = 0;
        let y = 0;

        let mut yoff = 0;
        let width = winx - x;
        let nchars = width / (12 / 2);
        for line in self.text.split('\n') {
            // TODO(xarkes): This sucks due to reallocation
            let mut line = line.to_string();
            line.truncate(nchars as usize);
            drawer.draw_text(x, y + yoff, 12, line.as_str());
            yoff += 14;

            // xarkes: Don't draw not visible lines
            if y + yoff > winy {
                break;
            }
        }
    }
}
