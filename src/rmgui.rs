use std::cell::RefCell;
use std::rc::Rc;

use crate::draw::Drawer;
use crate::widgets::Draw;

pub struct RMGUI {
    children: Vec<Rc<RefCell<dyn Draw>>>,
    drawer: Drawer,
}
impl RMGUI {
    pub fn new(drawer: Drawer) -> Self {
        RMGUI {
            drawer,
            children: Vec::new(),
        }
    }
    pub fn add(&mut self, node: Rc<RefCell<dyn Draw>>) {
        self.children.push(node);
    }
    pub fn draw(&self) {
        for node in &self.children {
            node.borrow().draw(&self.drawer);
        }
    }
}
