use std::{
    cell::RefCell,
    io::{BufRead, Read},
    rc::Rc,
};

use super::{Color, RelPoint, UILayout, UISize};
use crate::{draw::Drawer, render::RectCoords};
pub type UIBoxRef = Rc<RefCell<UIBox>>;

#[repr(u64)]
pub(crate) enum UIBoxFlag {
    Clickable = 1u64,
    Draggable = 2u64 + 1, // Draggable implies clickable
    Resizable = 4u64 + 1, // Resizable implies clickable

    DrawBackground = 64u64,
    DrawBorder = 128u64,
    DrawText = 256u64,
    DrawHot = 512u64,
}

#[repr(u64)]
pub(crate) enum UIBoxEvent {
    MouseOver = 1u64,
    MouseClicked = 2u64,
    MouseReleased = 4u64,
    // KeyPressed = 8u64,
}

#[derive(Clone)]
pub struct UIBoxParams {
    pub(crate) width: Option<UISize>,
    pub(crate) height: Option<UISize>,
    pub(crate) layout: Option<UILayout>,
    pub(crate) position: Option<RelPoint>,
    pub(crate) bg_col: Option<Color>,
}
impl UIBoxParams {
    pub fn new() -> Self {
        UIBoxParams {
            width: None,
            height: None,
            layout: None,
            position: None,
            bg_col: None,
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
    pub fn position(&mut self, position: RelPoint) -> &mut Self {
        self.position = Some(position);
        self
    }
    pub fn background_color(&mut self, bg_col: Color) -> &mut Self {
        self.bg_col = Some(bg_col);
        self
    }
    pub fn reset(&mut self) {
        self.width = None;
        self.height = None;
        self.layout = None;
        self.position = None;
        self.bg_col = None;
    }
}

pub struct UIBox {
    pub(crate) key: u64,
    pub(crate) bounds: RectCoords,
    pub(crate) children: Vec<UIBoxRef>,
    pub(crate) parent: Option<UIBoxRef>,
    pub(crate) previous: Option<UIBoxRef>,
    pub(crate) layout: Option<UILayout>,

    // event flags
    pub(crate) flags: u64,
    pub(crate) events: u64,

    pub(crate) string: Option<String>,

    pub(crate) style: UIBoxParams,

    #[cfg(debug_assertions)]
    pub(crate) depth: usize,
}

pub(crate) fn u64_hash_from_string(seed: u64, string: &String) -> u64 {
    // dirty implementation, I just want to generate keys atm I don't care of the quality
    let p1 = 0x2B7E151628AED2A5u64;
    let p2 = 0x9E3793492EEDC3F7u64;
    let p3 = 0x3243F6A8885A308Du64;

    let mut h = [
        std::num::Wrapping(p1),
        std::num::Wrapping(p2),
        std::num::Wrapping(p3),
        std::num::Wrapping(seed),
    ];
    let mut k = 0;
    let length = string.len() / 32;

    #[inline(always)]
    fn load_u64_le(bytes: &[u8]) -> u64 {
        u64::from_le_bytes(bytes[..8].try_into().unwrap())
    }

    let bytes = string.as_bytes();
    for _ in 0..length {
        for i in 0..4 {
            let l = load_u64_le(&bytes[k..]);
            h[i] = h[i] ^ std::num::Wrapping(l);
            h[i] = h[i] * std::num::Wrapping(p1);
            h[(i + 1) & 3] ^= (l << 40) | (l >> 24);
            k += 8;
        }
    }

    h[0] += ((string.len() << 32) | (string.len() >> 32)) as u64;
    if (string.len() & 1) == 1 {
        h[0] ^= bytes[k] as u64;
        k += 1;
    }
    h[0] *= p2;
    h[0] ^= h[0] >> 31;

    for i in 1..=8 {
        if string.len() - k < 8 {
            break;
        }
        let l = load_u64_le(&bytes[k..]);
        h[i] ^= l;
        h[i] *= p2;
        h[i] ^= h[i] >> 31;
        k += 8;
    }

    let remain = string.len() - k;
    if remain >= 4 {
        h[2] ^= u32::from_le_bytes(TryInto::<[u8; 4]>::try_into(&bytes[k..k + 4]).unwrap()) as u64;
        h[3] ^=
            u32::from_le_bytes(TryInto::<[u8; 4]>::try_into(&bytes[string.len() - 4..]).unwrap())
                as u64;
    } else if remain > 0 {
        h[2] ^= bytes[k] as u64;
        h[3] ^= bytes[remain / 2] as u64 | (bytes[remain - 1] as u64) << 8;
    }
    let mut i = 0;
    while k < string.len() {
        h[i] ^= bytes[k] as u64 | (bytes[k + 1] as u64) << 8;
        h[i] *= p3;
        h[i] ^= h[i] >> 31;
        k += 2;
        i += 1;
    }

    let mut x = std::num::Wrapping(seed);
    x ^= h[0] * (h[2] >> 32) | std::num::Wrapping(1);
    x ^= h[1] * (h[3] >> 32) | std::num::Wrapping(1);
    x ^= h[2] * (h[0] >> 32) | std::num::Wrapping(1);
    x ^= h[3] * (h[1] >> 32) | std::num::Wrapping(1);
    x.0
}

impl UIBox {
    pub fn root() -> Self {
        UIBox {
            key: u64_hash_from_string(1234, &String::from("#root")),
            bounds: RectCoords::from_size(0., 0., 0., 0.),
            parent: None,
            previous: None,
            children: Vec::new(),
            layout: Some(UILayout::Root),
            flags: 0,
            events: 0,
            string: None,
            style: UIBoxParams::new(),
            #[cfg(debug_assertions)]
            depth: 0,
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
    pub(crate) fn draggable(&self) -> bool {
        (self.flags & UIBoxFlag::Draggable as u64) == UIBoxFlag::Draggable as u64
    }
    pub(crate) fn resizable(&self) -> bool {
        (self.flags & UIBoxFlag::Resizable as u64) == UIBoxFlag::Resizable as u64
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

    // pub fn compute_size(&self, size: &UISize, drawer: &Drawer) -> f32 {
    //     match size {
    //         UISize::DPixels(val) => val,
    //         UISize::Percents(val) => val * self.parent.unwrap().borrow().bounds,
    //         UISize::TextContent => {
    //             drawer.get_text_size(self.style.text_size, self.string, self.string.len())
    //         }
    //     }
    //     100.
    // }
}
