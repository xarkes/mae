use crate::draw::Drawer;

pub fn textarea(drawer: &Drawer, x: u32, y: u32, winx: u32, winy: u32, content: &str) {
    // xarkes: iterate lines and draw them
    let mut yoff = 0;
    let width = winx - x;
    let nchars = width / (12 / 2);
    for line in content.split('\n') {
        // TODO(xarkes): This sucks due to reallocation
        let mut line = line.to_string();
        line.truncate(nchars as usize);
        drawer.draw_text(x, y + yoff, 12, line.as_str());
        yoff += 14;

        // xarkes: Don't draw not visible lines
        if y + yoff > winy {
            break;
        }
    }
}
