use super::{Color, IMUI, UIBoxRef, UILocaleKind, UISize};

#[derive(Clone)]
pub(crate) struct IMUIDebug {
    pub fps: bool,
    pub hints: bool,
    pub vsync: bool,
    pub locale: bool,
}

impl IMUIDebug {
    pub fn default() -> Self {
        IMUIDebug {
            fps: true,
            hints: false,
            vsync: false,
            locale: true,
        }
    }
}

// fn draw_node_info(ui: &mut IMUI, node: UIWidgetRef) {
//     let mut i = 0;
//     let mut worklist = vec![node];
//     ui.label("-------------------");
//     loop {
//         let curnode = match worklist.pop() {
//             Some(n) => n,
//             None => {
//                 break;
//             }
//         };
//         for c in &curnode.borrow().children {
//             worklist.push(c.clone());
//         }
//         ui.label(
//             format!(
//                 "{1:0$}Node {2} ({3:.1}x{4:.1}, {5} children)",
//                 curnode.borrow().depth + 1,
//                 " ",
//                 i,
//                 curnode.borrow().bounds.x0,
//                 curnode.borrow().bounds.y0,
//                 curnode.borrow().children.len()
//             )
//             .as_str(),
//         );
//         i += 1;
//     }
// }

fn draw_debug_pane(ui: &mut IMUI, debug: &mut IMUIDebug, time: f64) {
    let xoff = ui.size.0 - 250.;
    ui.params()
        .width(UISize::DPixels(250.))
        .height(UISize::DPixels(400.))
        .position((UISize::DPixels(xoff), UISize::DPixels(40.)));
    ui.floating_pane("Debug metrics", |ui| {
        if debug.fps {
            let fps = 1f64 / time * 1000f64;
            let text = format!("Render: {:.2}ms - {}fps", time, fps as u64);
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
        // draw_node_info(ui, ui.root.clone());
    });

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
        .draw_empty_rect(&start.borrow().bounds, color, 1.0, true);
}

pub fn draw_debug_info(ui: &mut IMUI, mut debug: IMUIDebug, time: f64) {
    draw_debug_pane(ui, &mut debug, time);

    if debug.hints {
        draw_debug_hints(ui, ui.root.clone());
    }
}
