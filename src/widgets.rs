use crate::{
    draw::{self, Drawer},
    render::V4f32,
};

const COLOR_BG: V4f32 = V4f32 {
    r: 0.2,
    g: 0.2,
    b: 0.2,
    a: 1.0,
};

pub fn treeview(drawer: &Drawer, x: u32, y: u32, width: u32, height: u32) {
    drawer.draw_rect(x, y, width, height, COLOR_BG);
    drawer.draw_rect(width - 1, y, 1, height, draw::color::WHITE);
    let text = "Files";
    drawer.draw_text(
        width / 2 - text.len() as u32 * 6,
        y + 12,
        12,
        text,
        text.len(),
        draw::color::WHITE,
    );
}

pub fn textarea(drawer: &Drawer, x: u32, y: u32, winx: u32, winy: u32, content: &str) {
    let width = winx - x;
    let height = winy - y;

    // xarkes: draw background
    drawer.draw_rect(x, y, width, height, COLOR_BG);

    // xarkes: iterate lines and draw them
    let mut yoff = 0;
    let nchars = width / (12 / 2);
    for line in content.split('\n') {
        drawer.draw_text(x, y + yoff, 12, line, nchars as usize, draw::color::WHITE);
        yoff += 14;

        // xarkes: Don't draw not visible lines
        if y + yoff > winy {
            break;
        }
    }
}
