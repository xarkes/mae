use super::{Color, IMUI, UILocaleKind, UIWidgetRef};

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
        if ui.checkbox("LTR", &mut debug.locale).borrow().clicked() {
            ui.locale_kind = match debug.locale {
                true => super::UILocaleKind::LtrTtb,
                false => super::UILocaleKind::RtlTtb,
            };
            ui.event.drag_cache.clear();
        };
    });

    ui.debug = debug.clone();
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
        let bounds = ui.root.borrow().bounds;

        // XXX: Hack
        let tmp0 = ui.style.text_color;
        let tmp1 = ui.locale_kind;
        ui.style.text_color = Color {
            r: 1.,
            g: 0.,
            b: 0.,
            a: 1.,
        };
        ui.locale_kind = UILocaleKind::RtlTtb;
        ui.draw_text(&bounds, text.as_str(), text.len(), font_size);
        ui.locale_kind = tmp1;
        ui.style.text_color = tmp0;
    }

    if debug.hints {
        draw_debug_hints(ui, ui.root.clone());
    }
}
