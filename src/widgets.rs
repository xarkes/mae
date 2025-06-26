// use crate::{
//     UIState, draw,
//     render::{RectCoords, V4f32},
// };

// const COLOR_BG: V4f32 = V4f32 {
//     r: 0.2,
//     g: 0.2,
//     b: 0.2,
//     a: 1.0,
// };
// const COLOR_BG2: V4f32 = V4f32 {
//     r: 0.3,
//     g: 0.3,
//     b: 0.3,
//     a: 1.0,
// };

// pub fn button(ui: &UIState, coords: &RectCoords, label: Option<&str>) {
// let mut bg_color = draw::color::TMP;
// if ui.hover(coords) {
//     bg_color = draw::color::TMP2;
// }
// ui.drawer.draw_rect(coords, bg_color);
// if let Some(label) = label {
//     ui.drawer.draw_text(
//         coords.x0,
//         coords.y0,
//         12,
//         label,
//         label.len(),
//         draw::color::WHITE,
//     );
// }
// }

// pub fn label(ui: &UIState, x: f32, y: f32, label: &str) {
//     ui.drawer.draw_rect(
//         &RectCoords::from_size(x, y, label.len() as f32 * 6.0, 12.0),
//         COLOR_BG,
//     );
//     ui.drawer
//         .draw_text(x, y, 12, label, label.len(), draw::color::WHITE);
// }

// pub fn treeview(ui: &UIState, x: f32, y: f32, width: f32, height: f32) {
//     // xarkes: draw background
//     let coords = RectCoords::from_size(x, y, width, height);
//     let mut color = COLOR_BG;
//     if ui.hover(&coords) {
//         color = COLOR_BG2;
//     }
//     ui.drawer.draw_rect(&coords, color);

//     // xarkes: draw title
//     let text = "Files";
//     ui.drawer.draw_text(
//         width / 2.0 - text.len() as f32 * 6.0,
//         y + 12.0,
//         12,
//         text,
//         text.len(),
//         draw::color::WHITE,
//     );

//     // xarkes: draw file names
//     let mut cury = y + 24.0;
//     let fnames = ["note.md", "writing_a_gui.md", "rust_tips.md", "whatever.md"];
//     for fname in fnames {
//         let text_rect = RectCoords::from_size(x, cury, fname.len() as f32 * 12.0, 12.0);
//         if ui.hover(&text_rect) {
//             ui.drawer.draw_rect(&text_rect, COLOR_BG);
//         }
//         ui.drawer
//             .draw_text(x, cury, 12, fname, fname.len(), draw::color::WHITE);
//         cury += 14.0;
//     }

//     // xarkes: draw button at the bottom
//     button(
//         ui,
//         &RectCoords::from_size(width / 2.0 - 40.0, coords.y1 - 20.0 - 20.0, 80.0, 20.0),
//         Some("Click me!"),
//     );
// }

// pub fn textarea(ui: &UIState, x: f32, y: f32, width: f32, height: f32, content: &str) {
//     // xarkes: draw background
//     let coords = RectCoords::from_size(x, y, width, height);
//     let mut color = COLOR_BG;
//     if ui.hover(&coords) {
//         color = COLOR_BG2;
//     }
//     ui.drawer.draw_rect(&coords, color);

//     // xarkes: iterate lines and draw them
//     let font_size = 12;
//     let line_height = 14.0;
//     let mut yoff = 0.0;
//     let nchars = width as u32 / (font_size / 2);
//     for line in content.split('\n') {
//         ui.drawer.draw_text(
//             x,
//             y + yoff,
//             font_size,
//             line,
//             nchars as usize,
//             draw::color::WHITE,
//         );
//         yoff += line_height;

//         // xarkes: Don't draw not visible lines
//         if y + yoff > height {
//             break;
//         }
//     }

//     // xarkes: draw cursor
//     let font_size = font_size as f32;
//     let cursor_pos = (400., 410.);
//     let cursor_pos = (
//         (((cursor_pos.0 - x) / font_size) as u32) as f32,
//         (((cursor_pos.1 - y) / line_height) as u32) as f32,
//     );
//     if cursor_pos.0 >= 0.0 && cursor_pos.1 >= 0.0 {
//         ui.drawer.draw_rect(
//             &RectCoords {
//                 x0: x + cursor_pos.0 * font_size - 1.0,
//                 y0: y + cursor_pos.1 * line_height,
//                 x1: x + cursor_pos.0 * font_size,
//                 y1: y + cursor_pos.1 * line_height + font_size,
//             },
//             draw::color::WHITE,
//         )
//     }
// }
