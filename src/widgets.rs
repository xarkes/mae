use crate::{
    draw::{self, Drawer},
    render::RectCoords,
    render::V2f32,
    render::V4f32,
};

const COLOR_BG: V4f32 = V4f32 {
    r: 0.2,
    g: 0.2,
    b: 0.2,
    a: 1.0,
};

fn point_in_rect(loc: &RectCoords, point: &V2f32) -> bool {
    return point.x >= loc.x0 && point.x <= loc.x1 && point.y >= loc.y0 && point.y <= loc.y1;
}

pub fn button(drawer: &Drawer, coords: &RectCoords, label: Option<&str>, mouse_pos: &V2f32) {
    let mut bg_color = draw::color::TMP;
    if point_in_rect(coords, mouse_pos) {
        bg_color = draw::color::TMP2;
    }
    drawer.draw_rect(coords, bg_color);
    if let Some(label) = label {
        drawer.draw_text(
            coords.x0,
            coords.y0,
            12,
            label,
            label.len(),
            draw::color::WHITE,
        );
    }
}

pub fn treeview(drawer: &Drawer, x: f32, y: f32, width: f32, height: f32, mouse_pos: &V2f32) {
    drawer.draw_rect(
        &RectCoords {
            x0: x,
            y0: y,
            x1: width,
            y1: height,
        },
        COLOR_BG,
    );
    let text = "Files";
    drawer.draw_text(
        width / 2.0 - text.len() as f32 * 6.0,
        y + 12.0,
        12,
        text,
        text.len(),
        draw::color::WHITE,
    );

    button(
        drawer,
        &RectCoords::from_size(width / 2.0 - 40.0, y + 50.0, 80.0, 20.0),
        Some("Click me!"),
        &mouse_pos,
    );
}

pub fn textarea(drawer: &Drawer, x: f32, y: f32, width: f32, height: f32, content: &str) {
    // xarkes: draw background
    drawer.draw_rect(
        &RectCoords {
            x0: x,
            y0: y,
            x1: width,
            y1: height,
        },
        COLOR_BG,
    );

    // xarkes: iterate lines and draw them
    // let mut yoff = 0;
    // let nchars = width / (12 / 2);
    // for line in content.split('\n') {
    //     drawer.draw_text(x, y + yoff, 12, line, nchars as usize, draw::color::WHITE);
    //     yoff += 14;

    //     // xarkes: Don't draw not visible lines
    //     if y + yoff > winy {
    //         break;
    //     }
    // }
}
