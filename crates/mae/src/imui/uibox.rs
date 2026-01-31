use std::{cell::RefCell, rc::Rc};

use crate::render::{RectCoords, V4f32};

use super::{CrossAxisAlign, MainAxisAlign, Point, Size, UILayout, UISize, color_rgb};
pub type UIBoxRef = Rc<RefCell<UIBox>>;

#[derive(Clone, Copy, Debug, Default)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Padding { top, right, bottom, left }
    }

    pub fn all(value: f32) -> Self {
        Padding { top: value, right: value, bottom: value, left: value }
    }

    pub fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Padding { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

pub struct UIBoxRef2 {
    _box: Rc<RefCell<UIBox>>,
}

impl UIBoxRef2 {
    // Common APIs
    pub fn new(_box: Rc<RefCell<UIBox>>) -> Self {
        UIBoxRef2 { _box }
    }
    pub fn get(&self) -> Rc<RefCell<UIBox>> {
        self._box.clone()
    }

    // Styling APIs
    pub fn width(&self, width: UISize) -> &Self {
        self._box.borrow_mut().width = width;
        self
    }
    pub fn height(&self, height: UISize) -> &Self {
        self._box.borrow_mut().height = height;
        self
    }
    pub fn background(&self, color: Color) -> &Self {
        self._box.borrow_mut().flags |= UIBoxFlag::DrawBackground as u64;
        self._box.borrow_mut().style.bg_color = color;
        self
    }
    pub fn text_color(&self, color: Color) -> &Self {
        self._box.borrow_mut().style.text_color = color;
        self
    }

    // Layout APIs
    pub fn padding(&self, p: Padding) -> &Self {
        self._box.borrow_mut().padding = p;
        self
    }
    pub fn padding_all(&self, v: f32) -> &Self {
        self._box.borrow_mut().padding = Padding::all(v);
        self
    }
    pub fn gap(&self, g: f32) -> &Self {
        self._box.borrow_mut().child_gap = g;
        self
    }
    pub fn align_main(&self, a: MainAxisAlign) -> &Self {
        self._box.borrow_mut().main_axis_align = a;
        self
    }
    pub fn align_cross(&self, a: CrossAxisAlign) -> &Self {
        self._box.borrow_mut().cross_axis_align = a;
        self
    }
    pub fn align(&self, main: MainAxisAlign, cross: CrossAxisAlign) -> &Self {
        {
            let mut b = self._box.borrow_mut();
            b.main_axis_align = main;
            b.cross_axis_align = cross;
        }
        self
    }
}

pub type Color = V4f32;
impl Color {
    pub fn transparent() -> Self {
        Color {
            r: 0.,
            g: 0.,
            b: 0.,
            a: 0.,
        }
    }
    pub fn new(text: &str) -> Self {
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
        } else if (text.len() == 7 || text.len() == 9) && text.as_bytes()[0] == b'#' {
            let bytes = text.as_bytes();
            let mut vals: [f32; 4] = [0., 0., 0., 1.];
            for i in 0..4 {
                if i == 3 && text.len() == 7 {
                    break;
                }
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
                a: vals[3],
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
pub enum UIBoxFlag {
    // event related
    Clickable = 1u64,
    ScrollableX = 8u64,
    ScrollableY = 16u64,

    // drawing related
    DrawBackground = 64u64,
    DrawBorder = 128u64,
    DrawText = 256u64,
    DrawHot = 512u64,

    Scrollable = UIBoxFlag::ScrollableX as u64 | UIBoxFlag::ScrollableY as u64,
}

#[repr(u64)]
pub(crate) enum UIBoxEvent {
    MouseOver = 1u64,
    MouseClicked = 2u64,
    MouseReleased = 4u64,
}

#[derive(Clone, Copy, Debug)]
pub struct UIBoxParams {
    pub(crate) width: Option<UISize>,
    pub(crate) height: Option<UISize>,
    pub(crate) layout: Option<UILayout>,
    pub(crate) bg_color: Option<Color>,
}
impl UIBoxParams {
    pub fn new() -> Self {
        UIBoxParams {
            width: None,
            height: None,
            layout: None,
            bg_color: None,
        }
    }
    pub fn width(&mut self, width: UISize) -> &mut Self {
        self.width = Some(width);
        self
    }
    pub fn height(&mut self, height: UISize) -> &mut Self {
        self.height = Some(height);
        self
    }
    pub fn layout(&mut self, layout: UILayout) -> &mut Self {
        self.layout = Some(layout);
        self
    }
    pub fn bg_color(&mut self, bg_color: Color) -> &mut Self {
        self.bg_color = Some(bg_color);
        self
    }
    pub fn reset(&mut self) {
        self.width = None;
        self.height = None;
        self.layout = None;
        self.bg_color = None;
    }
}

pub(crate) fn u64_hash_from_string(seed: u64, string: &str) -> u64 {
    // xarkes: DJB2 hash with a twist (seed)
    let mut hash: u64 = 5381 + seed;
    for byte in string.bytes() {
        hash = (hash << 5).wrapping_add(hash).wrapping_add(byte as u64);
    }
    hash
}

#[derive(Debug)]
pub struct UIBoxStyle {
    pub(crate) margin: f32,
    pub(crate) border_size: f32,
    pub(crate) font_size: f32,
    pub(crate) bg_color: Color,
    pub(crate) font_icon: bool,
    pub(crate) text_color: Color,
}
impl UIBoxStyle {
    pub fn default() -> Self {
        UIBoxStyle {
            margin: 2.,
            border_size: 2.,
            font_size: 40.,
            bg_color: color_rgb(255, 0, 255),
            font_icon: false,
            text_color: color_rgb(0, 0, 0),
        }
    }
}

pub struct UIBox {
    // persistent data
    pub(crate) key: u64,

    // per-build links
    pub(crate) children: Vec<UIBoxRef>,
    pub(crate) parent: Option<UIBoxRef>,
    pub(crate) previous: Option<UIBoxRef>,

    // per-build data
    pub(crate) fixed_origin: Point,
    pub(crate) origin: Point,
    pub(crate) width: UISize,            // renamed from pref_width
    pub(crate) height: UISize,           // renamed from pref_height
    pub(crate) computed_size: Size,      // renamed from size
    pub(crate) flags: u64,
    pub(crate) events: u64,
    pub(crate) string: Option<String>,
    pub(crate) visible: bool,
    pub(crate) layout: Option<UILayout>,

    // layout configuration
    pub(crate) padding: Padding,             // NEW
    pub(crate) child_gap: f32,               // NEW
    pub(crate) main_axis_align: MainAxisAlign,   // NEW
    pub(crate) cross_axis_align: CrossAxisAlign, // NEW

    // per-build styling
    pub(crate) style: UIBoxStyle,

    // persistent data
    pub(crate) scrollx: f32,
    pub(crate) scrolly: f32,
}

impl UIBox {
    pub fn root(id: String) -> Self {
        UIBox {
            key: u64_hash_from_string(1234, id.as_str()),
            width: UISize::Fit,
            height: UISize::Fit,
            origin: Point::default(),
            fixed_origin: Point::default(), // TODO: rename as drag_position
            computed_size: Size::default(),
            parent: None,
            previous: None,
            children: Vec::new(),
            layout: Some(UILayout::Vertical),
            visible: true,
            flags: 0,
            events: 0,
            string: None,
            padding: Padding::default(),
            child_gap: 0.,
            main_axis_align: MainAxisAlign::default(),
            cross_axis_align: CrossAxisAlign::default(),
            scrollx: 0.,
            scrolly: 0.,

            style: UIBoxStyle::default(),
        }
    }
    pub fn hover(&self) -> bool {
        (self.events & UIBoxEvent::MouseOver as u64) > 0
    }
    pub fn click(&self) -> bool {
        (self.events & UIBoxEvent::MouseClicked as u64) > 0
    }
    pub fn clicked(&self) -> bool {
        (self.events & UIBoxEvent::MouseReleased as u64) > 0
    }

    pub(crate) fn clickable(&self) -> bool {
        (self.flags & UIBoxFlag::Clickable as u64) == UIBoxFlag::Clickable as u64
    }
    pub(crate) fn draw_background(&self) -> bool {
        (self.flags & UIBoxFlag::DrawBackground as u64) == UIBoxFlag::DrawBackground as u64
    }
    pub(crate) fn draw_border(&self) -> bool {
        (self.flags & UIBoxFlag::DrawBorder as u64) == UIBoxFlag::DrawBorder as u64
    }
    pub(crate) fn draw_text(&self) -> bool {
        (self.flags & UIBoxFlag::DrawText as u64) == UIBoxFlag::DrawText as u64
    }
    pub(crate) fn draw_hot(&self) -> bool {
        (self.flags & UIBoxFlag::DrawHot as u64) == UIBoxFlag::DrawHot as u64
    }
    pub(crate) fn scrollable_x(&self) -> bool {
        (self.flags & UIBoxFlag::ScrollableX as u64) == UIBoxFlag::ScrollableX as u64
    }
    pub(crate) fn scrollable_y(&self) -> bool {
        (self.flags & UIBoxFlag::ScrollableY as u64) == UIBoxFlag::ScrollableY as u64
    }

    /// Returns false if size is 0
    pub fn visible(&self) -> bool {
        self.visible && self.computed_size.width > 0. && self.computed_size.height > 0.
    }

    pub fn bounds(&self) -> RectCoords {
        RectCoords {
            x0: self.origin.x,
            y0: self.origin.y,
            x1: self.origin.x + self.computed_size.width,
            y1: self.origin.y + self.computed_size.height,
        }
    }
}
