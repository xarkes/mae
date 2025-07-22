mod text_input_state;
mod uibox;

use std::{cell::RefCell, collections::HashMap, rc::Rc};
use text_input_state::IMUITextInputState;
use uibox::{Color, UIBox, UIBoxEvent, UIBoxFlag, UIBoxParams, UIBoxRef, u64_hash_from_string};

#[cfg(debug_assertions)]
mod debug;
#[cfg(debug_assertions)]
use debug::{IMUIDebug, draw_debug_info};

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

use crate::{
    draw::{self, Drawer},
    os::{self, OSEvent, OSEventType, OSKey},
    render::{self, RectCoords, V4f32, font_cache::FontCache},
};

pub enum UITextAlign {
    Left,
    Center,
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}
impl Axis {
    pub fn val(&self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Point {
    x: f32,
    y: f32,
}
impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
    fn default() -> Self {
        Point { x: 0., y: 0. }
    }
    pub fn axis(&self, axis: Axis) -> &f32 {
        match axis {
            Axis::X => &self.x,
            Axis::Y => &self.y,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Size {
    width: f32,
    height: f32,
}
impl Size {
    pub fn from(value: (f32, f32)) -> Self {
        Size {
            width: value.0,
            height: value.1,
        }
    }

    pub fn axis_mut(&mut self, axis: Axis) -> &mut f32 {
        match axis {
            Axis::X => &mut self.width,
            Axis::Y => &mut self.height,
        }
    }
    pub fn axis(&self, axis: Axis) -> &f32 {
        match axis {
            Axis::X => &self.width,
            Axis::Y => &self.height,
        }
    }

    fn default() -> Size {
        Size {
            width: 0.,
            height: 0.,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum UISize {
    DPixels(f32),  // DPI scaled pixels; in current implementation all draws are dpi scaled
    Percents(f32), // Percentages of parent's size
    TextContent,   // Adapt to fit text attached to box
}
#[macro_export]
macro_rules! uisize {
    ($value:tt) => {
        if let Some(val) = $value.strip_suffix("px") {
            UISize::DPixels(val.parse::<f32>().unwrap())
        } else if let Some(val) = $value.strip_suffix("%") {
            UISize::Percents(val.parse::<f32>().unwrap() / 100.)
        } else {
            panic!("Unrecognized unit")
        }
    };
}

#[derive(Copy, Clone)]
pub enum UILayout {
    Vertical,    // Default, natural vertical layout - results depends on the localization
    VerticalLtr, // Vertical layout, forcing left to right reading
    VerticalRtl, // Vertical layout, forcing right to left reading
    Horizontal,
    HorizontalLtr,
    HorizontalRtl,
}
impl UILayout {
    pub fn specialize(&self, localekind: UILocaleKind) -> Self {
        match self {
            UILayout::Vertical => match localekind {
                UILocaleKind::LtrTtb => UILayout::VerticalLtr,
                UILocaleKind::RtlTtb => UILayout::VerticalRtl,
                _ => {
                    println!("Warning: unsupported locale kind");
                    UILayout::VerticalLtr
                }
            },
            UILayout::Horizontal => match localekind {
                UILocaleKind::LtrTtb => UILayout::HorizontalLtr,
                UILocaleKind::RtlTtb => UILayout::HorizontalRtl,
                _ => {
                    println!("Warning: unsupported locale kind");
                    UILayout::HorizontalLtr
                }
            },
            layout => *layout,
        }
    }
}

#[derive(Default)]
struct IMUIEventState {
    events: Vec<OSEvent>,
    //// input events cache
    mouse: Option<Point>,
    click: Option<Point>,
    active: Option<u64>,

    drag_pos: Option<Point>,
    drag_cache: HashMap<String, Point>,
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
    // persistent data
    drawer: Drawer,
    root: UIBoxRef,
    floating_roots: Vec<UIBoxRef>,
    locale_kind: UILocaleKind,
    #[cfg(debug_assertions)]
    debug: IMUIDebug,

    // per-build data
    size: Size,
    event: IMUIEventState,
    text_input_state: Option<IMUITextInputState>,

    // ui construction helpers
    params: UIBoxParams,
    parent_stack: Vec<UIBoxRef>,
    uiboxes: HashMap<u64, UIBoxRef>,
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

        let root = Rc::new(RefCell::new(UIBox::root()));
        IMUI {
            drawer,
            #[cfg(debug_assertions)]
            debug: IMUIDebug::default(),
            size: Size {
                width: 0.,
                height: 0.,
            },
            params: UIBoxParams::new(),
            event: IMUIEventState::default(),
            text_input_state: None,
            locale_kind: UILocaleKind::LtrTtb,
            root: root.clone(),
            floating_roots: Vec::new(),
            uiboxes: HashMap::new(),
            parent_stack: vec![root.clone()],
            style: UIStyle::default(),
        }
    }
    pub fn eventloop(&mut self, mut build_ui_func: impl FnMut(&mut IMUI)) {
        let freq = os::timer_init();
        let mut time = 0f64;
        let mut start = os::timer_value();
        loop {
            // xarkes: handle events
            {
                self.consume_events();
                self.resize();
            }

            // xarkes: clean previous state
            {
                self.root.borrow_mut().children.clear();
                self.floating_roots.clear();
            }

            // xarkes: build interface
            {
                build_ui_func(self);
                self.build_ui_end();
                self.layout_all();
            }

            #[cfg(debug_assertions)]
            {
                // xarkes: second draw and layout when debugging -> allows debug information on boxes to be accurate
                draw_debug_info(self, self.debug.clone(), time);
                self.layout_all();
            }

            // xarkes: draw interface and render
            {
                self.draw_ui_all();
                self.drawer.renderer.render_frame();
            }

            let end = os::timer_value();
            time = (end - start) as f64 * 1_000_000.0 / freq;
            start = end;
        }
    }

    fn build_ui_end(&mut self) {
        // show cursor if any
        if let Some(text_input_state) = &self.text_input_state {
            let cursorx = match self.locale_kind {
                UILocaleKind::LtrTtb => {
                    text_input_state.focus.borrow().origin.x
                        + self.text_input_state.as_ref().unwrap().cursor_x
                }
                UILocaleKind::RtlTtb => {
                    text_input_state.focus.borrow().origin.x
                        + text_input_state.focus.borrow().size.width
                        - self.text_input_state.as_ref().unwrap().cursor_x
                }
                _ => {
                    println!("Textarea cursor localekind not handled!");
                    text_input_state.focus.borrow().origin.x
                        + self.text_input_state.as_ref().unwrap().cursor_x
                }
            };
            let cursory = text_input_state.focus.borrow().origin.y
                + self.text_input_state.as_ref().unwrap().cursor_y;

            let color = self.style.active_color;
            let height = self
                .drawer
                .renderer
                .font_cache
                .borrow()
                .line_height(self.style.text_size);
            // TODO: draw cursor
            // self.params()
            //     .background_color(color)
            //     .width(uisize!("2px"))
            //     .height(UISize::DPixels(height))
            //     .position((UISize::DPixels(cursorx), UISize::DPixels(cursory)));
            // self.add_box_from_string(None, UIBoxFlag::DrawBackground as u64);
        }
    }

    fn layout_resize_standalone(&self, root: UIBoxRef, axis: Axis) {
        iter_root(root, |nodeptr| {
            let mut node = nodeptr.borrow_mut();

            let pref_size = node
                .pref_size
                .unwrap_or((UISize::TextContent, UISize::TextContent));
            let pref_size = [pref_size.0, pref_size.1];

            *node.size.axis_mut(axis) = match pref_size[axis.val()] {
                UISize::DPixels(pixels) => pixels,
                UISize::TextContent => {
                    if let Some(string) = &node.string {
                        match axis {
                            Axis::X => {
                                self.drawer
                                    .get_text_size(node.font_size, string.as_str(), string.len())
                                    .0
                            }
                            Axis::Y => self
                                .drawer
                                .renderer
                                .font_cache
                                .borrow()
                                .line_height(node.font_size),
                        }
                    } else {
                        // xarkes: no attached string, so textcontent refers to something else
                        *node.size.axis(axis)
                    }
                }
                _ => {
                    // xarkes: other units are not considered standalone
                    *node.size.axis(axis)
                }
            };
            false
        });
    }
    fn layout_resize_upward_dependents(&self, root: UIBoxRef, axis: Axis) {
        iter_root(root, |nodeptr| {
            let mut node = nodeptr.borrow_mut();

            let pref_size = node
                .pref_size
                .unwrap_or((UISize::TextContent, UISize::TextContent));
            let pref_size = [pref_size.0, pref_size.1];

            *node.size.axis_mut(axis) = match pref_size[axis.val()] {
                UISize::Percents(percents) => {
                    let parent_size = &node.parent.as_ref().unwrap().borrow().size;
                    percents * *parent_size.axis(axis)
                }
                _ => {
                    // xarkes: other units are not considered upward dependents, or already computed
                    *node.size.axis(axis)
                }
            };
            false
        });
    }
    fn apply_layout(&self, root: UIBoxRef, axis: Axis) {
        iter_root(root, |nodeptr| {
            if nodeptr.borrow().parent.is_none() {
                debug_assert!(nodeptr.borrow().key == self.root.borrow().key);
                return false;
            }
            let mut node = nodeptr.borrow_mut();
            let parent = node.parent.as_ref().unwrap();
            let layout = parent.borrow().layout;
            if layout.is_none() {
                println!("Warning: box has children but no layout");
                return true;
            }
            let layout = layout.unwrap().specialize(self.locale_kind);

            match layout {
                UILayout::VerticalLtr => {
                    let mut insert_point = match node.previous.as_ref() {
                        Some(prev) => Point::new(
                            parent.borrow().origin.x,
                            prev.borrow().origin.y + prev.borrow().size.height,
                        ),
                        None => {
                            let origin = parent.borrow().origin;
                            Point::new(
                                origin.x + parent.borrow().scrollx,
                                origin.y + parent.borrow().scrolly,
                            )
                        }
                    };
                    // insert_point.x += node.scrollx;
                    // insert_point.y += node.scrolly;
                    // insert_point.x += node.parent.as_ref().unwrap().borrow().scrollx;
                    // insert_point.y += node.parent.as_ref().unwrap().borrow().scrolly;

                    node.origin = insert_point;
                }
                _ => {
                    println!("Unsupported layout");
                }
            }

            false
        });
    }
    fn resolve_constraints(&self, root: UIBoxRef, axis: Axis) {
        iter_root(root, |nodeptr| {
            let mut node = nodeptr.borrow_mut();
            if node.parent.is_none() {
                return false;
            }
            debug_assert!(*node.size.axis(axis) >= 0.);

            let parent_size = *node.parent.as_ref().unwrap().borrow().size.axis(axis);
            let parent_origin = *node.parent.as_ref().unwrap().borrow().origin.axis(axis);
            let parent_bound = parent_origin + parent_size;

            // xarkes: handle overflow, reduce children size
            let bound = node.origin.axis(axis) + node.size.axis(axis);
            if bound > parent_bound {
                *node.size.axis_mut(axis) = f32::max(parent_bound - node.origin.axis(axis), 0.);
            }
            // xarkes: handle underflow
            if bound < parent_origin {
                *node.size.axis_mut(axis) = 0.;
            }
            false
        });
    }

    fn layout_root(&mut self, root: UIBoxRef) {
        for axis in [Axis::X, Axis::Y] {
            self.layout_resize_standalone(root.clone(), axis);
            self.layout_resize_upward_dependents(root.clone(), axis);
            self.apply_layout(root.clone(), axis);
            self.resolve_constraints(root.clone(), axis);
        }
    }

    fn layout_all(&mut self) {
        self.layout_root(self.root.clone());
        for root in &self.floating_roots.clone() {
            self.layout_root(root.clone());
        }
    }

    fn draw_ui_root(&mut self, root: UIBoxRef) {
        iter_root(root, |curnode| {
            let curnode = curnode.borrow();

            if !curnode.visible() {
                return true;
            }

            let bounds = curnode.bounds();

            // xarkes: for each box, send the proper draw commands
            if curnode.draw_background() {
                let color = self.style.bg_color;
                self.drawer.draw_rect(&bounds, color);
            }

            if curnode.draw_border() {
                let color = match curnode.hover() {
                    true => self.style.active_color,
                    false => self.style.main_color,
                };
                self.drawer.draw_empty_rect(&bounds, color, 1.0, false);
            }

            if curnode.draw_text() {
                if let Some(string) = &curnode.string {
                    // XXX: maybe check underflow in the layout algorithm and store in the uibox
                    // XXX: check that the underflow on X is working too
                    let underflow = bounds.y0
                        < curnode.parent.as_ref().unwrap().borrow().bounds().y0
                        || bounds.x0 < curnode.parent.as_ref().unwrap().borrow().bounds().x0;
                    if !underflow {
                        self.drawer.draw_text(
                            bounds.x0,
                            bounds.y0,
                            self.style.text_size,
                            string.as_str(),
                            string.len(),
                            bounds.x1,
                            bounds.y1,
                            self.style.text_color,
                            false,
                        );
                    } else {
                        self.drawer.draw_text(
                            curnode.parent.as_ref().unwrap().borrow().bounds().x0,
                            curnode.parent.as_ref().unwrap().borrow().bounds().y0,
                            self.style.text_size,
                            string.as_str(),
                            string.len(),
                            bounds.x1,
                            bounds.y1,
                            self.style.text_color,
                            true,
                        );
                    }
                }
            }

            if self.debug.hints {
                // if true {
                let col = match curnode.hover() {
                    true => color_rgb(0, 255, 0),
                    false => color_rgb(255, 0, 0),
                };
                self.drawer.draw_empty_rect(&bounds, col, 1.2, true);
                if curnode.hover() {
                    let txt = format!(
                        "({},{}) {}x{}",
                        curnode.origin.x, curnode.origin.y, curnode.size.width, curnode.size.height
                    );
                    self.drawer.draw_text(
                        curnode.origin.x + 2.,
                        curnode.origin.y + 2.,
                        10.,
                        txt.as_str(),
                        txt.len(),
                        curnode.origin.x + curnode.size.width,
                        curnode.origin.y + curnode.size.height,
                        color_rgb(255, 255, 0),
                        false,
                    );
                    self.drawer.draw_text(
                        self.size.width / 2.,
                        self.size.height / 2.,
                        10.,
                        txt.as_str(),
                        txt.len(),
                        self.size.width,
                        self.size.height,
                        color_rgb(255, 255, 0),
                        false,
                    );
                }
            }
            return false;
        });
    }

    fn draw_ui_all(&mut self) {
        self.draw_ui_root(self.root.clone());
        for root in &self.floating_roots.clone() {
            self.draw_ui_root(root.clone());
        }
    }

    /////////////////////////////////
    //// Events related functions
    pub fn consume_events(&mut self) {
        self.event.events = self.drawer.renderer.win.get_events();

        self.event.events.retain(|ev| {
            let mut retain = true;
            if ev.ty == OSEventType::Press {
                if let Some(textinput) = self.text_input_state.as_mut() {
                    retain = !textinput.handle_event(&ev.key, &ev.chars);
                }
            }
            retain
        });
        // TODO(xarkes): we may want to propagate the event back to the OS window when the application did not consume them
    }
    pub(crate) fn resize(&mut self) -> Size {
        self.size = Size::from(self.drawer.renderer.win.get_size());
        let render_size = self.drawer.renderer.win.get_render_size();
        self.drawer.renderer.resize(render_size.0, render_size.1);
        self.root.borrow_mut().size.width = self.size.width;
        self.root.borrow_mut().size.height = self.size.height;
        self.size
    }
    pub fn text_input_changecount(&self) -> Option<usize> {
        match &self.text_input_state {
            Some(state) => Some(state.changecount),
            None => None,
        }
    }

    /////////////////////////////////
    //// Widgets functions
    pub fn params(&mut self) -> &mut UIBoxParams {
        self.params.reset();
        &mut self.params
    }
    pub fn vertical(&mut self, mut children: impl FnMut(&mut IMUI)) -> UIBoxRef {
        let pane = self.add_box_from_string(None, 0);
        pane.borrow_mut().layout = Some(UILayout::Vertical);
        self.parent_stack.push(pane.clone());
        self.params.reset();
        children(self);
        self.parent_stack.pop();
        pane
    }
    // same as vertical but you can specify an id to get persistence
    pub fn frame(&mut self, id: &str, mut children: impl FnMut(&mut IMUI)) -> UIBoxRef {
        let frame = self.add_box_from_string(Some(id), UIBoxFlag::Scrollable as u64);
        frame.borrow_mut().layout = Some(UILayout::Vertical);
        self.parent_stack.push(frame.clone());
        self.params.reset();
        children(self);
        self.parent_stack.pop();
        frame
    }
    pub fn floating_pane(
        &mut self,
        pos: Point,
        size: Size,
        title: &str,
        mut children: impl FnMut(&mut IMUI),
    ) -> UIBoxRef {
        let uibox = self.new_floating_root(pos, size);

        // children
        {
            self.parent_stack.push(uibox.clone());
            self.params.reset();
            let foldable_frame_id = format!("fp_frame_{}", title);
            if self
                .button(format!("Fold##fp_fold_{}", title).as_str())
                .borrow()
                .clicked()
            {
                let (key, _) = self.get_key_from_string(Some(foldable_frame_id.as_str()));
                if let Some(uibox) = self.uiboxes.get(&key) {
                    let old_visible = uibox.borrow().visible;
                    uibox.borrow_mut().visible = !old_visible;
                }
            }
            self.label(title);
            self.params.width = Some(uisize!("100%"));
            self.params.height = Some(uisize!("100%"));
            self.frame(foldable_frame_id.as_str(), |ui| {
                children(ui);
            });
            self.parent_stack.pop();
        }
        uibox
    }

    pub fn floating_box(&mut self, mut children: impl FnMut(&mut IMUI)) -> UIBoxRef {
        let uibox = self.add_box_from_string(
            None,
            UIBoxFlag::Clickable as u64
                | UIBoxFlag::DrawBackground as u64
                | UIBoxFlag::DrawBorder as u64
                | UIBoxFlag::Draggable as u64
                | UIBoxFlag::DrawHot as u64,
        );
        uibox.borrow_mut().layout = Some(UILayout::Vertical);
        self.parent_stack.push(uibox.clone());
        self.params.reset();
        children(self);
        self.parent_stack.pop();
        uibox
    }
    pub fn line_edit(&mut self, text_buffer: Rc<RefCell<String>>, id: &str) -> UIBoxRef {
        let line_edit = self.add_box_from_string(
            Some(id),
            UIBoxFlag::Clickable as u64
                | UIBoxFlag::DrawBackground as u64
                | UIBoxFlag::DrawBorder as u64,
        );
        line_edit.borrow_mut().layout = Some(UILayout::Vertical);
        let multiline = false;
        self.text_edit_impl(line_edit, text_buffer, multiline)
    }
    pub fn textarea(&mut self, text_buffer: Rc<RefCell<String>>, id: &str) -> UIBoxRef {
        let textarea = self.add_box_from_string(
            Some(id),
            UIBoxFlag::Clickable as u64
                | UIBoxFlag::DrawBackground as u64
                | UIBoxFlag::Scrollable as u64,
        );
        textarea.borrow_mut().layout = Some(UILayout::Vertical);
        let multiline = true;
        self.text_edit_impl(textarea, text_buffer, multiline)
    }
    fn text_edit_impl(
        &mut self,
        textarea: UIBoxRef,
        text_buffer: Rc<RefCell<String>>,
        multiline: bool,
    ) -> UIBoxRef {
        if textarea.borrow().clicked() {
            // xarkes: update the text input global state
            let mut state = IMUITextInputState::new(
                // textarea.key,
                textarea.clone(),
                self.drawer.renderer.font_cache.clone(),
                text_buffer.clone(),
                multiline,
            );
            let bounds = &textarea.borrow().bounds();
            state.compute_valid_cursor_loc(
                bounds,
                &text_buffer.borrow(),
                self.style.text_size,
                self.event.mouse.unwrap(),
            );
            self.text_input_state = Some(state);
        }

        // text
        self.parent_stack.push(textarea.clone());
        self.params.reset();
        if multiline {
            for line in text_buffer.borrow().lines() {
                self.label(line);
            }
        } else {
            self.label(text_buffer.borrow().as_str());
        }
        self.parent_stack.pop();

        textarea.clone()
    }

    fn get_key_from_string(&self, label: Option<&str>) -> (u64, Option<String>) {
        match label {
            None => (0u64, None),
            Some(label) => {
                if let Some(idx) = label.find("###") {
                    (
                        u64_hash_from_string(self.root.borrow().key, &label[idx..]),
                        Some(String::from(&label[..idx])),
                    )
                } else if let Some(idx) = label.find("##") {
                    (
                        u64_hash_from_string(self.root.borrow().key, label),
                        Some(String::from(&label[..idx])),
                    )
                } else {
                    (
                        u64_hash_from_string(self.root.borrow().key, label),
                        Some(String::from(label)),
                    )
                }
            }
        }
    }

    fn new_floating_root(&mut self, position: Point, size: Size) -> UIBoxRef {
        let mut root = UIBox::root();
        root.origin = position;
        root.pref_size = Some((UISize::DPixels(size.width), UISize::DPixels(size.height)));
        root.size = size;
        root.flags = UIBoxFlag::DrawBackground as u64;
        let root_ref = Rc::new(RefCell::new(root));
        self.floating_roots.push(root_ref.clone());
        root_ref
    }

    fn add_box_from_string(&mut self, label: Option<&str>, flags: u64) -> UIBoxRef {
        let parent = self.parent_stack.last().unwrap();
        let (key, string) = self.get_key_from_string(label);
        let pref_size;
        if self.params.width.is_some() || self.params.height.is_some() {
            pref_size = Some((
                self.params.width.unwrap_or(UISize::TextContent),
                self.params.height.unwrap_or(UISize::TextContent),
            ));
        } else {
            pref_size = None;
        }
        let uibox = match self.uiboxes.get(&key) {
            Some(uibox) => {
                // xarkes: clear previous frame event info
                {
                    let mut uibox = uibox.borrow_mut();
                    uibox.events = 0;
                    uibox.children.clear();
                    uibox.parent = Some(parent.clone());
                }
                uibox.clone()
            }
            None => {
                let uibox = Rc::new(RefCell::new(UIBox {
                    key,
                    pref_size,
                    size: Size::default(),
                    origin: Point::default(),
                    children: Vec::new(),
                    #[cfg(debug_assertions)]
                    depth: parent.borrow().depth + 1,
                    parent: Some(parent.clone()),
                    previous: None,
                    layout: None,
                    visible: true,

                    flags,
                    events: 0,

                    string,

                    scrollx: 0.,
                    scrolly: 0.,
                    font_size: self.style.text_size,
                }));
                if key != 0 {
                    self.uiboxes.insert(key, uibox.clone());
                }
                uibox
            }
        };

        uibox.borrow_mut().previous = parent.borrow().children.last().cloned();
        parent.borrow_mut().children.push(uibox.clone());
        self.handle_uibox_event(uibox.clone());
        uibox
    }

    pub fn label(&mut self, label: &str) -> UIBoxRef {
        let uibox = self.add_box_from_string(None, UIBoxFlag::DrawText as u64);
        uibox.borrow_mut().string = Some(String::from(label));
        uibox
    }

    fn handle_uibox_event(&mut self, uibox: UIBoxRef) {
        for ev in &self.event.events {
            let in_bounds = point_in_rect(&uibox.borrow().bounds(), ev.pos);
            let clickable = uibox.borrow().clickable();
            let is_active = self
                .event
                .active
                .as_ref()
                .is_some_and(|key| *key == uibox.borrow().key);

            if ev.ty == OSEventType::Press
                && ev.key == OSKey::LeftMouseButton
                && clickable
                && in_bounds
            {
                self.event.click = ev.pos;
                self.event.active = Some(uibox.borrow().key);
                self.event.mouse = ev.pos;
                uibox.borrow_mut().events |= UIBoxEvent::MouseClicked as u64;
            } else if ev.ty == OSEventType::Release
                && ev.key == OSKey::LeftMouseButton
                && clickable
                && in_bounds
                && is_active
            {
                self.event.active = None;
                self.event.click = None;
                uibox.borrow_mut().events |= UIBoxEvent::MouseReleased as u64;
                self.text_input_state = None;
            }

            if ev.ty == OSEventType::MouseMove {
                self.event.mouse = ev.pos;
            }

            if uibox.borrow().scrollable_y() && ev.ty == OSEventType::Scroll {
                uibox.borrow_mut().scrolly += ev.delta;
            }
        }

        if point_in_rect(&uibox.borrow().bounds(), self.event.mouse) {
            uibox.borrow_mut().events |= UIBoxEvent::MouseOver as u64;
        }
        if point_in_rect(&uibox.borrow().bounds(), self.event.mouse) {
            uibox.borrow_mut().events |= UIBoxEvent::MouseClicked as u64;
        }
    }

    pub fn button(&mut self, label: &str) -> UIBoxRef {
        let uibox = self.add_box_from_string(
            Some(label),
            UIBoxFlag::Clickable as u64
                | UIBoxFlag::DrawBackground as u64
                | UIBoxFlag::DrawBorder as u64
                | UIBoxFlag::DrawText as u64
                | UIBoxFlag::DrawHot as u64,
        );
        uibox
    }
}

//// Utility functions
fn point_in_rect(loc: &RectCoords, point: Option<Point>) -> bool {
    if let Some(point) = point {
        point.x >= loc.x0 && point.x <= loc.x1 && point.y >= loc.y0 && point.y <= loc.y1
    } else {
        false
    }
}
pub fn color_rgb(r: u8, g: u8, b: u8) -> V4f32 {
    V4f32 {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        a: 1.,
    }
}

fn iter_root(start_node: UIBoxRef, mut handle_node: impl FnMut(UIBoxRef) -> bool) {
    // xarkes: iterate created boxes from root (lowest), breadth first search (BFS)
    let mut worklist = vec![start_node];
    // let mut start = start_node.borrow().children.clone();
    // start.reverse();
    // for c in start {
    //     worklist.push(c.clone());
    // }
    loop {
        let curnode_r = match worklist.pop() {
            Some(n) => n,
            None => {
                break;
            }
        };

        let skip_children = handle_node(curnode_r.clone());
        if !skip_children {
            for c in &curnode_r.borrow().children {
                worklist.insert(0, c.clone());
            }
        }
    }
}
