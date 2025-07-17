use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[cfg(debug_assertions)]
mod debug;

#[cfg(target_os = "android")]
use android_activity::AndroidApp;
use debug::{IMUIDebug, draw_debug_info};

use crate::{
    draw::{self, Drawer},
    os::{self, OSEvent, OSEventType, OSKey, OSKeyCode},
    render::{self, Point, RectCoords, V4f32, font_cache::FontCache},
};

type UIWidgetRef = Rc<RefCell<UIBox>>;

pub mod color {
    pub const NONE: crate::render::V4f32 = crate::render::V4f32 {
        r: 0.,
        g: 0.,
        b: 0.,
        a: 0.,
    };
}

pub type Color = V4f32;
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
        } else if text.len() == 7 && text.as_bytes()[0] == b'#' {
            let bytes = text.as_bytes();
            let mut vals: [f32; 3] = [0., 0., 0.];
            for i in 0..3 {
                let mut val = 0;
                for j in 0..2 {
                    let b = bytes[1 + i * 2 + j];
                    let v;
                    if b >= b'0' && b <= b'9' {
                        v = b - b'0';
                    } else if b >= b'a' && b <= b'f' {
                        v = b - b'a' + 10;
                    } else if b >= b'A' && b <= b'F' {
                        v = b - b'A' + 10;
                    } else {
                        v = 0;
                    }
                    let v = (1 << (1 - j as u8) * 4) * v;
                    val += v;
                }
                vals[i] = val as f32 / 256.;
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

#[repr(u64)]
enum UIWidgetFlag {
    Clickable = 1u64,
    Draggable = 2u64 + 1, // Draggable implies clickable
    Resizable = 4u64 + 1, // Resizable implies clickable
}

#[repr(u64)]
enum UIWidgetEvent {
    MouseOver = 1u64,
    MouseClicked = 2u64,
    MouseReleased = 4u64,
}

pub enum UITextAlign {
    Left,
    Center,
}

#[derive(Clone, Copy)]
pub enum UISize {
    DPixels(f32), // DPI scaled pixels, in current implementation, all draws are dpi scaled
    Percents(f32),
}
impl UISize {
    pub fn from_str(input: &str) -> Self {
        let val = match i32::from_str_radix(input, 10) {
            Ok(r) => r as f32,
            Err(_) => 0.,
        };
        UISize::DPixels(val)
    }
    pub fn pixels(&self, parent_val: f32) -> f32 {
        match self {
            UISize::DPixels(val) => *val,
            UISize::Percents(val) => val * parent_val,
        }
    }
}

#[derive(Copy, Clone)]
pub enum UILayout {
    Root,        // Root specific layout, allows children to be floating over everything else
    Vertical,    // Default, natural vertical layout - results depends on the localization
    VerticalLtr, // Vertical layout, forcing left to right reading
    VerticalRtl, // Vertical layout, forcing right to left reading
    Horizontal,
    HorizontalLtr,
    HorizontalRtl,
}

pub struct UIBox {
    bounds: RectCoords,
    layout: UILayout,
    children: Vec<UIWidgetRef>,

    // event flags
    flags: u64,
    events: u64,
}
impl UIBox {
    pub fn hover(&self) -> bool {
        (self.events & UIWidgetEvent::MouseOver as u64) > 0
    }
    pub fn click(&self) -> bool {
        (self.events & UIWidgetEvent::MouseClicked as u64) > 0
    }
    pub fn clicked(&self) -> bool {
        (self.events & UIWidgetEvent::MouseReleased as u64) > 0
    }

    fn clickable(&self) -> bool {
        (self.flags & UIWidgetFlag::Clickable as u64) == UIWidgetFlag::Clickable as u64
    }
    fn draggable(&self) -> bool {
        (self.flags & UIWidgetFlag::Draggable as u64) == UIWidgetFlag::Draggable as u64
    }
    fn resizable(&self) -> bool {
        (self.flags & UIWidgetFlag::Resizable as u64) == UIWidgetFlag::Resizable as u64
    }
}

pub struct UIWidgetParams {
    default_parent: UIWidgetRef,
    parent: UIWidgetRef,
    width: UISize,
    height: UISize,
    color: V4f32,
    layout: UILayout,
    text_align: UITextAlign,
    flags: u64,
}
impl UIWidgetParams {
    pub fn new(parent: UIWidgetRef) -> Self {
        UIWidgetParams {
            default_parent: parent.clone(),
            parent,
            width: UISize::Percents(1.),
            height: UISize::Percents(1.),
            color: V4f32 {
                r: 1.,
                g: 1.,
                b: 1.,
                a: 1.,
            },
            layout: UILayout::Vertical,
            text_align: UITextAlign::Left,
            flags: 0,
        }
    }
    pub fn size(&mut self, w: UISize, h: UISize) -> &mut Self {
        self.width(w);
        self.height(h);
        self
    }
    pub fn flags(&mut self, flags: u64) -> &mut Self {
        self.flags = flags;
        self
    }
    pub fn width(&mut self, w: UISize) -> &mut Self {
        self.width = w;
        self
    }
    pub fn height(&mut self, h: UISize) -> &mut Self {
        self.height = h;
        self
    }
    pub fn parent(&mut self, parent: UIWidgetRef) -> &mut Self {
        self.parent = parent;
        self
    }
    pub fn color(&mut self, color: V4f32) -> &mut Self {
        self.color = color;
        self
    }
    pub fn layout(&mut self, layout: UILayout) -> &mut Self {
        self.layout = layout;
        self
    }
    pub fn text_align(&mut self, mode: UITextAlign) -> &mut Self {
        self.text_align = mode;
        self
    }
    pub fn reset(&mut self) {
        self.parent = self.default_parent.clone();
        self.width = UISize::Percents(1.);
        self.height = UISize::Percents(1.);
        self.color = V4f32 {
            r: 1.,
            g: 1.,
            b: 1.,
            a: 1.,
        };
        self.layout = UILayout::Vertical;
        self.text_align = UITextAlign::Left;
    }
}

#[derive(Default)]
struct IMUIEvents {
    events: Vec<OSEvent>,
    //// input events cache
    mouse: Option<Point>,
    click: Option<Point>,
    release: Option<Point>,

    drag_pos: Option<Point>,
    drag_cache: HashMap<String, Point>,
}

struct IMUITextInputState {
    focus: String,
    buffer: Rc<RefCell<String>>,
    idx: usize,
    cursor_col: usize,
    cursor_row: usize,
    cursor_x: f32,
    cursor_y: f32,
    multiline: bool,
    font_cache: Rc<RefCell<FontCache>>,
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
        id: String,
        font_cache: Rc<RefCell<FontCache>>,
        text_buffer: Rc<RefCell<String>>,
        multiline: bool,
    ) -> Self {
        IMUITextInputState {
            focus: String::from(id),
            buffer: text_buffer.clone(),
            idx: 0,
            cursor_col: 0,
            cursor_row: 0,
            cursor_x: 0.,
            cursor_y: 0.,
            multiline,
            font_cache,
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
            OSKey::Keyboard(keycode) => match keycode {
                OSKeyCode::KeyBackspace => {
                    if self.idx > 0 {
                        self.buffer.borrow_mut().remove(self.idx - 1);
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
                        self.update_cursor_loc(self.idx + 1);
                    }
                }
                _ => {
                    self.buffer
                        .borrow_mut()
                        .insert_str(self.idx, chars.as_ref().unwrap().as_str());
                    self.update_cursor_loc(self.idx + 1);
                }
            },
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
enum UILocaleKind {
    LtrTtb, // European languages
    RtlTtb, // Hebrew, Arabic like
    TtbLtr, // Mongolian like
    TtbRtl, // Japanese like
}

struct UIStyle {
    main_color: Color,
    bg_color: Color,
    text_color: Color,
    text_size: f32,
    active_color: Color,
}

impl UIStyle {
    pub fn default() -> Self {
        UIStyle {
            main_color: Color {
                r: 40. / 256.,
                g: 60. / 256.,
                b: 140. / 256.,
                a: 1.0,
            },
            bg_color: Color {
                r: 10. / 256.,
                g: 10. / 256.,
                b: 10. / 256.,
                a: 0.8,
            },
            text_color: Color {
                r: 1.,
                g: 1.,
                b: 1.,
                a: 1.,
            },
            text_size: 12.,
            active_color: Color {
                r: 1.,
                g: 0.6,
                b: 0.6,
                a: 1.,
            },
        }
    }
}

pub struct IMUI {
    drawer: Drawer,
    #[cfg(debug_assertions)]
    debug: IMUIDebug,
    size: (f32, f32),
    params: UIWidgetParams,
    event: IMUIEvents,
    text_input_state: Option<IMUITextInputState>,
    locale_kind: UILocaleKind,

    // ui construction helpers
    root: UIWidgetRef,
    parent_stack: Vec<UIWidgetRef>,
    style: UIStyle,
}
impl IMUI {
    #[cfg(not(target_os = "android"))]
    pub fn new(w: u32, h: u32) -> Self {
        let window = os::Window::new(w, h);
        IMUI::new_body(window)
    }
    #[cfg(target_os = "android")]
    pub fn android(app: AndroidApp) -> Self {
        let win = os::Window::new(app);

        // xarkes: wait for InitWindow to initialize the renderer
        win.wait_for_native_window();

        IMUI::new_body(win)
    }
    fn new_body(window: os::Window) -> Self {
        let renderer = render::Renderer::new(window);
        let drawer = draw::Drawer::new(renderer);

        let root = Rc::new(RefCell::new(UIBox {
            bounds: RectCoords::from_size(0., 0., 0., 0.),
            layout: UILayout::Root,
            flags: 0,
            events: 0,
            children: Vec::new(),
        }));
        IMUI {
            drawer,
            #[cfg(debug_assertions)]
            debug: IMUIDebug::default(),
            size: (0., 0.),
            params: UIWidgetParams::new(root.clone()),
            event: IMUIEvents::default(),
            text_input_state: None,
            locale_kind: UILocaleKind::LtrTtb,
            root: root.clone(),
            parent_stack: vec![root.clone()],
            style: UIStyle::default(),
        }
    }
    pub fn eventloop(&mut self, mut drawfunction: impl FnMut(&mut IMUI)) {
        let freq = os::timer_init();
        let mut time = 0f64;
        let mut start = os::timer_value();
        loop {
            // xarkes: handle events
            {
                self.get_events();
                self.resize();
                self.root.borrow_mut().children.clear();
            }

            // xarkes: draw interface
            {
                drawfunction(self);
            }

            #[cfg(debug_assertions)]
            {
                draw_debug_info(self, self.debug.clone(), time);
            }

            // xarkes: render
            {
                self.drawer.renderer.render_frame();
            }

            let end = os::timer_value();
            time = (end - start) as f64 * 1_000_000.0 / freq;
            start = end;
        }
    }

    /////////////////////////////////
    //// Events related functions
    fn consume_events(&mut self) {
        for ev in &self.event.events {
            if ev.ty == OSEventType::MouseMove {
                self.event.mouse = ev.pos;
            } else if ev.ty == OSEventType::Press && ev.key == OSKey::LeftMouseButton {
                self.event.click = ev.pos;
                self.event.mouse = ev.pos; // Handles when a click happens before the mouse is moved

                // xarkes: when there is a click anywhere, reset the text input global state
                // if the click happens to be on something clickable, then the widget will handle it,
                // if not this resets the current state
                self.text_input_state = None;
            } else if ev.ty == OSEventType::Release && ev.key == OSKey::LeftMouseButton {
                self.event.click = None;
                self.event.release = ev.pos;
            }

            // xarkes: consume global keyboard events
            if ev.ty == OSEventType::Press {
                if let Some(textinput) = self.text_input_state.as_mut() {
                    textinput.handle_event(&ev.key, &ev.chars);
                }
            }

            // TODO(xarkes): we may want to propagate the event back to the OS window when the application did not consume them
        }
    }
    pub fn get_events(&mut self) {
        self.event.events = self.drawer.renderer.win.get_events();
        self.consume_events();
    }
    pub fn resize(&mut self) -> Point {
        self.size = self.drawer.renderer.win.get_size();
        let render_size = self.drawer.renderer.win.get_render_size();
        self.drawer.renderer.resize(render_size.0, render_size.1);
        // let root = Rc::new(RefCell::new(UIWidget {
        //     bounds: RectCoords::from_size(0., 0., self.size.0, self.size.1),
        //     parent: None,
        //     children: Vec::new(),
        //     flags: 0,
        //     events: 0,
        // }));
        // self.root = root;
        self.root.borrow_mut().bounds.x1 = self.size.0;
        self.root.borrow_mut().bounds.y1 = self.size.1;
        self.size
    }

    /////////////////////////////////
    //// Widgets functions
    fn draw_text(&mut self, bounds: &RectCoords, text: &str, length: usize, size: f32) -> f32 {
        let text_pos = match self.locale_kind {
            UILocaleKind::LtrTtb => bounds.x0,
            UILocaleKind::RtlTtb => bounds.x1 - self.drawer.get_text_size(size, text, length).0,
            _ => {
                unimplemented!("Text display is not implemented for this locale at the moment.");
            }
        };
        self.drawer.draw_text(
            text_pos,
            bounds.y0,
            size,
            text,
            length,
            self.style.text_color,
        )
    }
    fn layout_new_widget(
        &mut self,
        id: Option<String>,
        size: (UISize, UISize),
        flags: u64,
    ) -> UIWidgetRef {
        let mut parent = self.parent_stack.last().unwrap().borrow_mut();
        // xarkes: compute bounds depending on layout and requested size
        let layout = match parent.layout {
            UILayout::Vertical => match self.locale_kind {
                UILocaleKind::LtrTtb => UILayout::VerticalLtr,
                UILocaleKind::RtlTtb => UILayout::VerticalRtl,
                _ => unimplemented!("Handle other kinds of locales!"),
            },
            UILayout::Horizontal => match self.locale_kind {
                UILocaleKind::LtrTtb => UILayout::HorizontalLtr,
                UILocaleKind::RtlTtb => UILayout::HorizontalRtl,
                _ => unimplemented!("Handle other kinds of locales!"),
            },
            _ => parent.layout,
        };
        let bounds = match layout {
            UILayout::Root => RectCoords::from_size(
                parent.bounds.x0,
                parent.bounds.y0,
                size.0.pixels(parent.bounds.width()),
                size.1.pixels(parent.bounds.height()),
            ),
            UILayout::VerticalLtr => {
                let insert_point = match parent.children.last() {
                    Some(child) => (child.borrow().bounds.x0, child.borrow().bounds.y1),
                    None => (parent.bounds.x0, parent.bounds.y0),
                };
                RectCoords::from_size(
                    insert_point.0,
                    insert_point.1,
                    f32::min(parent.bounds.width(), size.0.pixels(parent.bounds.width())),
                    f32::min(
                        parent.bounds.height(),
                        size.1.pixels(parent.bounds.height()),
                    ),
                )
            }
            UILayout::VerticalRtl => {
                let insert_point = match parent.children.last() {
                    Some(child) => (
                        child.borrow().bounds.x1 - size.0.pixels(parent.bounds.width()),
                        child.borrow().bounds.y1,
                    ),
                    None => (
                        parent.bounds.x1 - size.0.pixels(parent.bounds.width()),
                        parent.bounds.y0,
                    ),
                };

                RectCoords::from_size(
                    insert_point.0,
                    insert_point.1,
                    f32::min(parent.bounds.width(), size.0.pixels(parent.bounds.width())),
                    f32::min(
                        parent.bounds.height(),
                        size.1.pixels(parent.bounds.height()),
                    ),
                )
            }
            UILayout::HorizontalLtr => {
                let insert_point = match parent.children.last() {
                    Some(child) => (child.borrow().bounds.x1, child.borrow().bounds.y0),
                    None => (parent.bounds.x0, parent.bounds.y0),
                };
                RectCoords::from_size(
                    insert_point.0,
                    insert_point.1,
                    f32::min(parent.bounds.width(), size.0.pixels(parent.bounds.width())),
                    f32::min(
                        parent.bounds.height(),
                        size.1.pixels(parent.bounds.height()),
                    ),
                )
            }
            UILayout::HorizontalRtl => {
                let insert_point = match parent.children.last() {
                    Some(child) => (
                        child.borrow().bounds.x0 - size.0.pixels(parent.bounds.width()),
                        child.borrow().bounds.y0,
                    ),
                    None => (
                        parent.bounds.x1 - size.0.pixels(parent.bounds.width()),
                        parent.bounds.y0,
                    ),
                };
                RectCoords::from_size(
                    insert_point.0,
                    insert_point.1,
                    f32::min(parent.bounds.width(), size.0.pixels(parent.bounds.width())),
                    f32::min(
                        parent.bounds.height(),
                        size.1.pixels(parent.bounds.height()),
                    ),
                )
            }
            _ => unreachable!("Generic layout impossible here!"),
        };

        // xarkes: create box
        let mut uibox = UIBox {
            bounds,
            layout: UILayout::Vertical,
            flags,
            events: 0,
            children: Vec::new(),
        };

        // xarkes: pre-update dragged widget positions for events to work
        if uibox.draggable() {
            let id = id.as_ref().unwrap();
            if let Some(dragpos) = self.event.drag_cache.get(id) {
                // widget was dragged, update its position
                uibox.bounds.x0 += dragpos.0;
                uibox.bounds.x1 += dragpos.0;
                uibox.bounds.y0 += dragpos.1;
                uibox.bounds.y1 += dragpos.1;
            }
        }

        // xarkes: compute event flags
        let mut events = 0;
        if point_in_rect(&uibox.bounds, self.event.mouse) {
            events |= UIWidgetEvent::MouseOver as u64;
        }
        if point_in_rect(&uibox.bounds, self.event.click) && uibox.clickable() {
            events |= UIWidgetEvent::MouseClicked as u64;
        } else if point_in_rect(&uibox.bounds, self.event.release) && uibox.clickable() {
            events |= UIWidgetEvent::MouseReleased as u64;
        }
        uibox.events = events;

        // xarkes: update draggable position
        if uibox.draggable() {
            let id = id.as_ref().unwrap();
            if uibox.click() {
                if self.event.drag_pos.is_none() {
                    // save the first click
                    self.event.drag_pos = self.event.mouse;
                } else {
                    let dist_x = self.event.mouse.unwrap().0 - self.event.drag_pos.unwrap().0;
                    let dist_y = self.event.mouse.unwrap().1 - self.event.drag_pos.unwrap().1;
                    uibox.bounds.x0 += dist_x;
                    uibox.bounds.x1 += dist_x;
                    uibox.bounds.y0 += dist_y;
                    uibox.bounds.y1 += dist_y;
                }
            } else if self.event.drag_pos.is_some() {
                let old_distance = match self.event.drag_cache.get(id) {
                    Some(dist) => dist,
                    None => &(0., 0.),
                };
                let dist_x = self.event.mouse.unwrap().0 - self.event.drag_pos.unwrap().0;
                let dist_y = self.event.mouse.unwrap().1 - self.event.drag_pos.unwrap().1;
                self.event.drag_cache.insert(
                    id.clone(),
                    (old_distance.0 + dist_x, old_distance.1 + dist_y),
                );
                self.event.drag_pos = None;
                uibox.bounds.x0 += dist_x;
                uibox.bounds.x1 += dist_x;
                uibox.bounds.y0 += dist_y;
                uibox.bounds.y1 += dist_y;
            }
        }

        // create ref and push as child
        let uibox = Rc::new(RefCell::new(uibox));
        parent.children.push(uibox.clone());
        uibox
    }
    pub fn horizontal(
        &mut self,
        mut children: impl FnMut(&mut IMUI) -> UIWidgetRef,
    ) -> UIWidgetRef {
        let pane = self.layout_new_widget(None, (UISize::Percents(1.), UISize::Percents(1.)), 0);
        pane.borrow_mut().layout = UILayout::Horizontal;
        self.parent_stack.push(pane.clone());
        let out = children(self);
        self.parent_stack.pop();
        let mut pu = pane.borrow_mut();
        // XXX: This is a hack, should we allow it?
        pu.bounds.y1 = f32::min(pu.bounds.y1, out.borrow().bounds.y1);
        out
    }
    pub fn vertical(&mut self, mut children: impl FnMut(&mut IMUI) -> UIWidgetRef) -> UIWidgetRef {
        let pane = self.layout_new_widget(None, (UISize::Percents(1.), UISize::Percents(1.)), 0);
        self.parent_stack.push(pane.clone());
        let out = children(self);
        self.parent_stack.pop();
        // let mut pu = pane.borrow_mut();
        // XXX: This is a hack, should we allow it?
        // pu.bounds.y1 = f32::min(pu.bounds.y1, out.borrow().bounds.y1);
        out
    }
    // TODO:
    // - corriger gestion evenements + fenetre flottante
    // --> event stack?
    // --> multiple floating windows?
    // - separer wigets customisables et widgets basiques
    // --> l'idee c'est de fournir des API pour faire une UI jolie et facilement, rapidemment
    // --> mais aussi fournir des API pour la devapp qui permet de customiser au max
    pub fn floating_pane(&mut self, title: &str, mut children: impl FnMut(&mut IMUI)) {
        // xarkes: draw widget
        let width = 200.;
        let height = 250.;

        // XXX: layout_new_widget should be used only for non floating things, things inside a layout
        let pane = self.layout_new_widget(
            Some(format!("##pane_{}", title)),
            (UISize::DPixels(width), UISize::DPixels(height)),
            UIWidgetFlag::Draggable as u64 | UIWidgetFlag::Resizable as u64,
        );

        let pbounds = pane.borrow().bounds;
        let bar_height = 20.;
        let bar_bounds = RectCoords::from_size(pbounds.x0, pbounds.y0, pbounds.width(), bar_height);
        let bounds = RectCoords::from_size(
            pbounds.x0,
            pbounds.y0 + bar_height,
            pbounds.width(),
            pbounds.height() - bar_height,
        );
        self.drawer.draw_rect(&bar_bounds, self.style.main_color);
        self.draw_text(&bar_bounds, title, title.len(), self.style.text_size);
        self.drawer.draw_rect(&bounds, self.style.bg_color);

        // xarkes: recompute bounds
        pane.borrow_mut().bounds = RectCoords::from_size(
            bar_bounds.x0,
            bar_bounds.y0 + bar_height, // XXX: Temporary hack
            bounds.width(),
            bounds.height() + bar_height,
        );

        // xarkes: draw children
        self.parent_stack.push(pane);
        children(self);
        self.parent_stack.pop();
    }
    pub fn checkbox_widget(&mut self, value: &mut bool) -> UIWidgetRef {
        let line_height = self
            .drawer
            .renderer
            .font_cache
            .borrow()
            .line_height(self.style.text_size as f32);
        let box_size = line_height;
        let widget_r = self.layout_new_widget(
            None,
            (UISize::DPixels(box_size), UISize::DPixels(box_size)),
            UIWidgetFlag::Clickable as u64,
        );
        let widget = widget_r.borrow();

        let draw_color = match *value {
            true => self.style.bg_color,
            false => self.style.text_color,
        };
        self.drawer.draw_rect(
            &RectCoords::from_size(widget.bounds.x0, widget.bounds.y0, box_size, box_size),
            draw_color,
        );
        let border_color = match widget.hover() {
            true => self.style.active_color,
            false => self.style.main_color,
        };
        self.drawer.draw_empty_rect(
            &RectCoords::from_size(widget.bounds.x0, widget.bounds.y0, box_size, box_size),
            border_color,
            1.0,
            false,
        );

        // TODO(xarkes): We need a better API...
        if widget.clicked() && self.event.release.is_some() {
            *value = !*value;
            // NOTE(xarkes): consume the release so clicked() is called only once
            self.event.release = None;
        }

        widget_r.clone()
    }
    pub fn label(&mut self, label: &str) -> UIWidgetRef {
        let (width, _) = self
            .drawer
            .get_text_size(self.style.text_size, label, label.len());
        let height = self
            .drawer
            .renderer
            .font_cache
            .borrow()
            .line_height(self.style.text_size);
        let widget = self.layout_new_widget(
            Some(format!("##label_{}", label)),
            (UISize::DPixels(width), UISize::DPixels(height)),
            0,
        );
        self.draw_text(
            &RectCoords::from_size(
                widget.borrow().bounds.x0,
                widget.borrow().bounds.y0,
                widget.borrow().bounds.width(),
                widget.borrow().bounds.height(),
            ),
            label,
            label.len(),
            self.style.text_size,
        );
        widget
    }
    pub fn checkbox(&mut self, label: &str, value: &mut bool) -> UIWidgetRef {
        self.horizontal(|ui| {
            let checkbox = ui.checkbox_widget(value);
            ui.label(label);
            checkbox
        })
    }
    pub fn line_edit(&mut self, text_buffer: Rc<RefCell<String>>, id: &str) -> UIWidgetRef {
        let multiline = false;
        self.text_edit_impl(text_buffer, id, multiline)
    }
    pub fn textarea(&mut self, text_buffer: Rc<RefCell<String>>, id: &str) -> UIWidgetRef {
        let multiline = true;
        self.text_edit_impl(text_buffer, id, multiline)
    }
    fn text_edit_impl(
        &mut self,
        text_buffer: Rc<RefCell<String>>,
        id: &str,
        multiline: bool,
    ) -> UIWidgetRef {
        // TODO(xarkes): once you rewrite this, think of the LTR text inputs and handle it
        let textarea = self.layout_new_widget(
            Some(String::from(id)),
            (UISize::Percents(1.), UISize::Percents(1.)),
            UIWidgetFlag::Clickable as u64,
        );
        let bounds = &textarea.borrow().bounds;
        if textarea.borrow().clicked() {
            // xarkes: update the text input global state
            let mut state = IMUITextInputState::new(
                String::from(id),
                self.drawer.renderer.font_cache.clone(),
                text_buffer.clone(),
                multiline,
            );
            state.compute_valid_cursor_loc(
                bounds,
                &text_buffer.borrow(),
                self.style.text_size,
                self.event.mouse.unwrap(),
            );
            self.text_input_state = Some(state);
            // TODO(xarkes): ------------------> HERE YOU NEED TO CONSUME THE EVENTS IN ORDER LOL
            // BECAUSE TOP PANE IS NOT ABLE TO HANDLE THIS FUCK
            self.event.release = None;
        }

        // background
        self.drawer
            .draw_rect(&textarea.borrow().bounds, self.style.bg_color);

        // text
        if multiline {
            let mut y = bounds.y0;
            for (i, line) in text_buffer.borrow().lines().enumerate() {
                let x = bounds.x0;
                self.draw_text(
                    &RectCoords::from_size(x, y, bounds.width(), bounds.height()),
                    line,
                    line.len(),
                    self.style.text_size,
                );
                y += self
                    .drawer
                    .renderer
                    .font_cache
                    .borrow()
                    .line_height(self.style.text_size);
                if y >= bounds.y1 {
                    break;
                }
            }
        } else {
            self.draw_text(
                bounds,
                text_buffer.borrow().as_str(),
                text_buffer.borrow().len(),
                self.style.text_size,
            );
        }

        // cursor
        let show_cursor = match &self.text_input_state {
            Some(state) => state.focus.eq(id),
            None => false,
        };
        if show_cursor {
            let cursorx = match self.locale_kind {
                UILocaleKind::LtrTtb => {
                    bounds.x0 + self.text_input_state.as_ref().unwrap().cursor_x
                }
                UILocaleKind::RtlTtb => {
                    bounds.x1 - self.text_input_state.as_ref().unwrap().cursor_x
                }
                _ => {
                    println!("Textarea cursor localekind not handled!");
                    bounds.x0 + self.text_input_state.as_ref().unwrap().cursor_x
                }
            };
            let cursory = bounds.y0 + self.text_input_state.as_ref().unwrap().cursor_y;
            self.drawer.draw_rect(
                &RectCoords::from_size(cursorx, cursory, 2., self.style.text_size + 4.),
                self.style.active_color,
            );
        }

        textarea.clone()
    }
    pub fn button(&mut self, label: Option<&str>) -> UIWidgetRef {
        let width = match label {
            Some(label) => {
                self.drawer
                    .get_text_size(self.style.text_size, label, label.len())
                    .0
            }
            None => 40.,
        };
        let height = 40.;
        let button = self.layout_new_widget(
            None,
            (UISize::DPixels(width), UISize::DPixels(height)),
            UIWidgetFlag::Clickable as u64,
        );
        let uibox = button.borrow();
        let bg_color = match uibox.hover() {
            false => self.params.color,
            true => V4f32 {
                r: self.params.color.r * 1.1,
                g: self.params.color.g * 1.1,
                b: self.params.color.b * 1.1,
                a: self.params.color.a,
            },
        };
        let draw_off = match uibox.click() {
            false => 0.,
            true => 1.,
        };
        self.drawer.draw_rect(
            &RectCoords {
                x0: uibox.bounds.x0 + draw_off,
                y0: uibox.bounds.y0 + draw_off,
                x1: uibox.bounds.x1 + draw_off,
                y1: uibox.bounds.y1 + draw_off,
            },
            bg_color,
        );
        if let Some(label) = label {
            self.draw_text(
                &RectCoords::from_size(
                    uibox.bounds.x0 + draw_off,
                    uibox.bounds.y0 + draw_off,
                    uibox.bounds.width(),
                    uibox.bounds.height(),
                ),
                label,
                label.len(),
                self.style.text_size,
            );
        }
        button.clone()
    }
}

//// Utility functions
fn point_in_rect(loc: &RectCoords, point: Option<Point>) -> bool {
    if let Some(point) = point {
        point.0 >= loc.x0 && point.0 <= loc.x1 && point.1 >= loc.y0 && point.1 <= loc.y1
    } else {
        false
    }
}
pub fn color_rgb(r: u8, g: u8, b: u8) -> V4f32 {
    V4f32 {
        r: r as f32 / 256.,
        g: g as f32 / 256.,
        b: b as f32 / 256.,
        a: 1.,
    }
}
