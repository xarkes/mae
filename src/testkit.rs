use std::collections::HashMap;

use crate::{
    imui::{IMUI, Point, Size, UiKey, UiSignal},
    os::{OSEvent, OSKey, OSKeyCode},
    render::{
        self, RectCoords, RenderBatch,
        software::{self, SoftwareSurface, Texture},
    },
};

#[derive(Clone, Debug)]
pub struct UiNodeSnapshot {
    pub key: UiKey,
    pub label: Option<String>,
    pub text: Option<String>,
    pub bounds: RectCoords,
    pub computed_size: Size,
    pub scroll: Point,
    pub signal: UiSignal,
    pub visible: bool,
    pub focused: bool,
    pub mouse_clickable: bool,
    pub text_input: bool,
    pub scroll_x: bool,
    pub scroll_y: bool,
}

impl UiNodeSnapshot {
    pub fn matches(&self, id: &str) -> bool {
        self.label.as_deref() == Some(id) || self.text.as_deref() == Some(id)
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.bounds.x0 + self.bounds.width() / 2.0,
            self.bounds.y0 + self.bounds.height() / 2.0,
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct UiSnapshot {
    pub nodes: Vec<UiNodeSnapshot>,
}

impl UiSnapshot {
    pub fn node(&self, id: &str) -> &UiNodeSnapshot {
        self.try_node(id)
            .unwrap_or_else(|| panic!("UI node not found: {id}"))
    }

    pub fn try_node(&self, id: &str) -> Option<&UiNodeSnapshot> {
        self.nodes.iter().find(|node| node.matches(id))
    }
}

pub struct UiHarness {
    ui: IMUI,
    last_snapshot: UiSnapshot,
}

impl UiHarness {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            ui: IMUI::new_for_test(width, height),
            last_snapshot: UiSnapshot::default(),
        }
    }

    pub fn ui(&self) -> &IMUI {
        &self.ui
    }

    pub fn ui_mut(&mut self) -> &mut IMUI {
        &mut self.ui
    }

    pub fn frame(&mut self, build: impl FnOnce(&mut IMUI)) -> UiSnapshot {
        self.ui.begin_frame();
        build(&mut self.ui);
        self.last_snapshot = self.ui.end_test_frame();
        self.last_snapshot.clone()
    }

    pub fn push_event(&mut self, event: OSEvent) {
        self.ui.push_test_event(event);
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.push_event(OSEvent::mouse_move(Point::new(x, y)));
    }

    pub fn mouse_down(&mut self, button: OSKey, x: f32, y: f32) {
        self.push_event(OSEvent::press(button, Some(Point::new(x, y))));
    }

    pub fn mouse_up(&mut self, button: OSKey, x: f32, y: f32) {
        self.push_event(OSEvent::release(button, Some(Point::new(x, y))));
    }

    pub fn click_at(&mut self, x: f32, y: f32) {
        self.mouse_move(x, y);
        self.mouse_down(OSKey::LeftMouseButton, x, y);
        self.mouse_up(OSKey::LeftMouseButton, x, y);
    }

    pub fn click(&mut self, id: &str) {
        let center = self.last_snapshot.node(id).center();
        self.click_at(center.x(), center.y());
    }

    pub fn scroll_at(&mut self, x: f32, y: f32, delta: f32) {
        self.push_event(OSEvent::scroll(Point::new(x, y), delta));
    }

    pub fn scroll(&mut self, id: &str, delta: f32) {
        let center = self.last_snapshot.node(id).center();
        self.scroll_at(center.x(), center.y(), delta);
    }

    pub fn key_press(&mut self, key: OSKeyCode) {
        self.push_event(OSEvent::press(OSKey::Keyboard(key), None));
    }

    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.push_event(OSEvent::text(ch));
        }
    }

    pub fn snapshot(&self) -> &UiSnapshot {
        &self.last_snapshot
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderSnapshot {
    surface: SoftwareSurface,
}

impl RenderSnapshot {
    pub fn width(&self) -> usize {
        self.surface.width()
    }

    pub fn height(&self) -> usize {
        self.surface.height()
    }

    pub fn pixels(&self) -> &[u32] {
        self.surface.pixels()
    }

    pub fn pixel(&self, x: usize, y: usize) -> u32 {
        self.surface.pixels()[y * self.surface.width() + x]
    }
}

pub fn render_batches(width: usize, height: usize, batches: &[RenderBatch]) -> RenderSnapshot {
    render_batches_with_textures(width, height, batches, &HashMap::new())
}

pub fn render_batches_with_textures(
    width: usize,
    height: usize,
    batches: &[RenderBatch],
    textures: &HashMap<u32, Texture>,
) -> RenderSnapshot {
    let mut surface = SoftwareSurface::new(width, height);
    surface.clear(software::DEFAULT_CLEAR_COLOR);
    render::software::render_batches(&mut surface, batches, textures);
    RenderSnapshot { surface }
}
