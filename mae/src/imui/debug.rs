use super::{Color, IMUI, UIWidgetRef};

#[derive(Clone)]
pub(crate) struct IMUIDebug {
    pub fps: bool,
    pub hints: bool,
    pub vsync: bool,
    pub locale: bool,
    pub target: Option<UIWidgetRef>,
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

fn draw_debug_pane(ui: &mut IMUI, debug: &mut IMUIDebug) {
    ui.pane("Debug metrics", |ui| {
        ui.checkbox("Show fps", &mut debug.fps);
        ui.checkbox("Show hints", &mut debug.hints);
        if ui
            .checkbox("Enable VSync", &mut debug.vsync)
            .borrow()
            .clicked()
        {
            ui.drawer.renderer.vsync(debug.vsync);
        };
        ui.checkbox("LTR", &mut debug.locale);
    });

    ui.debug = debug.clone();
    ui.locale_kind = match debug.locale {
        true => super::UILocaleKind::LtrTtb,
        false => super::UILocaleKind::RtlTtb,
    };
}

fn draw_debug_hints(ui: &mut IMUI, start: UIWidgetRef) {
    for c in &start.borrow().children {
        draw_debug_hints(ui, c.clone());
    }
    ui.drawer.draw_empty_rect(
        &start.borrow().bounds,
        Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
        1.0,
        true,
    );
}

pub fn draw_debug_info(ui: &mut IMUI, mut debug: IMUIDebug, time: f64) {
    draw_debug_pane(ui, &mut debug);

    if debug.fps {
        let fps = 1f64 / time * 1000f64;
        let text = format!("{:.2}ms - {}fps", time, fps as u64);
        let font_size = 12;
        ui.drawer.draw_text(
            ui.size.0 - (text.len() as f32 * font_size as f32 / 1.6),
            0.0,
            font_size,
            text.as_str(),
            text.len(),
            Color {
                r: 1.,
                g: 0.,
                b: 0.,
                a: 1.,
            },
        );
    }

    if debug.hints {
        draw_debug_hints(ui, ui.root.clone());
    }
}
