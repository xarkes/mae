use std::{cell::RefCell, rc::Rc};

use crate::{
    os::{OSKey, OSKeyCode},
    render::{Point, RectCoords},
};

use super::{FontCache, UIWidgetRef};

pub struct IMUITextInputState {
    // focus: String,
    focus: UIWidgetRef,
    buffer: Rc<RefCell<String>>,
    idx: usize,
    cursor_col: usize,
    cursor_row: usize,
    pub(crate) cursor_x: f32,
    pub(crate) cursor_y: f32,
    multiline: bool,
    font_cache: Rc<RefCell<FontCache>>,
    pub(crate) changecount: usize,
}
impl IMUITextInputState {
    pub fn compute_valid_cursor_loc(
        &mut self,
        bounds: &RectCoords,
        text_buffer: &String,
        font_size: f32,
        point: Point,
    ) {
        let relative_x = point.0 - bounds.x0;
        let relative_y = point.1 - bounds.y0;
        if relative_x < 0. || relative_y < 0. {
            return;
        }

        // xarkes: first, get the corresponding line
        let line_height = self.font_cache.borrow().line_height(font_size);
        let line_number = (relative_y / line_height) as usize;
        self.cursor_row = std::cmp::min(line_number, text_buffer.lines().count());
        let cursor_y = line_height * self.cursor_row as f32;

        // xarkes: get the line's length and set final cursor position
        let lines = text_buffer.lines();
        let mut buffer_idx = 0;
        let mut cursor_x = 0.;
        for (i, line) in lines.enumerate() {
            if i < self.cursor_row {
                buffer_idx += line.len() + 1; // XXX: Are we sure this line.len() counts \r on Windows?
                continue;
            }
            let idx;
            (cursor_x, idx) = self
                .font_cache
                .borrow_mut()
                .get_cursor_position(font_size, line, relative_x);
            self.cursor_col = idx;
            buffer_idx += idx;
            break;
        }

        self.idx = buffer_idx;
        self.cursor_x = cursor_x;
        self.cursor_y = cursor_y;
    }
    pub fn new(
        // id: String,
        id: UIWidgetRef,
        font_cache: Rc<RefCell<FontCache>>,
        text_buffer: Rc<RefCell<String>>,
        multiline: bool,
    ) -> Self {
        IMUITextInputState {
            // focus: String::from(id),
            focus: id,
            buffer: text_buffer.clone(),
            idx: 0,
            cursor_col: 0,
            cursor_row: 0,
            cursor_x: 0.,
            cursor_y: 0.,
            multiline,
            font_cache,
            changecount: 0,
        }
    }
    fn update_cursor_loc(&mut self, idx: usize) {
        self.idx = idx;
        let buf = self.buffer.borrow();
        let mut curidx = 0;
        let font_size = 12.; // XXX
        let mut fc = self.font_cache.borrow_mut();
        for (lineidx, line) in buf.lines().enumerate() {
            if self.idx <= curidx + line.len() {
                // this is current line, compute proper x
                let mut length = 0.;
                let mut col = 0;
                for c in line.chars() {
                    let (glyph, _) = fc.get(c, 12.); // XXX: font_size
                    if let Some(glyph) = glyph {
                        if curidx + col < self.idx {
                            length += glyph.advance;
                        } else {
                            break;
                        }
                    }
                    col += 1;
                }
                // update whole state
                self.cursor_col = col;
                self.cursor_row = lineidx;
                self.cursor_x = length;
                self.cursor_y = fc.line_height(font_size) * self.cursor_row as f32;
                break;
            } else if self.idx == curidx + line.len() + 1 {
                // if we are at the '\n', go to next line instead
                self.cursor_col = 0;
                self.cursor_row = lineidx + 1;
                self.cursor_x = 0.;
                self.cursor_y = fc.line_height(font_size) * self.cursor_row as f32;
                break;
            }
            curidx += line.len() + 1; // +1 for '\n'
        }
    }
    pub fn handle_event(&mut self, key: &OSKey, chars: &Option<String>) {
        match key {
            OSKey::Keyboard(keycode) => {
                let mut bufchanged = false;
                match keycode {
                    OSKeyCode::KeyBackspace => {
                        if self.idx > 0 {
                            self.buffer.borrow_mut().remove(self.idx - 1);
                            bufchanged = true;
                            self.update_cursor_loc(self.idx - 1);
                        }
                    }
                    OSKeyCode::KeyLeftArrow => {
                        if self.idx > 0 {
                            self.update_cursor_loc(self.idx - 1);
                        }
                    }
                    OSKeyCode::KeyRightArrow => {
                        if self.idx < self.buffer.borrow().len() {
                            self.update_cursor_loc(self.idx + 1);
                        }
                    }
                    OSKeyCode::KeyDownArrow => {
                        if self.multiline {
                            let new_idx = {
                                let buf = self.buffer.borrow();
                                let line_num = self.cursor_row + 1;
                                let mut idx = 0;
                                for (i, line) in buf.lines().enumerate() {
                                    if i == line_num {
                                        idx += self.cursor_col;
                                        break;
                                    }
                                    idx += line.len() + 1; // +1 for '\n'
                                }
                                idx
                            };
                            self.update_cursor_loc(new_idx);
                        }
                    }
                    OSKeyCode::KeyUpArrow => {
                        if self.multiline {
                            let new_idx = {
                                let buf = self.buffer.borrow();
                                let lines = buf.lines();
                                let line_num = match self.cursor_row {
                                    0 => 0,
                                    _ => self.cursor_row - 1,
                                };
                                let mut idx = 0;
                                for (i, line) in lines.enumerate() {
                                    if i == line_num {
                                        idx += std::cmp::min(line.len(), self.cursor_col);
                                        break;
                                    }
                                    idx += line.len() + 1; // +1 for '\n'
                                }
                                idx
                            };
                            self.update_cursor_loc(new_idx);
                        }
                    }
                    OSKeyCode::KeyEnter => {
                        if self.multiline {
                            self.buffer.borrow_mut().insert_str(self.idx, "\n");
                            bufchanged = true;
                            self.update_cursor_loc(self.idx + 1);
                        }
                    }
                    _ => {
                        self.buffer
                            .borrow_mut()
                            .insert_str(self.idx, chars.as_ref().unwrap().as_str());
                        bufchanged = true;
                        self.update_cursor_loc(self.idx + 1);
                    }
                }

                if bufchanged {
                    // self.focus.borrow_mut().events |= UIWidgetEvent::KeyPressed as u64;
                    self.changecount += 1;
                }
            }
            _ => {}
        }
    }
}
