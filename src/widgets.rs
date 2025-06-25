use crate::draw::Drawer;

pub fn textarea(drawer: &Drawer, x: u32, y: u32, winx: u32, winy: u32, content: &str) {
    let width = winx - x;
    let height = winy - y;

    // xarkes: draw background
    drawer.draw_rect(x, y, width, height);

    // xarkes: iterate lines and draw them
    let mut yoff = 0;
    let nchars = width / (12 / 2);
    for line in content.split('\n') {
        drawer.draw_text(x, y + yoff, 12, line, nchars as usize);
        yoff += 14;

        // xarkes: Don't draw not visible lines
        if y + yoff > winy {
            break;
        }
    }
}
