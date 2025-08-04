use std::{cell::RefCell, rc::Rc};

use crate::{
    os::{OSKey, OSKeyCode},
    render::RectCoords,
};

use super::{FontCache, Point, uibox::UIBoxRef};

pub struct IMUITextInputState {
    // focus: String,
    pub(crate) focus: UIBoxRef,
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
    pub fn new(
        // id: String,
        uibox: UIBoxRef,
        font_cache: Rc<RefCell<FontCache>>,
        text_buffer: Rc<RefCell<String>>,
        multiline: bool,
    ) -> Self {
        IMUITextInputState {
            // focus: String::from(id),
            focus: uibox,
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
    pub fn compute_valid_cursor_loc(
        &mut self,
        bounds: &RectCoords,
        text_buffer: &String,
        font_size: f32,
        point: Point,
        delta: Point,
    ) {
        let relative_x = point.x - bounds.x0 - delta.x;
        let relative_y = point.y - bounds.y0 - delta.y;
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
                buffer_idx += line.chars().count() + 1; // +1 for '\n'
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
    fn update_cursor_loc(&mut self, idx: usize) {
        self.idx = idx;
        let buf = self.buffer.borrow();
        let mut curidx = 0;
        let font_size = self.focus.borrow().style.font_size;
        let mut fc = self.font_cache.borrow_mut();
        for (lineidx, line) in buf.lines().enumerate() {
            if self.idx <= curidx + line.chars().count() {
                // xarkes: this is current line, compute proper x
                let mut length = 0.;
                let mut col = 0;
                for c in line.chars() {
                    let glyph = fc.get(c, font_size);
                    if curidx + col < self.idx {
                        length += glyph.advance;
                    } else {
                        break;
                    }
                    col += 1;
                }
                // xarkes: update whole state
                self.cursor_col = col;
                self.cursor_row = lineidx;
                self.cursor_x = length;
                self.cursor_y = fc.line_height(font_size) * self.cursor_row as f32;
                // xarkes: update box scrolling
                {
                    let mut uibox = self.focus.borrow_mut();
                    if self.cursor_x > uibox.size.width - uibox.scrollx
                        || self.cursor_x + uibox.scrollx < 0.
                    {
                        let line_idx = idx - curidx;
                        if line_idx == line.chars().count() {
                            uibox.scrollx = -1.
                                * (fc
                                    .get_text_size(font_size, line, line.len())
                                    .0
                                    + 2. // cursor_width
                                    - uibox.size.width);
                        } else {
                            let line_char_idx =
                                line.char_indices().map(|(i, _)| i).nth(line_idx).unwrap();
                            let line_slice = line[..line_char_idx].to_string();

                            let cursor_width = 2.;
                            let direction_right = !(self.cursor_x + uibox.scrollx < 0.);
                            if direction_right {
                                // cursor at the right
                                uibox.scrollx = -1.
                                    * (fc
                                        .get_text_size(
                                            font_size,
                                            line_slice.as_str(),
                                            line_slice.len(),
                                        )
                                        .0
                                        + cursor_width
                                        - uibox.size.width);
                            } else {
                                // cursor at the left
                                let left_char = line.chars().nth(line_idx).unwrap();
                                uibox.scrollx = f32::min(
                                    0.,
                                    uibox.scrollx
                                        + fc.get(left_char, font_size).width as f32
                                        + cursor_width,
                                );
                            }
                        }
                    }
                }
                break;
            } else if self.idx == curidx + line.chars().count() + 1 {
                // xarkes: if we are at the '\n', go to next line instead
                self.cursor_col = 0;
                self.cursor_row = lineidx + 1;
                self.cursor_x = 0.;
                self.cursor_y = fc.line_height(font_size) * self.cursor_row as f32;
                // xarkes: update box scrolling
                {
                    let mut uibox = self.focus.borrow_mut();
                    if uibox.scrollx != 0. {
                        uibox.scrollx = 0.;
                    }
                    if self.cursor_y > uibox.size.height - uibox.scrolly {
                        // cursor going down
                        uibox.scrolly -= fc.line_height(font_size);
                    } else if self.cursor_y < -1. * uibox.scrolly {
                        // cursor going up
                        uibox.scrolly += fc.line_height(font_size);
                    }
                }
                break;
            }
            curidx += line.chars().count() + 1; // +1 for '\n'
        }
    }
    pub fn handle_event(&mut self, key: &OSKey, data: &Option<char>) -> bool {
        let handled;
        let get_str_insert_idx = |idx| {
            if idx != self.buffer.borrow().len() {
                return self.buffer.borrow().char_indices().nth(idx).unwrap().0;
            }
            return self.idx;
        };
        match key {
            OSKey::Keyboard(keycode) => {
                let mut bufchanged = false;
                match keycode {
                    OSKeyCode::KeyBackspace => {
                        if self.idx > 0 {
                            let byte_idx = get_str_insert_idx(self.idx - 1);
                            self.buffer.borrow_mut().remove(byte_idx);
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
                                    idx += line.chars().count() + 1; // +1 for '\n'
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
                                    idx += line.chars().count() + 1; // +1 for '\n'
                                }
                                idx
                            };
                            self.update_cursor_loc(new_idx);
                        }
                    }
                    OSKeyCode::KeyEnter => {
                        if self.multiline {
                            let byte_idx = get_str_insert_idx(self.idx);
                            self.buffer.borrow_mut().insert(byte_idx, '\n');
                            bufchanged = true;
                            self.update_cursor_loc(self.idx + 1);
                        }
                    }
                    _ => {
                        if let Some(data) = data {
                            let byte_idx = get_str_insert_idx(self.idx);
                            self.buffer.borrow_mut().insert(byte_idx, *data);
                            bufchanged = true;
                            self.update_cursor_loc(self.idx + 1);
                        }
                    }
                }

                if bufchanged {
                    // self.focus.borrow_mut().events |= UIWidgetEvent::KeyPressed as u64;
                    self.changecount += 1;
                }
                handled = true;
            }
            _ => {
                handled = false;
            }
        }
        handled
    }
}
