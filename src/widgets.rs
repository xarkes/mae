use crate::{
    UIState, draw,
    render::{RectCoords, V4f32},
};

const COLOR_BG: V4f32 = V4f32 {
    r: 0.2,
    g: 0.2,
    b: 0.2,
    a: 1.0,
};

pub fn button(ui: &UIState, coords: &RectCoords, label: Option<&str>) {
    let mut bg_color = draw::color::TMP;
    if ui.hover(coords) {
        bg_color = draw::color::TMP2;
    }
    ui.drawer.draw_rect(coords, bg_color);
    if let Some(label) = label {
        ui.drawer.draw_text(
            coords.x0,
            coords.y0,
            12,
            label,
            label.len(),
            draw::color::WHITE,
        );
    }
}

pub fn treeview(ui: &UIState, x: f32, y: f32, width: f32, height: f32) {
    ui.drawer.draw_rect(
        &RectCoords {
            x0: x,
            y0: y,
            x1: width,
            y1: height,
        },
        COLOR_BG,
    );
    let text = "Files";
    ui.drawer.draw_text(
        width / 2.0 - text.len() as f32 * 6.0,
        y + 12.0,
        12,
        text,
        text.len(),
        draw::color::WHITE,
    );

    button(
        ui,
        &RectCoords::from_size(width / 2.0 - 40.0, y + 50.0, 80.0, 20.0),
        Some("Click me!"),
    );
}

pub fn textarea(ui: &UIState, x: f32, y: f32, width: f32, height: f32, content: &str) {
    // xarkes: draw background
    ui.drawer.draw_rect(
        &RectCoords {
            x0: x,
            y0: y,
            x1: width,
            y1: height,
        },
        COLOR_BG,
    );

    // xarkes: iterate lines and draw them
    let mut yoff = 0.0;
    let nchars = width as u32 / (12 / 2);
    for line in content.split('\n') {
        ui.drawer
            .draw_text(x, y + yoff, 12, line, nchars as usize, draw::color::WHITE);
        yoff += 14.0;

        // xarkes: Don't draw not visible lines
        if y + yoff > height {
            break;
        }
    }
}
