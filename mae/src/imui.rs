mod text_input_state;
pub mod uibox;
mod widgets;

use std::{cell::RefCell, collections::HashMap, rc::Rc};
use text_input_state::IMUITextInputState;
use uibox::{
    Color, UIBox, UIBoxEvent, UIBoxFlag, UIBoxParams, UIBoxRef, UIBoxStyle, u64_hash_from_string,
};

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
pub enum Axis {
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

#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Copy, Debug)]
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
    Children,      // Compute the sum of all children
    ChildrenMax,   // Get the biggest child
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

#[derive(Copy, Clone, Debug)]
pub enum UILayout {
    Vertical,    // Default, natural vertical layout - results depends on the localization
    VerticalLtr, // Vertical layout, forcing left to right reading
    VerticalRtl, // Vertical layout, forcing right to left reading
    Horizontal,
    HorizontalLtr,
    HorizontalRtl,
    Absolute, // Specify a node be positionned at specific location
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
    rmouse: Option<Point>,
    rclick: Option<Point>,

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
    margin: f32,
    border_size: f32,
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
            margin: 0.,
            border_size: 2.,
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

        let root = Rc::new(RefCell::new(UIBox::root(String::from("#root"))));
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

    fn layout_resize_standalone(&self, root: UIBoxRef, axis: Axis) {
        iter_root(root, |nodeptr| {
            let mut node = nodeptr.borrow_mut();

            let pref_size = node.pref_size;
            let pref_size = [pref_size.0, pref_size.1];

            match pref_size[axis.val()] {
                UISize::DPixels(pixels) => {
                    *node.size.axis_mut(axis) = pixels;
                }
                UISize::TextContent => {
                    if let Some(string) = &node.string {
                        *node.size.axis_mut(axis) = match axis {
                            Axis::X => {
                                self.drawer
                                    .get_text_size(
                                        node.style.font_size,
                                        string.as_str(),
                                        string.len(),
                                    )
                                    .0
                            }
                            Axis::Y => self
                                .drawer
                                .renderer
                                .font_cache
                                .borrow()
                                .line_height(node.style.font_size),
                        }
                    } else {
                        // xarkes: no attached string
                        // XXX: does it mean that textcontent is an error?
                    }
                }
                _ => {
                    // xarkes: other units are not considered standalone
                }
            };
            false
        });
    }
    fn layout_resize_upward_dependents(&self, root: UIBoxRef, axis: Axis) {
        iter_root(root, |nodeptr| {
            let mut node = nodeptr.borrow_mut();

            let pref_size = node.pref_size;
            let pref_size = [pref_size.0, pref_size.1];

            let mut size = match pref_size[axis.val()] {
                UISize::Percents(percents) => {
                    let parent_size = &node.parent.as_ref().unwrap().borrow().size;
                    percents * *parent_size.axis(axis)
                }
                _ => {
                    // xarkes: other units are not considered upward dependents, or already computed
                    *node.size.axis(axis)
                }
            };
            *node.size.axis_mut(axis) = size;
            false
        });
    }
    fn layout_resize_downward_dependents(&self, root: UIBoxRef, axis: Axis) {
        iter_root(root, |nodeptr| {
            let mut node = nodeptr.borrow_mut();

            let pref_size = node.pref_size;
            let pref_size = [pref_size.0, pref_size.1];

            match pref_size[axis.val()] {
                UISize::Children => {
                    let mut size = 0.;
                    for child in &node.children {
                        size += *child.borrow().size.axis(axis);
                    }
                    *node.size.axis_mut(axis) = size;
                }
                UISize::ChildrenMax => {
                    let mut size_max = 0.;
                    for child in &node.children {
                        size_max = f32::max(size_max, *child.borrow().size.axis(axis));
                    }
                    *node.size.axis_mut(axis) = size_max;
                }
                _ => {
                    // xarkes: other units are not considered downward dependents, or already computed
                }
            };
            false
        });
    }
    fn apply_layout(&self, root: UIBoxRef) {
        iter_root(root, |nodeptr| {
            if nodeptr.borrow().parent.is_none() {
                return false;
            }
            let mut node = nodeptr.borrow_mut();

            // First special case: node has absolute position used for e.g. cursor
            // XXX: This is where we need a position attribute rather than using layout to do this...
            if let Some(layout) = node.layout {
                match layout {
                    UILayout::Absolute => {
                        node.origin = node.fixed_origin;
                        return false;
                    }
                    _ => {}
                }
            }

            let parent = node.parent.as_ref().unwrap();
            let layout = parent.borrow().layout;
            if layout.is_none() {
                println!("Warning: box has children but no layout");
                // // xarkes: no layout means we will just position things relatively to parent
                // let bounds = parent.borrow().bounds();
                // node.origin = Point::new(bounds.x0, bounds.y0);
                return false;
            }
            let layout = layout.unwrap().specialize(self.locale_kind);

            match layout {
                UILayout::VerticalLtr => {
                    let insert_point = match node.previous.as_ref() {
                        Some(prev) => Point::new(
                            prev.borrow().origin.x,
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
                    node.origin = insert_point;
                }
                UILayout::HorizontalLtr => {
                    let insert_point = match node.previous.as_ref() {
                        Some(prev) => Point::new(
                            prev.borrow().origin.x + prev.borrow().size.width,
                            prev.borrow().origin.y,
                        ),
                        None => {
                            let origin = parent.borrow().origin;
                            Point::new(
                                origin.x + parent.borrow().scrollx,
                                origin.y + parent.borrow().scrolly,
                            )
                        }
                    };
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
            // xarkes: make sure the previous positioning did not generate wrong data
            debug_assert!(*node.size.axis(axis) >= 0.);

            let parent_size = *node.parent.as_ref().unwrap().borrow().size.axis(axis);
            let parent_origin = *node.parent.as_ref().unwrap().borrow().origin.axis(axis);
            let parent_bound = parent_origin + parent_size;

            // xarkes: handle overflow, reduce children size
            // XXX: maybe you don't want to handle overflow at all and just let
            // the renderer clip it?
            // if !node.parent.as_ref().unwrap().borrow().scrollable_x() {
            //     let bound = node.origin.axis(axis) + node.size.axis(axis);
            //     if bound > parent_bound {
            //         *node.size.axis_mut(axis) = f32::max(parent_bound - node.origin.axis(axis), 0.);
            //     }
            //     // xarkes: handle underflow
            //     if bound < parent_origin {
            //         *node.size.axis_mut(axis) = 0.;
            //     }
            // }
            false
        });
    }

    fn layout_root(&mut self, root: UIBoxRef) {
        for axis in [Axis::X, Axis::Y] {
            self.layout_resize_standalone(root.clone(), axis);
            self.layout_resize_upward_dependents(root.clone(), axis);
            // XXX: we should iterate from the deepest node up to the root, but instead I compute it twice :>
            self.layout_resize_downward_dependents(root.clone(), axis);
            self.layout_resize_downward_dependents(root.clone(), axis);
        }
        self.apply_layout(root.clone());
        for axis in [Axis::X, Axis::Y] {
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
            let mut margin = 0.;
            if curnode.draw_background() {
                margin = curnode.style.margin;
                let color = match curnode.clickable() && curnode.draw_hot() && curnode.hover() {
                    true => {
                        let col = curnode.style.bg_color;
                        Color {
                            r: col.r + 50. / 256.,
                            g: col.g + 50. / 256.,
                            b: col.b + 50. / 256.,
                            a: col.a,
                        }
                    }
                    false => curnode.style.bg_color,
                };
                self.drawer.draw_rect(&bounds, color);
            }

            if curnode.draw_border() {
                let color = curnode.style.bg_color;
                self.drawer
                    .draw_empty_rect(&bounds, color, curnode.style.border_size, false);
            }

            if curnode.draw_text() {
                if let Some(string) = &curnode.string {
                    // TODO(xarkes): Clipping should be applied here
                    self.drawer.draw_text(
                        bounds.x0 + margin,
                        bounds.y0 + margin,
                        self.style.text_size,
                        string.as_str(),
                        string.len(),
                        bounds.x1 - margin,
                        bounds.y1 - margin,
                        self.style.text_color,
                        false,
                    );
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
        self.root.borrow_mut().pref_size = (
            UISize::DPixels(self.size.width),
            UISize::DPixels(self.size.height),
        );
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
    pub fn container(
        &mut self,
        key: Option<&str>,
        layout: UILayout,
        flags: u64,
        params: Option<UIBoxParams>,
        mut children: impl FnMut(&mut IMUI),
    ) -> UIBoxRef {
        let node = self.parent_stack.last().unwrap().clone();
        let first_frame = self
            .uiboxes
            .get(&self.get_key_from_string(key, node).0)
            .is_none();
        let container = self.add_box_from_string(key, flags);

        if (key.is_some() && first_frame) || key.is_none() {
            container.borrow_mut().pref_size = match layout.specialize(self.locale_kind) {
                UILayout::VerticalLtr => (UISize::Percents(1.), UISize::Children),
                UILayout::HorizontalLtr => (UISize::Percents(1.), UISize::ChildrenMax),
                _ => {
                    println!("Unsupported layout");
                    (UISize::Percents(1.), UISize::Children)
                }
            };

            if let Some(params) = params {
                if let Some(width) = params.width {
                    let old_height = container.borrow().pref_size.1;
                    container.borrow_mut().set_pref_size((width, old_height));
                }
                if let Some(height) = params.height {
                    let old_width = container.borrow().pref_size.0;
                    container.borrow_mut().set_pref_size((old_width, height));
                }
            }
        }

        container.borrow_mut().layout = Some(layout);
        container.borrow_mut().style.bg_color = self.style.main_color;
        self.parent_stack.push(container.clone());
        children(self);
        self.parent_stack.pop();
        container
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
    pub fn textarea(
        &mut self,
        text_buffer: Rc<RefCell<String>>,
        id: &str,
        params: Option<UIBoxParams>,
    ) -> UIBoxRef {
        // xarkes: compute scrollbars
        let max_size_x = {
            let buf = text_buffer.borrow();
            let lines = buf.lines();
            let mut count = 0;
            let mut width = 0.;
            for l in lines {
                let w = self.drawer.get_text_size(12., l, l.len()).0;
                width = f32::max(w, width);
                count += 1;
            }
            width as f32
        };
        let max_size_y = text_buffer.borrow().lines().count() as f32
            * self.drawer.renderer.font_cache.borrow().line_height(12.);

        let container = self.container(None, UILayout::Vertical, 0, params, |ui| {
            let mut textarea = None;
            ui.container(None, UILayout::Horizontal, 0, params, |ui| {
                let mut textarea_ = ui.add_box_from_string(
                    Some(id),
                    UIBoxFlag::Clickable as u64
                        | UIBoxFlag::DrawBackground as u64
                        | UIBoxFlag::Scrollable as u64,
                );
                textarea_.borrow_mut().layout = Some(UILayout::Vertical);
                textarea_
                    .borrow_mut()
                    .set_pref_size((uisize!("90%"), uisize!("90%")));
                let multiline = true;

                // xarkes: fixup scrolling, event handler does not know the child's logic
                {
                    let mut textarea = textarea_.borrow_mut();
                    let can_scroll_y = max_size_y > textarea.size.height;
                    let can_scroll_x = max_size_x > textarea.size.width;

                    if can_scroll_y {
                        let max_scroll_y = textarea.size.height - max_size_y;
                        if textarea.scrolly > 0. {
                            textarea.scrolly = 0.;
                        } else if textarea.scrolly < max_scroll_y {
                            textarea.scrolly = max_scroll_y;
                        }
                    } else {
                        textarea.scrolly = 0.;
                    }
                    if can_scroll_x {
                        let max_scroll_x = textarea.size.width - max_size_x;
                        if textarea.scrollx > 0. {
                            textarea.scrollx = 0.;
                        } else if textarea.scrollx < max_scroll_x {
                            textarea.scrollx = max_scroll_x;
                        }
                    } else {
                        textarea.scrollx = 0.;
                    }
                }

                ui.text_edit_impl(textarea_.clone(), text_buffer.clone(), multiline);

                if max_size_y > textarea_.borrow().size.height {
                    ui.scrollbar(textarea_.clone(), max_size_y, Axis::Y);
                }
                textarea = Some(textarea_.clone());
            });

            let textarea_ref = textarea.unwrap();
            if max_size_x > textarea_ref.borrow().size.width {
                ui.scrollbar(textarea_ref.clone(), max_size_x, Axis::X);
            }
        });

        container
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
                Point::new(textarea.borrow().scrollx, textarea.borrow().scrolly),
            );
            self.text_input_state = Some(state);
        }

        self.parent_stack.push(textarea.clone());
        {
            // text
            if multiline {
                // xarkes: only display visible lines
                let line_height = self
                    .drawer
                    .renderer
                    .font_cache
                    .borrow()
                    .line_height(textarea.borrow().style.font_size);
                let buffer = text_buffer.borrow();
                let lines = buffer.lines();
                let line_idx_start = (-1. * textarea.borrow().scrolly / line_height) as usize;
                let line_idx_end =
                    line_idx_start + (textarea.borrow().size.height / line_height) as usize;
                for (i, line) in lines.enumerate() {
                    if i < line_idx_start {
                        // XXX: I still add a label here because of the way the position is computed
                        // still better than having a full label
                        self.label("");
                        continue;
                    }
                    if i > line_idx_end {
                        break;
                    }
                    self.label(line);
                }
            } else {
                self.label(text_buffer.borrow().as_str());
            }
            // cursor
            if let Some(tis) = &self.text_input_state {
                if tis.focus.borrow().key == textarea.borrow().key {
                    let cx = tis.cursor_x;
                    let cy = tis.cursor_y;
                    let cursor_box =
                        self.add_box_from_string(None, UIBoxFlag::DrawBackground as u64);
                    cursor_box.borrow_mut().set_pref_size((
                        UISize::DPixels(textarea.borrow().style.font_size / 6.),
                        UISize::DPixels(textarea.borrow().style.font_size),
                    ));
                    let bounds = textarea.borrow().bounds();
                    cursor_box.borrow_mut().layout = Some(UILayout::Absolute);
                    cursor_box.borrow_mut().fixed_origin = Point::new(
                        bounds.x0 + cx + textarea.borrow().scrollx,
                        bounds.y0 + cy + textarea.borrow().scrolly,
                    );
                    cursor_box.borrow_mut().style.bg_color = self.style.active_color;
                }
            }
        }
        self.parent_stack.pop();

        textarea.clone()
    }

    fn get_key_from_string(&self, label: Option<&str>, node: UIBoxRef) -> (u64, Option<String>) {
        // xarkes: generate per-root keys
        let mut parent = node;
        loop {
            let maybe_parent = parent.borrow().parent.clone();
            parent = match maybe_parent {
                Some(parent) => parent.clone(),
                None => {
                    break;
                }
            };
        }
        let seed = parent.borrow().key;

        // xarkes: parse key string and extract hash and printable string
        match label {
            None => (0u64, None),
            Some(label) => {
                if let Some(idx) = label.find("###") {
                    (
                        u64_hash_from_string(seed, &label[idx..]),
                        Some(String::from(&label[..idx])),
                    )
                } else if let Some(idx) = label.find("##") {
                    (
                        u64_hash_from_string(seed, label),
                        Some(String::from(&label[..idx])),
                    )
                } else {
                    (u64_hash_from_string(seed, label), Some(String::from(label)))
                }
            }
        }
    }

    fn new_floating_root(&mut self, key: u64, position: Point) -> UIBoxRef {
        let root =
            self.get_or_create_box_from_key(key, None, UIBoxFlag::DrawBackground as u64, true);
        root.borrow_mut().layout = Some(UILayout::Vertical);
        root.borrow_mut().fixed_origin = position;
        root.borrow_mut().origin = position;
        self.floating_roots.push(root.clone());
        root
    }

    fn get_or_create_box_from_key(
        &mut self,
        key: u64,
        string: Option<String>,
        flags: u64,
        root: bool,
    ) -> UIBoxRef {
        let pref_size_default = (UISize::TextContent, UISize::TextContent);
        let uibox = match self.uiboxes.get(&key) {
            Some(uibox) => {
                // xarkes: clear previous frame event info
                {
                    let mut uibox = uibox.borrow_mut();
                    uibox.events = 0;
                    uibox.children.clear();
                    if !root {
                        uibox.parent = Some(self.parent_stack.last().unwrap().clone());
                    }
                }
                uibox.clone()
            }
            None => {
                let uibox = Rc::new(RefCell::new(UIBox {
                    key,
                    pref_size: pref_size_default,
                    size: Size::default(),
                    origin: Point::default(),
                    fixed_origin: Point::default(),
                    resize_delta: Point::default(),
                    children: Vec::new(),
                    #[cfg(debug_assertions)]
                    depth: 0,
                    parent: None,
                    previous: None,
                    layout: None,
                    visible: true,

                    flags,
                    events: 0,

                    string,

                    scrollx: 0.,
                    scrolly: 0.,
                    style: UIBoxStyle {
                        margin: self.style.margin,
                        font_size: self.style.text_size,
                        border_size: self.style.border_size,
                        bg_color: self.style.bg_color,
                    },
                }));
                if key != 0 {
                    self.uiboxes.insert(key, uibox.clone());
                }
                uibox
            }
        };
        if !root {
            uibox.borrow_mut().parent = Some(self.parent_stack.last().unwrap().clone());
        }

        uibox
    }

    fn add_box_from_string(&mut self, label: Option<&str>, flags: u64) -> UIBoxRef {
        let parent = self.parent_stack.last().unwrap().clone();
        let (key, string) = self.get_key_from_string(label, parent.clone());

        let uibox = self.get_or_create_box_from_key(key, string, flags, false);

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

            // LMB click
            if ev.ty == OSEventType::Press
                && ev.key == OSKey::LeftMouseButton
                && clickable
                && in_bounds
            {
                self.event.click = ev.pos;
                self.event.active = Some(uibox.borrow().key);
                self.event.mouse = ev.pos;
                #[cfg(debug_assertions)]
                {
                    self.debug.target = Some(uibox.clone());
                }
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

            // RMB click
            if ev.ty == OSEventType::Press
                && ev.key == OSKey::RightMouseButton
                && clickable
                && in_bounds
            {
                self.event.rclick = ev.pos;
                self.event.rmouse = ev.pos;
                uibox.borrow_mut().events |= UIBoxEvent::MouseClicked as u64;
            } else if ev.ty == OSEventType::Release
                && ev.key == OSKey::RightMouseButton
                && clickable
                && in_bounds
            {
                self.event.rclick = None;
                self.event.rmouse = None;
                uibox.borrow_mut().events |= UIBoxEvent::MouseReleased as u64;
                self.text_input_state = None;
            }

            if ev.ty == OSEventType::MouseMove {
                // XXX: we have to differentiate
                self.event.mouse = ev.pos;
                self.event.rmouse = ev.pos;
            }

            // xarkes: scroll behavior
            if uibox.borrow().scrollable_y()
                && ev.ty == OSEventType::Scroll
                && in_bounds
                && ev.key == OSKey::RightMouseButton
            {
                let mut uibox_mut = uibox.borrow_mut();
                uibox_mut.scrolly += ev.delta * 5.;
            }
            if uibox.borrow().scrollable_x()
                && ev.ty == OSEventType::Scroll
                && in_bounds
                && ev.key == OSKey::LeftMouseButton
            {
                let mut uibox_mut = uibox.borrow_mut();
                uibox_mut.scrollx += ev.delta * 5.;
            }
        }

        // // XXX: dirty - the whole event handling is dirty... god help me
        // if uibox.borrow().resizable() && self.event.rmouse.is_some() {
        //     if self.event.rclick.is_some() {
        //         let delta_x = self.event.rmouse.unwrap().x - self.event.rclick.unwrap().x;
        //         let delta_y = self.event.rmouse.unwrap().y - self.event.rclick.unwrap().y;
        //         uibox.borrow_mut().resize_delta = Point::new(delta_x, delta_y);
        //     } else if uibox.borrow().clicked() {
        //         let sz = uibox.borrow().size;
        //         uibox.borrow_mut().pref_size =
        //             Some((UISize::DPixels(sz.width), UISize::DPixels(sz.height)));
        //         uibox.borrow_mut().resize_delta = Point::default();
        //     }
        // }
        // if uibox.borrow().draggable() && self.event.mouse.is_some() {
        //     if self.event.click.is_some() {
        //         // XXX: direct write to origin only works for floating panes
        //         let delta_x = self.event.click.unwrap().x - self.event.mouse.unwrap().x;
        //         let delta_y = self.event.click.unwrap().y - self.event.mouse.unwrap().y;
        //         let fixed_origin = uibox.borrow().fixed_origin;
        //         uibox.borrow_mut().origin =
        //             Point::new(fixed_origin.x - delta_x, fixed_origin.y - delta_y);
        //     } else if uibox.borrow().clicked() {
        //         let mut uibox = uibox.borrow_mut();
        //         uibox.fixed_origin = uibox.origin;
        //     }
        // }
        if point_in_rect(&uibox.borrow().bounds(), self.event.mouse) {
            uibox.borrow_mut().events |= UIBoxEvent::MouseOver as u64;
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
        uibox.borrow_mut().style.bg_color = self.style.main_color;
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
