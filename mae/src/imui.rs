mod text_input_state;
mod uibox;

use std::{cell::RefCell, collections::HashMap, rc::Rc};
use text_input_state::IMUITextInputState;
use uibox::{UIBox, UIBoxEvent, UIBoxFlag, UIBoxParams, UIBoxRef, u64_hash_from_string};

#[cfg(debug_assertions)]
mod debug;
#[cfg(debug_assertions)]
use debug::{IMUIDebug, draw_debug_info};

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

use crate::{
    draw::{self, Drawer},
    os::{self, OSEvent, OSEventType, OSKey},
    render::{self, Point, RectCoords, V4f32, font_cache::FontCache},
};

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

pub enum UITextAlign {
    Left,
    Center,
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

pub type RelPoint = (UISize, UISize);

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
    drawer: Drawer,
    #[cfg(debug_assertions)]
    debug: IMUIDebug,
    size: (f32, f32),
    params: UIBoxParams,
    event: IMUIEventState,
    text_input_state: Option<IMUITextInputState>,
    locale_kind: UILocaleKind,

    // ui construction helpers
    root: UIBoxRef,
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
            size: (0., 0.),
            params: UIBoxParams::new(),
            event: IMUIEventState::default(),
            text_input_state: None,
            locale_kind: UILocaleKind::LtrTtb,
            root: root.clone(),
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

            // xarkes: build interface
            {
                self.root.borrow_mut().children.clear();
                build_ui_func(self);
            }

            // TODO: Apply layout here
            // XXX: I wonder atm how will you in the future not renderer when nothing is happening?
            // I start to think I was wrong regarding the "only retained mode can do this" :')
            // no basically if I have a widget that say shows time
            // how do you know you have nothing to do until the string is different? this involves checking for each frame anyways? idk.
            self.build_ui_end();
            self.layout_root();

            #[cfg(debug_assertions)]
            {
                draw_debug_info(self, self.debug.clone(), time);
                self.layout_root();
            }

            // xarkes: draw interface and render
            {
                self.draw_ui();
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
                    text_input_state.focus.borrow().bounds.x0
                        + self.text_input_state.as_ref().unwrap().cursor_x
                }
                UILocaleKind::RtlTtb => {
                    text_input_state.focus.borrow().bounds.x1
                        - self.text_input_state.as_ref().unwrap().cursor_x
                }
                _ => {
                    println!("Textarea cursor localekind not handled!");
                    text_input_state.focus.borrow().bounds.x0
                        + self.text_input_state.as_ref().unwrap().cursor_x
                }
            };
            let cursory = text_input_state.focus.borrow().bounds.y0
                + self.text_input_state.as_ref().unwrap().cursor_y;

            let color = self.style.active_color;
            let height = self
                .drawer
                .renderer
                .font_cache
                .borrow()
                .line_height(self.style.text_size);
            self.params()
                .background_color(color)
                .width(uisize!("2px"))
                .height(UISize::DPixels(height))
                .position((UISize::DPixels(cursorx), UISize::DPixels(cursory)));
            self.add_box_from_string(None, UIBoxFlag::DrawBackground as u64);
        }
    }

    // TODO(xarkes): Consider doing this, which is likely a simpler algorithm
    // and will avoid shooting ourselves in the foot
    // This implies revisiting the originally thought layout system.
    // fn layout_standalone(&self) {
    //     self.iter_root(|node| {
    //     });
    // }
    // fn layout_upward_dependents(&self) {
    //     self.iter_root(|node| {});
    // }

    fn layout_root(&mut self) {
        // self.layout_standalone();
        // self.layout_upward_dependents();

        // xarkes: iterate created boxes from root (lowest), breadth first search (BFS)
        iter_root(self.root.clone(), |curnode_r| {
            let bounds = {
                let curnode = curnode_r.borrow();
                // xarkes: compute bounds depending on layout and requested size
                if curnode.parent.is_none() {
                    return false;
                }
                let parent = curnode.parent.as_ref().unwrap().borrow();
                if parent.layout.is_none() {
                    println!("Warning: box has children but no layout");
                    return false;
                }
                let layout = match parent.layout.unwrap() {
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
                    _ => parent.layout.unwrap(),
                };
                let req_size = (
                    curnode.style.width.unwrap_or(UISize::TextContent),
                    curnode.style.height.unwrap_or(UISize::TextContent),
                );
                fn compute_size(
                    imui: &mut IMUI,
                    node: UIBoxRef,
                    size: &UISize,
                    parent_size: f32,
                    axis: usize,
                ) -> f32 {
                    match size {
                        UISize::DPixels(v) => *v,
                        UISize::Percents(v) => v * parent_size,
                        UISize::TextContent => {
                            let tmp = node.borrow();
                            let string = tmp.string.as_ref().unwrap();
                            if axis == 0 {
                                imui.drawer
                                    .get_text_size(
                                        imui.style.text_size,
                                        string.as_str(),
                                        string.len(),
                                    )
                                    .0
                            } else {
                                imui.drawer
                                    .renderer
                                    .font_cache
                                    .borrow()
                                    .line_height(imui.style.text_size)
                            }
                        }
                    }
                }
                let bounds = match curnode.visible {
                    false => RectCoords::from_size(0., 0., 0., 0.),
                    true => match layout {
                        UILayout::Root => RectCoords::from_size(
                            compute_size(
                                self,
                                curnode_r.clone(),
                                &curnode.style.position.unwrap().0,
                                self.size.0,
                                0,
                            ),
                            compute_size(
                                self,
                                curnode_r.clone(),
                                &curnode.style.position.unwrap().1,
                                self.size.1,
                                1,
                            ),
                            compute_size(
                                self,
                                curnode_r.clone(),
                                &curnode.style.width.unwrap(),
                                self.size.0,
                                0,
                            ),
                            compute_size(
                                self,
                                curnode_r.clone(),
                                &curnode.style.height.unwrap(),
                                self.size.1,
                                1,
                            ),
                            // curnode.compute_size(&curnode.style.position.unwrap().0, &self.drawer),
                            // curnode.compute_size(&curnode.style.position.unwrap().1),
                            // curnode.compute_size(&curnode.style.width.unwrap()),
                            // curnode.compute_size(&curnode.style.height.unwrap()),

                            // curnode.style.position.unwrap().0.pixels(self.size.0),
                            // curnode.style.position.unwrap().1.pixels(self.size.1),
                            // curnode.style.width.unwrap().pixels(self.size.0),
                            // curnode.style.height.unwrap().pixels(self.size.1),
                        ),
                        UILayout::VerticalLtr => {
                            let insert_point = match &curnode.previous {
                                Some(prev) => (prev.borrow().bounds.x0, prev.borrow().bounds.y1),
                                None => (parent.bounds.x0, parent.bounds.y0),
                            };
                            RectCoords::from_size(
                                insert_point.0,
                                insert_point.1,
                                compute_size(
                                    self,
                                    curnode_r.clone(),
                                    &req_size.0,
                                    parent.bounds.width(),
                                    0,
                                ),
                                compute_size(
                                    self,
                                    curnode_r.clone(),
                                    &req_size.1,
                                    parent.bounds.height(),
                                    1,
                                ),
                            )
                        }
                        // UILayout::VerticalRtl => {
                        //     let insert_point = match parent.children.last() {
                        //         Some(child) => (
                        //             child.borrow().bounds.x1 - size.0.pixels(parent.bounds.width()),
                        //             child.borrow().bounds.y1,
                        //         ),
                        //         None => (
                        //             parent.bounds.x1 - size.0.pixels(parent.bounds.width()),
                        //             parent.bounds.y0,
                        //         ),
                        //     };

                        //     RectCoords::from_size(
                        //         insert_point.0,
                        //         insert_point.1,
                        //         f32::min(parent.bounds.width(), size.0.pixels(parent.bounds.width())),
                        //         f32::min(
                        //             parent.bounds.height(),
                        //             size.1.pixels(parent.bounds.height()),
                        //         ),
                        //     )
                        // }
                        // UILayout::HorizontalLtr => {
                        //     let insert_point = match parent.children.last() {
                        //         Some(child) => (child.borrow().bounds.x1, child.borrow().bounds.y0),
                        //         None => (parent.bounds.x0, parent.bounds.y0),
                        //     };
                        //     RectCoords::from_size(
                        //         insert_point.0,
                        //         insert_point.1,
                        //         f32::min(parent.bounds.width(), size.0.pixels(parent.bounds.width())),
                        //         f32::min(
                        //             parent.bounds.height(),
                        //             size.1.pixels(parent.bounds.height()),
                        //         ),
                        //     )
                        // }
                        // UILayout::HorizontalRtl => {
                        //     let insert_point = match parent.children.last() {
                        //         Some(child) => (
                        //             child.borrow().bounds.x0 - size.0.pixels(parent.bounds.width()),
                        //             child.borrow().bounds.y0,
                        //         ),
                        //         None => (
                        //             parent.bounds.x1 - size.0.pixels(parent.bounds.width()),
                        //             parent.bounds.y0,
                        //         ),
                        //     };
                        //     RectCoords::from_size(
                        //         insert_point.0,
                        //         insert_point.1,
                        //         f32::min(parent.bounds.width(), size.0.pixels(parent.bounds.width())),
                        //         f32::min(
                        //             parent.bounds.height(),
                        //             size.1.pixels(parent.bounds.height()),
                        //         ),
                        //     )
                        // }
                        _ => unreachable!("Generic layout impossible here!"),
                    },
                };
                bounds
            };
            curnode_r.borrow_mut().bounds = bounds;
            return !curnode_r.borrow().visible;
        });
    }

    fn draw_ui(&mut self) {
        iter_root(self.root.clone(), |curnode| {
            let curnode = curnode.borrow();

            if !curnode.visible {
                return true;
            }

            // xarkes: for each box, send the proper draw commands
            if curnode.draw_background() {
                let color = curnode.style.bg_col.unwrap_or(self.style.bg_color);
                // XXX: this doesnt work
                // let bounds = match curnode.click() {
                //     false => &curnode.bounds,
                //     true => &curnode.bounds.x(2.),
                // };
                let bounds = &curnode.bounds;
                self.drawer.draw_rect(bounds, color);
            }

            if curnode.draw_border() {
                let color = match curnode.hover() {
                    true => self.style.active_color,
                    false => self.style.main_color,
                };
                self.drawer
                    .draw_empty_rect(&curnode.bounds, color, 1.0, false);
            }

            if curnode.draw_text() {
                if let Some(string) = &curnode.string {
                    self.drawer.draw_text(
                        curnode.bounds.x0,
                        curnode.bounds.y0,
                        self.style.text_size,
                        string.as_str(),
                        string.len(),
                        self.style.text_color,
                    );
                }
            }

            // if debug.hints
            // self.drawer
            //     .draw_empty_rect(&curnode.bounds, color_rgb(255, 0, 0), 2.0, true);
            return false;
        });
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
        let pane = self.add_box_from_string(Some(id), 0);
        pane.borrow_mut().layout = Some(UILayout::Vertical);
        self.parent_stack.push(pane.clone());
        self.params.reset();
        children(self);
        self.parent_stack.pop();
        pane
    }
    pub fn floating_pane(&mut self, title: &str, mut children: impl FnMut(&mut IMUI)) -> UIBoxRef {
        let uibox = self.add_box_from_string(
            None,
            UIBoxFlag::Clickable as u64
                | UIBoxFlag::DrawBackground as u64
                | UIBoxFlag::DrawBorder as u64
                | UIBoxFlag::Draggable as u64,
        );
        uibox.borrow_mut().layout = Some(UILayout::Vertical);
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
        self.frame(foldable_frame_id.as_str(), |ui| {
            children(ui);
        });
        self.parent_stack.pop();
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
    // pub fn checkbox_widget(&mut self, value: &mut bool) -> UIWidgetRef {
    //     let line_height = self
    //         .drawer
    //         .renderer
    //         .font_cache
    //         .borrow()
    //         .line_height(self.style.text_size as f32);
    //     let box_size = line_height;
    //     let widget_r = self.layout_new_widget(
    //         None,
    //         None,
    //         (UISize::DPixels(box_size), UISize::DPixels(box_size)),
    //         UIBoxFlag::Clickable as u64,
    //         UILayout::Vertical,
    //     );
    //     let widget = widget_r.borrow();

    //     let draw_color = match *value {
    //         true => self.style.bg_color,
    //         false => self.style.text_color,
    //     };
    //     self.drawer.draw_rect(
    //         &RectCoords::from_size(widget.bounds.x0, widget.bounds.y0, box_size, box_size),
    //         draw_color,
    //     );
    //     let border_color = match widget.hover() {
    //         true => self.style.active_color,
    //         false => self.style.main_color,
    //     };
    //     self.drawer.draw_empty_rect(
    //         &RectCoords::from_size(widget.bounds.x0, widget.bounds.y0, box_size, box_size),
    //         border_color,
    //         1.0,
    //         false,
    //     );

    //     // TODO(xarkes): We need a better API...
    //     if widget.clicked() && self.event.release.is_some() {
    //         *value = !*value;
    //         // NOTE(xarkes): consume the release so clicked() is called only once
    //         self.event.release = None;
    //     }

    //     widget_r.clone()
    // }
    // pub fn checkbox(&mut self, label: &str, value: &mut bool) -> UIWidgetRef {
    //     // TODO(xarkes): horizontal callback returning a widget sucks
    //     self.horizontal(|ui| {
    //         let checkbox = ui.checkbox_widget(value);
    //         ui.label(label);
    //         checkbox
    //     })
    // }
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
            UIBoxFlag::Clickable as u64 | UIBoxFlag::DrawBackground as u64,
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
            let bounds = &textarea.borrow().bounds;
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

    fn add_box_from_string(&mut self, label: Option<&str>, flags: u64) -> UIBoxRef {
        let parent = self.parent_stack.last().unwrap();
        let (key, string) = self.get_key_from_string(label);
        let uibox = match self.uiboxes.get(&key) {
            Some(uibox) => {
                // xarkes: clear previous frame event info
                uibox.borrow_mut().events = 0;
                uibox.borrow_mut().children.clear();
                uibox.clone()
            }
            None => {
                let uibox = Rc::new(RefCell::new(UIBox {
                    key,
                    bounds: RectCoords::from_size(0., 0., 0., 0.),
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

                    style: self.params.clone(),
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
            let in_bounds = point_in_rect(&uibox.borrow().bounds, ev.pos);
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
        }

        if point_in_rect(&uibox.borrow().bounds, self.event.mouse) {
            uibox.borrow_mut().events |= UIBoxEvent::MouseOver as u64;
        }
        if point_in_rect(&uibox.borrow().bounds, self.event.mouse) {
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
        point.0 >= loc.x0 && point.0 <= loc.x1 && point.1 >= loc.y0 && point.1 <= loc.y1
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
    let mut worklist = Vec::new();
    let mut start = start_node.borrow().children.clone();
    start.reverse();
    for c in start {
        worklist.push(c.clone());
    }
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
