mod text_input_state;
pub mod uibox;
mod widgets;

use std::{cell::RefCell, collections::HashMap, rc::Rc};
use text_input_state::IMUITextInputState;
use uibox::{
    Color, Padding, UIBox, UIBoxEvent, UIBoxFlag, UIBoxHandle, UIBoxParams, UIBoxRef, UIBoxStyle,
    u64_hash_from_string,
};

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

use crate::{
    draw::{self, Drawer},
    os::{self, OSEvent, OSEventFlag, OSEventType, OSKey, OSKeyCode},
    render::{self, RectCoords, V4f32, font_cache::FontCache},
};

pub enum UITextAlign {
    Left,
    Center,
}

#[derive(Clone, Copy, PartialEq)]
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
    pub fn x(&self) -> f32 {
        self.x
    }
    pub fn y(&self) -> f32 {
        self.y
    }
    pub fn axis(&self, axis: Axis) -> &f32 {
        match axis {
            Axis::X => &self.x,
            Axis::Y => &self.y,
        }
    }
    pub fn axis_mut(&mut self, axis: Axis) -> &mut f32 {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UISize {
    Fixed(f32),           // DPI-scaled pixels
    Percent(f32),         // Percentage of parent's size
    Fit,                  // Wrap to content (text or children)
    FitMin(f32),          // Fit with minimum
    FitMax(f32),          // Fit with maximum
    FitMinMax(f32, f32),  // Fit with min and max
    Grow,                 // Fill remaining space (multiple allowed!)
    GrowMin(f32),         // Grow with minimum
    GrowMax(f32),         // Grow with maximum
    GrowMinMax(f32, f32), // Grow with min and max
    GrowWeight(f32),      // Grow with weight for proportional distribution
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum MainAxisAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum CrossAxisAlign {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Copy, Clone, Debug, PartialEq)]
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
    left_mouse_held: bool,
}

#[derive(Clone, Copy)]
pub enum UILocaleKind {
    LtrTtb, // European languages
    RtlTtb, // Hebrew, Arabic like
    TtbLtr, // Mongolian like
    TtbRtl, // Japanese like
}

pub struct UITheme {
    pub color_bg: Color,
    pub color_bg_popup: Color,
    pub color_main: Color,
    pub color_text: Color,
    pub size_text: f32,
}
impl UITheme {
    pub fn default() -> Self {
        UITheme {
            color_bg: Color::new("#ffffff"),
            color_bg_popup: Color::new("#12121280"),
            color_main: Color::new("#1ebc93"),
            color_text: Color::new("#ffffff"),
            size_text: 12.,
        }
    }
}

pub struct IMUI {
    // persistent data
    drawer: Drawer,
    root: UIBoxRef,
    floating_roots: Vec<UIBoxRef>,
    locale_kind: UILocaleKind,
    prompt: Option<UIBoxRef>,

    // per-build data
    size: Size,
    event: IMUIEventState,
    text_input_state: Option<IMUITextInputState>,

    // focus state (raddbg-inspired deferred focus)
    focus_active: Option<String>, // current frame's active focus key
    next_focus_active: Option<String>, // staged for next frame

    // ui construction helpers
    parent_stack: Vec<UIBoxRef>,
    uiboxes: HashMap<u64, UIBoxRef>,
    pub theme: UITheme,
    dirty: bool,
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
            size: Size {
                width: 0.,
                height: 0.,
            },
            event: IMUIEventState::default(),
            text_input_state: None,
            focus_active: None,
            next_focus_active: None,
            locale_kind: UILocaleKind::LtrTtb,
            prompt: None,
            root: root.clone(),
            floating_roots: Vec::new(),
            uiboxes: HashMap::new(),
            parent_stack: vec![root.clone()],
            theme: UITheme::default(),
            dirty: true,
        }
    }
    pub fn eventloop(&mut self, mut build_ui_func: impl FnMut(&mut IMUI)) {
        #[cfg(debug_assertions)]
        let mut start = os::timer_value();
        #[cfg(debug_assertions)]
        let freq = os::timer_init();
        let mut fc = 0usize;
        #[cfg(debug_assertions)]
        self.drawer.renderer.vsync(false);
        loop {
            fc += 1;
            // xarkes: handle events
            {
                self.pull_consume_events();
                if self.event.events.len() == 0 && fc > 2 && !self.dirty {
                    // xarkes: don't draw anything when not needed
                    // here we check for frame > 2 as we need 2 frames before displaying anything
                    // this allows to run the logic that needs extra frames, e.g. showing and hiding a prompt
                    // XXX(xarkes): dirty, we should wake up once an event is triggered rather than polling all the time
                    // especially because it caps our FPS
                    // but for the time being this allows us not eating all the CPU
                    std::thread::sleep(core::time::Duration::from_millis(16));
                    #[cfg(not(debug_assertions))]
                    continue;
                }
                let maybe_new_size = self.drawer.renderer.win.get_size();
                if maybe_new_size.0 != self.size.width || maybe_new_size.1 != self.size.height {
                    self.resize();
                }
            }

            // xarkes: clean previous state
            {
                self.root.borrow_mut().children.clear();
                self.floating_roots.clear();

                // commit staged focus (raddbg-inspired deferred focus)
                self.focus_active = self.next_focus_active.take();
            }

            // xarkes: build interface
            {
                build_ui_func(self);
                // #[cfg(debug_assertions)]
                // draw_debug_info(self, self.debug.clone(), time);
                self.layout_roots();
            }

            // xarkes: draw interface and render
            {
                self.draw_ui_all();

                #[cfg(debug_assertions)]
                {
                    let end = os::timer_value();
                    let time = (end - start) as f64;
                    let text = format!(
                        "debug build: {:.0}fps - {:.2}ms",
                        freq / time,
                        time * 1000. / freq
                    );
                    self.drawer.draw_text(
                        self.size.width / 2.
                            - self.drawer.get_text_size(12., text.as_str(), text.len()).0 / 2.,
                        12.,
                        12.,
                        text.as_str(),
                        text.len(),
                        self.size.width,
                        self.size.height,
                        Color::new("#ff0"),
                        false,
                        false,
                    );
                    start = end;
                }

                self.drawer.renderer.render_frame();
                if self.dirty {
                    self.dirty = false;
                    fc = 0;
                }
            }
        }
    }

    /// Phase 1: Bottom-up dimension calculation
    /// Post-order traversal to calculate intrinsic sizes for Fixed and Fit elements
    fn calculate_intrinsic_sizes(&self, root: UIBoxRef, axis: Axis) {
        // Post-order traversal: process children first, then parent
        iter_root_postorder(root, |nodeptr| {
            let mut node = nodeptr.borrow_mut();
            let size_spec = match axis {
                Axis::X => node.width,
                Axis::Y => node.height,
            };

            let computed = match size_spec {
                UISize::Fixed(pixels) => pixels,
                UISize::Fit | UISize::FitMin(_) | UISize::FitMax(_) | UISize::FitMinMax(_, _) => {
                    let intrinsic = self.compute_fit_size(&node, axis);
                    match size_spec {
                        UISize::FitMin(min) => intrinsic.max(min),
                        UISize::FitMax(max) => intrinsic.min(max),
                        UISize::FitMinMax(min, max) => intrinsic.clamp(min, max),
                        _ => intrinsic,
                    }
                }
                UISize::Percent(pct) => {
                    // Will be resolved in phase 2, but we need parent size
                    // For now, mark as 0 - will be computed in assign phase
                    if let Some(parent) = &node.parent {
                        let parent_size = *parent.borrow().computed_size.axis(axis);
                        let padding = match axis {
                            Axis::X => parent.borrow().padding.horizontal(),
                            Axis::Y => parent.borrow().padding.vertical(),
                        };
                        pct * (parent_size - padding)
                    } else {
                        0.0
                    }
                }
                // Grow variants: will be resolved in phase 2
                UISize::Grow
                | UISize::GrowMin(_)
                | UISize::GrowMax(_)
                | UISize::GrowMinMax(_, _)
                | UISize::GrowWeight(_) => {
                    // Mark with minimum size for now
                    match size_spec {
                        UISize::GrowMin(min) | UISize::GrowMinMax(min, _) => min,
                        _ => 0.0,
                    }
                }
            };
            *node.computed_size.axis_mut(axis) = computed;
        });
    }

    /// Helper: compute intrinsic (fit) size based on content
    fn compute_fit_size(&self, node: &UIBox, axis: Axis) -> f32 {
        // If there's text content, use text size
        if let Some(string) = &node.string {
            return match axis {
                Axis::X => {
                    self.drawer
                        .get_text_size(node.style.font_size, string.as_str(), string.len())
                        .0
                }
                Axis::Y => self
                    .drawer
                    .renderer
                    .font_cache
                    .borrow()
                    .line_height(node.style.font_size),
            };
        }

        // Otherwise, compute from children
        if node.children.is_empty() {
            return 0.0;
        }

        let padding = match axis {
            Axis::X => node.padding.horizontal(),
            Axis::Y => node.padding.vertical(),
        };

        let layout = node
            .layout
            .unwrap_or(UILayout::Vertical)
            .specialize(self.locale_kind);
        let is_main_axis = match layout {
            UILayout::HorizontalLtr | UILayout::HorizontalRtl => axis == Axis::X,
            UILayout::VerticalLtr | UILayout::VerticalRtl => axis == Axis::Y,
            _ => axis == Axis::Y,
        };

        if is_main_axis {
            // Sum of children along main axis + gaps
            let mut total = 0.0;
            for child in &node.children {
                total += *child.borrow().computed_size.axis(axis);
            }
            let gaps = if node.children.len() > 1 {
                node.child_gap * (node.children.len() - 1) as f32
            } else {
                0.0
            };
            total + gaps + padding
        } else {
            // Max of children along cross axis
            let mut max_size: f32 = 0.0;
            for child in &node.children {
                max_size = max_size.max(*child.borrow().computed_size.axis(axis));
            }
            max_size + padding
        }
    }

    /// Phase 2: Top-down position assignment
    /// Pre-order traversal to distribute space among Grow children and apply alignment
    fn assign_positions_and_grow(&self, root: UIBoxRef, axis: Axis) {
        iter_root(root, |nodeptr| {
            // Handle absolute positioning
            {
                let node = nodeptr.borrow();
                if let Some(layout) = node.layout {
                    if layout == UILayout::Absolute {
                        let fixed = node.fixed_origin;
                        drop(node);
                        nodeptr.borrow_mut().origin = fixed;
                        return false;
                    }
                }
            }

            // Skip if no parent (root node)
            if nodeptr.borrow().parent.is_none() {
                return false;
            }

            let parent = nodeptr.borrow().parent.clone().unwrap();

            // First, gather info we need without holding mutable borrow
            let (size_spec, parent_content_size, is_main_axis, cross_axis_align) = {
                let node = nodeptr.borrow();
                let parent_b = parent.borrow();

                let size_spec = match axis {
                    Axis::X => node.width,
                    Axis::Y => node.height,
                };

                let parent_available = *parent_b.computed_size.axis(axis);
                let padding = match axis {
                    Axis::X => parent_b.padding.horizontal(),
                    Axis::Y => parent_b.padding.vertical(),
                };
                let parent_content_size = parent_available - padding;

                let parent_layout = parent_b
                    .layout
                    .unwrap_or(UILayout::Vertical)
                    .specialize(self.locale_kind);
                let is_main_axis = match parent_layout {
                    UILayout::HorizontalLtr | UILayout::HorizontalRtl => axis == Axis::X,
                    UILayout::VerticalLtr | UILayout::VerticalRtl => axis == Axis::Y,
                    _ => axis == Axis::Y,
                };

                (
                    size_spec,
                    parent_content_size,
                    is_main_axis,
                    parent_b.cross_axis_align,
                )
            };

            // Now calculate grow space if needed (no borrows held)
            let grow_info = match size_spec {
                UISize::Grow
                | UISize::GrowMin(_)
                | UISize::GrowMax(_)
                | UISize::GrowMinMax(_, _)
                | UISize::GrowWeight(_)
                    if is_main_axis =>
                {
                    let parent_b = parent.borrow();
                    Some(self.calculate_grow_space(&parent_b, axis))
                }
                _ => None,
            };

            // Now apply the computed sizes
            {
                let mut node = nodeptr.borrow_mut();

                match size_spec {
                    UISize::Grow
                    | UISize::GrowMin(_)
                    | UISize::GrowMax(_)
                    | UISize::GrowMinMax(_, _)
                    | UISize::GrowWeight(_) => {
                        if is_main_axis {
                            let (remaining, grow_count, total_weight) = grow_info.unwrap();

                            let weight = match size_spec {
                                UISize::GrowWeight(w) => w,
                                _ => 1.0,
                            };

                            let share = if total_weight > 0.0 {
                                remaining * (weight / total_weight)
                            } else if grow_count > 0 {
                                remaining / grow_count as f32
                            } else {
                                0.0
                            };

                            let final_size = match size_spec {
                                UISize::GrowMin(min) => share.max(min),
                                UISize::GrowMax(max) => share.min(max),
                                UISize::GrowMinMax(min, max) => share.clamp(min, max),
                                _ => share,
                            };
                            *node.computed_size.axis_mut(axis) = final_size.max(0.0);
                        } else {
                            let final_size = match size_spec {
                                UISize::GrowMin(min) => parent_content_size.max(min),
                                UISize::GrowMax(max) => parent_content_size.min(max),
                                UISize::GrowMinMax(min, max) => parent_content_size.clamp(min, max),
                                _ => parent_content_size,
                            };
                            *node.computed_size.axis_mut(axis) = final_size.max(0.0);
                        }
                    }
                    _ => {}
                }

                // Handle cross-axis stretch alignment
                if !is_main_axis && cross_axis_align == CrossAxisAlign::Stretch {
                    match size_spec {
                        UISize::Fit
                        | UISize::FitMin(_)
                        | UISize::FitMax(_)
                        | UISize::FitMinMax(_, _) => {
                            *node.computed_size.axis_mut(axis) = parent_content_size.max(0.0);
                        }
                        _ => {}
                    }
                }
            }

            // Now position this node within its parent
            self.position_child_in_parent(nodeptr.clone(), axis);

            false
        });
    }

    /// Calculate remaining space for Grow children and count them
    fn calculate_grow_space(&self, parent: &UIBox, axis: Axis) -> (f32, usize, f32) {
        let parent_available = *parent.computed_size.axis(axis);
        let padding = match axis {
            Axis::X => parent.padding.horizontal(),
            Axis::Y => parent.padding.vertical(),
        };

        let gaps = if parent.children.len() > 1 {
            parent.child_gap * (parent.children.len() - 1) as f32
        } else {
            0.0
        };

        let mut fixed_size = 0.0;
        let mut grow_count = 0;
        let mut total_weight = 0.0;

        for child in &parent.children {
            let child_b = child.borrow();
            let size_spec = match axis {
                Axis::X => child_b.width,
                Axis::Y => child_b.height,
            };

            match size_spec {
                UISize::Grow
                | UISize::GrowMin(_)
                | UISize::GrowMax(_)
                | UISize::GrowMinMax(_, _) => {
                    grow_count += 1;
                    total_weight += 1.0;
                }
                UISize::GrowWeight(w) => {
                    grow_count += 1;
                    total_weight += w;
                }
                _ => {
                    fixed_size += *child_b.computed_size.axis(axis);
                }
            }
        }

        let remaining = (parent_available - padding - gaps - fixed_size).max(0.0);
        (remaining, grow_count, total_weight)
    }

    /// Position a child within its parent based on layout and alignment
    fn position_child_in_parent(&self, nodeptr: UIBoxRef, axis: Axis) {
        // Gather all info we need first, without holding mutable borrows
        let (
            node_size,
            previous,
            is_main_axis,
            padding_start,
            padding_end,
            parent_origin,
            parent_size,
            scroll_offset,
            child_gap,
            main_axis_align,
            cross_axis_align,
            total_children_size,
            num_children,
        ) = {
            let node_b = nodeptr.borrow();
            let parent = match &node_b.parent {
                Some(p) => p.clone(),
                None => return,
            };
            let node_size = *node_b.computed_size.axis(axis);
            let previous = node_b.previous.clone();
            drop(node_b);

            let parent_b = parent.borrow();
            let parent_layout = parent_b
                .layout
                .unwrap_or(UILayout::Vertical)
                .specialize(self.locale_kind);

            let is_main_axis = match parent_layout {
                UILayout::HorizontalLtr | UILayout::HorizontalRtl => axis == Axis::X,
                UILayout::VerticalLtr | UILayout::VerticalRtl => axis == Axis::Y,
                _ => axis == Axis::Y,
            };

            let padding_start = match axis {
                Axis::X => parent_b.padding.left,
                Axis::Y => parent_b.padding.top,
            };
            let padding_end = match axis {
                Axis::X => parent_b.padding.right,
                Axis::Y => parent_b.padding.bottom,
            };

            let parent_origin = *parent_b.origin.axis(axis);
            let parent_size = *parent_b.computed_size.axis(axis);
            let scroll_offset = match axis {
                Axis::X => parent_b.scrollx,
                Axis::Y => parent_b.scrolly,
            };
            let child_gap = parent_b.child_gap;
            let main_axis_align = parent_b.main_axis_align;
            let cross_axis_align = parent_b.cross_axis_align;
            let num_children = parent_b.children.len();

            // Calculate total children size now while we don't have nodeptr borrowed
            let total_children_size = self.calculate_total_children_size(&parent_b, axis);
            drop(parent_b);

            (
                node_size,
                previous,
                is_main_axis,
                padding_start,
                padding_end,
                parent_origin,
                parent_size,
                scroll_offset,
                child_gap,
                main_axis_align,
                cross_axis_align,
                total_children_size,
                num_children,
            )
        };

        // Calculate the position
        let position = if is_main_axis {
            match &previous {
                Some(prev) => {
                    let prev_b = prev.borrow();
                    let prev_end = *prev_b.origin.axis(axis) + *prev_b.computed_size.axis(axis);
                    prev_end + child_gap
                }
                None => {
                    let base = parent_origin + padding_start + scroll_offset;
                    let available_space =
                        parent_size - padding_start - padding_end - total_children_size;

                    match main_axis_align {
                        MainAxisAlign::Start => base,
                        MainAxisAlign::Center => base + available_space / 2.0,
                        MainAxisAlign::End => base + available_space,
                        MainAxisAlign::SpaceBetween => base,
                        MainAxisAlign::SpaceAround => {
                            let n = num_children as f32;
                            if n > 0.0 {
                                base + available_space / (2.0 * n)
                            } else {
                                base
                            }
                        }
                        MainAxisAlign::SpaceEvenly => {
                            let n = num_children as f32;
                            base + available_space / (n + 1.0)
                        }
                    }
                }
            }
        } else {
            let available = parent_size - padding_start - padding_end;
            match cross_axis_align {
                CrossAxisAlign::Start => parent_origin + padding_start + scroll_offset,
                CrossAxisAlign::Center => {
                    parent_origin + padding_start + scroll_offset + (available - node_size) / 2.0
                }
                CrossAxisAlign::End => {
                    parent_origin + padding_start + scroll_offset + available - node_size
                }
                CrossAxisAlign::Stretch => parent_origin + padding_start + scroll_offset,
            }
        };

        // Now apply the position with a short mutable borrow
        *nodeptr.borrow_mut().origin.axis_mut(axis) = position;
    }

    /// Calculate total size of children along an axis (including gaps)
    fn calculate_total_children_size(&self, parent: &UIBox, axis: Axis) -> f32 {
        let mut total = 0.0;
        for child in &parent.children {
            total += *child.borrow().computed_size.axis(axis);
        }
        if parent.children.len() > 1 {
            total += parent.child_gap * (parent.children.len() - 1) as f32;
        }
        total
    }

    fn layout_root(&mut self, root: UIBoxRef) {
        // Phase 1: Bottom-up intrinsic size calculation
        for axis in [Axis::X, Axis::Y] {
            self.calculate_intrinsic_sizes(root.clone(), axis);
        }
        // Phase 2: Top-down position and grow assignment
        for axis in [Axis::X, Axis::Y] {
            self.assign_positions_and_grow(root.clone(), axis);
        }
    }

    // TODO(xarkes): Rewrite using Clay's layout algorithm
    fn layout_roots(&mut self) {
        // TODO(xarkes): Merge normal root with "floating" roots
        self.layout_root(self.root.clone());
        for root in &self.floating_roots.clone() {
            self.layout_root(root.clone());
        }
    }

    // TODO(xarkes): should we introduce a "resolved style" struct that takes into account possible
    // parent style?
    // i.e. make every style None by default, explicitely set it by the user, and resolve it later (take parent if none, or keep none, or idk)
    fn draw_ui_root(&mut self, root: UIBoxRef) {
        iter_root(root, |curnode| {
            let curnode = curnode.borrow();

            if !curnode.visible() {
                return true;
            }

            let bounds = curnode.bounds();

            // xarkes: for each box, send the proper draw commands
            if curnode.draw_background() {
                let color = match curnode.clickable() && curnode.draw_hot() && curnode.hover() {
                    true => {
                        let col = curnode.style.bg_color;
                        Color {
                            r: col.r + 50. / 256.,
                            g: col.g + 50. / 256.,
                            b: col.b + 50. / 256.,
                            a: col.a + 0.2,
                        }
                    }
                    false => curnode.style.bg_color,
                };
                self.drawer.draw_rect(&bounds, color);
            }

            if curnode.draw_border() {
                let color = curnode.style.bg_color;
                self.drawer
                    .draw_empty_rect(&bounds, color, curnode.style.border_size);
            }

            if curnode.draw_text() {
                if let Some(string) = &curnode.string {
                    // TODO(xarkes): Clipping should be applied here
                    let margin = curnode.style.margin;
                    self.drawer.draw_text(
                        bounds.x0 + margin,
                        bounds.y0 + margin,
                        curnode.style.font_size,
                        string.as_str(),
                        string.len(),
                        bounds.x1 + margin,
                        bounds.y1 + margin,
                        curnode.style.text_color,
                        false,
                        curnode.style.font_icon,
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
    pub fn pull_consume_events(&mut self) {
        self.event.events = self.drawer.renderer.win.get_events();

        // xarkes: consume global scope events
        let mut escape_key_pressed = false;
        self.event.events.retain(|ev| {
            self.dirty = true;
            let mut retain = true;
            // Track global mouse button state
            if ev.key == OSKey::LeftMouseButton {
                if ev.ty == OSEventType::Press {
                    self.event.left_mouse_held = true;
                } else if ev.ty == OSEventType::Release {
                    self.event.left_mouse_held = false;
                }
            }
            if ev.ty == OSEventType::Press {
                if ev.key == OSKey::Keyboard(OSKeyCode::KeyEscape) {
                    escape_key_pressed = true;
                }
                if let Some(textinput) = self.text_input_state.as_mut() {
                    retain = !textinput.handle_event(&ev.key, &ev.chars, ev.flags);
                }
            }
            retain
        });

        if escape_key_pressed {
            self.text_input_state = None;
            self.clear_prompt();
        }
        // TODO(xarkes): we may want to propagate the event back to the OS window when the application did not consume them
    }
    pub(crate) fn resize(&mut self) -> Size {
        self.size = Size::from(self.drawer.renderer.win.get_size());
        let render_size = self.drawer.renderer.win.get_render_size();
        self.drawer.renderer.resize(render_size.0, render_size.1);
        self.root.borrow_mut().width = UISize::Fixed(self.size.width);
        self.root.borrow_mut().height = UISize::Fixed(self.size.height);
        self.root.borrow_mut().computed_size.width = self.size.width;
        self.root.borrow_mut().computed_size.height = self.size.height;
        self.size
    }
    pub fn text_input_changecount(&self) -> Option<usize> {
        match &self.text_input_state {
            Some(state) => Some(state.changecount),
            None => None,
        }
    }
    pub fn input(&mut self, key: OSKey, flags: Option<OSEventFlag>) -> bool {
        let mut handled = false;
        self.event.events.retain(|ev| {
            if ev.key == key {
                if flags.is_some() && ev.flags.is_some() {
                    if flags.unwrap() as u32 & (ev.flags.unwrap() as u32) > 0 {
                        handled = true;
                        return false;
                    }
                } else {
                    handled = true;
                    return false;
                }
            }
            true
        });
        handled
    }

    /// Get current mouse position
    pub fn mouse_position(&self) -> Option<Point> {
        self.event.mouse
    }

    /// Check if left mouse button is currently held down
    pub fn mouse_down(&self) -> bool {
        self.event.left_mouse_held
    }

    /////////////////////////////////
    //// Widgets functions
    pub fn row(&mut self, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        let row = self.add_box_from_string(None, 0);

        // XXX(xarkes): layout should be passed to add_box_from_string maybe
        row.borrow_mut().layout = Some(UILayout::Horizontal);
        row.borrow_mut().width = UISize::Grow;
        row.borrow_mut().height = UISize::Grow;

        self.parent_stack.push(row.clone());
        children(self);
        self.parent_stack.pop();
        UIBoxHandle::new(row)
    }
    pub fn column(&mut self, children: impl FnOnce(&mut IMUI)) -> UIBoxHandle {
        let column = self.add_box_from_string(None, 0);

        // XXX(xarkes): layout should be passed to add_box_from_string maybe
        column.borrow_mut().layout = Some(UILayout::Vertical);
        column.borrow_mut().width = UISize::Grow;
        column.borrow_mut().height = UISize::Grow;

        self.parent_stack.push(column.clone());
        children(self);
        self.parent_stack.pop();
        UIBoxHandle::new(column)
    }
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
            let (w, h) = match layout.specialize(self.locale_kind) {
                UILayout::VerticalLtr => (UISize::Percent(1.), UISize::Fit),
                UILayout::HorizontalLtr => (UISize::Percent(1.), UISize::Fit),
                _ => {
                    println!("Unsupported layout");
                    (UISize::Percent(1.), UISize::Fit)
                }
            };
            container.borrow_mut().width = w;
            container.borrow_mut().height = h;

            if let Some(params) = params {
                if let Some(width) = params.width {
                    container.borrow_mut().width = width;
                }
                if let Some(height) = params.height {
                    container.borrow_mut().height = height;
                }
            }
        }

        container.borrow_mut().layout = Some(layout);
        self.parent_stack.push(container.clone());
        children(self);
        self.parent_stack.pop();
        container
    }

    pub fn line_edit(
        &mut self,
        text_buffer: Rc<RefCell<String>>,
        id: &str,
        focus: bool,
    ) -> UIBoxHandle {
        let line_edit = self.add_box_from_string(
            Some(id),
            UIBoxFlag::Clickable as u64
                | UIBoxFlag::DrawBackground as u64
                | UIBoxFlag::DrawBorder as u64,
        );
        line_edit.borrow_mut().layout = Some(UILayout::Vertical);
        let multiline = false;
        self.text_edit_impl(line_edit.clone(), text_buffer, multiline, focus);
        UIBoxHandle::new(line_edit)
    }
    pub fn textarea(&mut self, text_buffer: Rc<RefCell<String>>, id: &str) -> UIBoxHandle {
        // check if this textarea should auto-focus (raddbg-inspired deferred focus)
        let force_focus = self.focus_active.as_ref().is_some_and(|key| key == id);
        if force_focus {
            self.focus_active = None; // consume the focus request
        }

        // xarkes: compute scrollbars
        let max_size_x = {
            let buf = text_buffer.borrow();
            let lines = buf.lines();
            let mut width = 0.;
            for l in lines {
                let w = self.drawer.get_text_size(12., l, l.len()).0;
                width = f32::max(w, width);
            }
            width as f32
        };
        let max_size_y = text_buffer.borrow().lines().count() as f32
            * self.drawer.renderer.font_cache.borrow().line_height(12.);

        self.column(|ui| {
            let mut textarea = None;
            ui.row(|ui| {
                let textarea_ = ui.add_box_from_string(
                    Some(id),
                    UIBoxFlag::Clickable as u64 | UIBoxFlag::Scrollable as u64,
                );
                textarea_.borrow_mut().width = UISize::Grow;
                textarea_.borrow_mut().height = UISize::Grow;
                textarea_.borrow_mut().layout = Some(UILayout::Vertical);

                // xarkes: fixup scrolling: event handler does not know the child's logic
                {
                    let mut textarea = textarea_.borrow_mut();
                    let can_scroll_y = max_size_y > textarea.computed_size.height;
                    let can_scroll_x = max_size_x > textarea.computed_size.width;

                    if can_scroll_y {
                        let max_scroll_y = textarea.computed_size.height - max_size_y;
                        if textarea.scrolly > 0. {
                            textarea.scrolly = 0.;
                        } else if textarea.scrolly < max_scroll_y {
                            textarea.scrolly = max_scroll_y;
                        }
                    } else {
                        textarea.scrolly = 0.;
                    }
                    if can_scroll_x {
                        let max_scroll_x = textarea.computed_size.width - max_size_x;
                        if textarea.scrollx > 0. {
                            textarea.scrollx = 0.;
                        } else if textarea.scrollx < max_scroll_x {
                            textarea.scrollx = max_scroll_x;
                        }
                    } else {
                        textarea.scrollx = 0.;
                    }
                }

                let multiline = true;
                ui.text_edit_impl(
                    textarea_.clone(),
                    text_buffer.clone(),
                    multiline,
                    force_focus,
                );

                if max_size_y > textarea_.borrow().computed_size.height {
                    ui.scrollbar(textarea_.clone(), max_size_y, Axis::Y);
                }
                textarea = Some(textarea_.clone());
            });

            let textarea_ref = textarea.unwrap();
            if max_size_x > textarea_ref.borrow().computed_size.width {
                ui.scrollbar(textarea_ref.clone(), max_size_x, Axis::X);
            }
        })
    }
    fn text_edit_impl(
        &mut self,
        textarea: UIBoxRef,
        text_buffer: Rc<RefCell<String>>,
        multiline: bool,
        force_focus: bool,
    ) -> UIBoxRef {
        if textarea.borrow().clicked()
            || (force_focus
                && (self.text_input_state.is_none()
                    || self
                        .text_input_state
                        .as_ref()
                        .is_some_and(|tis| tis.focus.borrow().key != textarea.borrow().key)))
        {
            // xarkes: update the text input global state
            let mut state = IMUITextInputState::new(
                // textarea.key,
                textarea.clone(),
                self.drawer.renderer.font_cache.clone(),
                text_buffer.clone(),
                multiline,
            );
            let bounds = &textarea.borrow().bounds();
            if self.event.mouse.is_some() {
                state.compute_valid_cursor_loc(
                    bounds,
                    &text_buffer.borrow(),
                    self.theme.size_text,
                    self.event.mouse.unwrap(),
                    Point::new(textarea.borrow().scrollx, textarea.borrow().scrolly),
                );
            }
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
                let line_idx_end = line_idx_start
                    + (textarea.borrow().computed_size.height / line_height) as usize;
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
            // draw cursor
            if let Some(tis) = &self.text_input_state {
                if tis.focus.borrow().key == textarea.borrow().key {
                    let font_size = textarea.borrow().style.font_size;
                    let cursor_height = self
                        .drawer
                        .renderer
                        .font_cache
                        .borrow()
                        .line_height(font_size);
                    // Cursor spans most of the line height, with small padding
                    // Offset slightly to account for descender space in line_height
                    let cx = tis.cursor_x;
                    let cy = tis.cursor_y;
                    let cursor_box =
                        self.add_box_from_string(None, UIBoxFlag::DrawBackground as u64);
                    cursor_box.borrow_mut().width = UISize::Fixed(2.);
                    cursor_box.borrow_mut().height = UISize::Fixed(cursor_height);
                    let bounds = textarea.borrow().bounds();
                    cursor_box.borrow_mut().layout = Some(UILayout::Absolute);
                    cursor_box.borrow_mut().fixed_origin = Point::new(
                        bounds.x0 + cx + textarea.borrow().scrollx,
                        bounds.y0 + cy + textarea.borrow().scrolly - 3., // XXX: 3. is completely arbitrary
                    );
                    cursor_box.borrow_mut().style.bg_color = self.theme.color_text;
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
        let size_default = UISize::Fit;
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
                    width: size_default,
                    height: size_default,
                    computed_size: Size::default(),
                    origin: Point::default(),
                    fixed_origin: Point::default(),
                    children: Vec::new(),
                    parent: None,
                    previous: None,
                    layout: None,
                    visible: true,

                    flags,
                    events: 0,

                    string,

                    padding: Padding::default(),
                    child_gap: 0.,
                    main_axis_align: MainAxisAlign::default(),
                    cross_axis_align: CrossAxisAlign::default(),

                    scrollx: 0.,
                    scrolly: 0.,
                    style: UIBoxStyle {
                        margin: 1.,
                        font_size: self.theme.size_text,
                        border_size: 0.,
                        bg_color: self.theme.color_bg_popup,
                        font_icon: false,
                        text_color: self.theme.color_text,
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

    pub fn focus(&mut self, target: UIBoxHandle) {}

    fn handle_uibox_event(&mut self, uibox: UIBoxRef) {
        let mut should_clear_prompt = false;
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
                let in_prompt_bounds = match &self.prompt {
                    Some(prompt) => point_in_rect(&prompt.borrow().bounds(), ev.pos),
                    None => false,
                };
                if !in_prompt_bounds {
                    should_clear_prompt = true;
                }
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

        if point_in_rect(&uibox.borrow().bounds(), self.event.mouse) {
            uibox.borrow_mut().events |= UIBoxEvent::MouseOver as u64;
        }

        if self.prompt.is_some() && should_clear_prompt {
            println!("Clearing");
            self.clear_prompt();
        }
    }

    pub fn button_new(&mut self, label: &str) -> UIBoxHandle {
        let button = self.button(label, None);
        UIBoxHandle::new(button)
    }
    pub fn button(&mut self, label: &str, tooltip_text: Option<&str>) -> UIBoxRef {
        let uibox = self.add_box_from_string(
            Some(label),
            UIBoxFlag::Clickable as u64
                | UIBoxFlag::DrawBackground as u64
                | UIBoxFlag::DrawBorder as u64
                | UIBoxFlag::DrawText as u64
                | UIBoxFlag::DrawHot as u64,
        );
        uibox.borrow_mut().style.bg_color = Color::transparent();

        // xarkes: show tooltip when needed
        if uibox.borrow().hover() && tooltip_text.is_some() {
            let point = self.event.mouse.unwrap();
            let point = Point::new(point.x + 10., point.y - 10.);
            let tooltip = self.new_floating_root(0, point);
            let line_height = self
                .drawer
                .renderer
                .font_cache
                .borrow()
                .line_height(self.theme.size_text);
            tooltip.borrow_mut().width = UISize::Fit;
            tooltip.borrow_mut().height = UISize::Fixed(line_height);
            self.handle_uibox_event(tooltip.clone());
            self.parent_stack.push(tooltip);
            {
                self.label(tooltip_text.unwrap());
            }
            self.parent_stack.pop();
        }
        uibox
    }

    pub fn button_icon(&mut self, label: &str, tooltip_text: Option<&str>) -> UIBoxRef {
        let uibox = self.button(label, tooltip_text);
        uibox.borrow_mut().style.font_icon = true;
        uibox.borrow_mut().style.font_size = 24.;
        uibox.borrow_mut().width = UISize::Fixed(25.);
        uibox.borrow_mut().height = UISize::Fixed(25.);
        uibox
    }

    pub fn reset_text_input_state(&mut self) {
        self.text_input_state = None;
    }

    /// Request focus on a widget by key for the next frame.
    /// The focus will be committed at the start of the next frame and consumed
    /// when the matching widget is built.
    pub fn set_focus_active(&mut self, key: &str) {
        self.next_focus_active = Some(key.to_string());
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

/// Post-order traversal: children first, then parent
fn iter_root_postorder(start_node: UIBoxRef, mut handle_node: impl FnMut(UIBoxRef)) {
    fn visit(node: UIBoxRef, handler: &mut impl FnMut(UIBoxRef)) {
        // First visit all children
        let children = node.borrow().children.clone();
        for child in children {
            visit(child, handler);
        }
        // Then handle this node
        handler(node);
    }
    visit(start_node, &mut handle_node);
}
