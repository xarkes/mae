use super::{Color, IMUI, Point, Size, UIBoxRef, UILocaleKind, UISize, iter_root};

#[derive(Clone)]
pub(crate) struct IMUIDebug {
    pub(crate) fps: bool,
    pub(crate) hints: bool,
    pub(crate) vsync: bool,
    pub(crate) locale: bool,
    pub(crate) target: Option<UIBoxRef>,
}

impl IMUIDebug {
    pub fn default() -> Self {
        IMUIDebug {
            fps: true,
            hints: false,
            vsync: false,
            locale: true,
            target: None,
        }
    }
}

fn draw_root_info(ui: &mut IMUI, node: UIBoxRef) {
    let mut count = 0;
    iter_root(node, |node| {
        count += 1;
        return false;
    });
    ui.label(&format!("{} nodes", count));
}

fn draw_target_node_info(ui: &mut IMUI) {
    if ui.debug.target.is_none() {
        ui.label("Target: None");
        ui.label("-");
        return;
    }
    let target = ui.debug.target.as_ref().unwrap().clone();
    ui.label(format!("Target: {:x}", target.borrow().key).as_str());
    ui.label(format!("  {:?}", target.borrow().style).as_str());
}

fn draw_debug_pane(ui: &mut IMUI, debug: &mut IMUIDebug, time: f64) {
    let xoff = ui.size.width - 200.;
    let fp = ui.floating_pane(
        Point::new(xoff, 40.),
        Size::from((200., 250.)),
        "Debug metrics",
        |ui| {
            if debug.fps {
                let fps = 1e9 / time;
                let text = format!("Render: {:.2}ms - {}fps", time / 1e6, fps as u64);
                ui.label(text.as_str());
            }
            // ui.checkbox("Show hints", &mut debug.hints);
            // if ui
            //     .checkbox("Enable VSync", &mut debug.vsync)
            //     .borrow()
            //     .clicked()
            // {
            //     ui.drawer.renderer.vsync(debug.vsync);
            // };
            // if ui.checkbox("LTR", &mut debug.locale).borrow().clicked() {
            //     ui.locale_kind = match debug.locale {
            //         true => super::UILocaleKind::LtrTtb,
            //         false => super::UILocaleKind::RtlTtb,
            //     };
            //     ui.event.drag_cache.clear();
            // };
            draw_target_node_info(ui);
            draw_root_info(ui, ui.root.clone());
            for root in ui.floating_roots.clone() {
                draw_root_info(ui, root);
            }
        },
    );
    fp.borrow_mut().pref_size = (UISize::DPixels(200.), UISize::Children);
    // fp.borrow_mut().pref_size = (UISize::DPixels(200.), UISize::Children);
    ui.debug = debug.clone();
}

fn draw_debug_hints(ui: &mut IMUI, start: UIBoxRef) {
    for c in &start.borrow().children {
        draw_debug_hints(ui, c.clone());
    }
    let color = match start.borrow().hover() {
        true => Color {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        },
        false => Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
    };
    ui.drawer
        .draw_empty_rect(&start.borrow().bounds(), color, 1.0, true);
}

pub fn draw_debug_info(ui: &mut IMUI, mut debug: IMUIDebug, time: f64) {
    draw_debug_pane(ui, &mut debug, time);

    if debug.hints {
        draw_debug_hints(ui, ui.root.clone());
    }
}
