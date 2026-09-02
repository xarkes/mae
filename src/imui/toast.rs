//! Transient corner notifications ("toasts").
//!
//! Toasts are framework-managed overlay cards stacked in the top-right corner.
//! A caller raises one imperatively with [`IMUI::toast`] (edge-triggered — call
//! it once per event, not every frame); the framework then renders, animates,
//! auto-expires and lays them out on its own, above all regular content. Each
//! toast carries a severity [`ToastLevel`], a close cross, and a shrinking line
//! at its base that tracks the remaining time before auto-dismiss.

use std::time::Duration;

use super::*;

/// Severity of a [toast](IMUI::toast), selecting its accent colour and icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Warning,
    Danger,
}

/// Default lifetime before a toast auto-dismisses.
const DEFAULT_DURATION: Duration = Duration::from_secs(30);

const TOAST_WIDTH: f32 = 340.0;
const TOAST_MARGIN: f32 = 16.0;
const TOAST_GAP: f32 = 10.0;
const PROGRESS_HEIGHT: f32 = 3.0;

// Material Icons glyphs (see assets/MaterialIcons-Regular.ttf).
const ICON_INFO: &str = "\u{e88e}";
const ICON_WARNING: &str = "\u{e002}";
const ICON_DANGER: &str = "\u{e000}";
const ICON_CLOSE: &str = "\u{e5cd}";

/// One live toast. Retained across frames (unlike UIBoxes) so its lifetime and
/// enter/exit animation survive the immediate-mode rebuild.
pub(super) struct Toast {
    id: u64,
    level: ToastLevel,
    message: String,
    /// `now_seconds()` at creation; drives the shrinking time line.
    created: f64,
    duration: f64,
    /// Set by the close cross (or [`IMUI::dismiss_toast`]); starts the exit.
    dismissed: bool,
    /// 0 → 1 enter / exit progress (drives opacity).
    anim: f32,
}

impl ToastLevel {
    fn icon(self) -> &'static str {
        match self {
            ToastLevel::Info => ICON_INFO,
            ToastLevel::Warning => ICON_WARNING,
            ToastLevel::Danger => ICON_DANGER,
        }
    }

    fn accent(self, theme: &UITheme) -> Color {
        match self {
            ToastLevel::Info => theme.info,
            ToastLevel::Warning => theme.warning,
            ToastLevel::Danger => theme.danger,
        }
    }
}

/// Flattened, alpha-resolved view of a toast for the build pass — lets us drop
/// the borrow on `self.toasts` before touching the box pool.
struct ToastRender {
    id: u64,
    level: ToastLevel,
    message: String,
    alpha: f32,
    /// Remaining-time fraction (1 → 0) for the progress line.
    remaining: f32,
}

impl IMUI {
    /// Raise a toast with the default 30s lifetime. Returns its id (for
    /// [`dismiss_toast`](Self::dismiss_toast)). Call this on an event, not every
    /// frame, or it will spawn a new toast per frame.
    pub fn toast(&mut self, level: ToastLevel, message: impl Into<String>) -> u64 {
        self.toast_with_duration(level, message, DEFAULT_DURATION)
    }

    /// Raise a toast that auto-dismisses after `duration`.
    pub fn toast_with_duration(
        &mut self,
        level: ToastLevel,
        message: impl Into<String>,
        duration: Duration,
    ) -> u64 {
        let id = self.next_toast_id;
        self.next_toast_id += 1;
        self.toasts.push(Toast {
            id,
            level,
            message: message.into(),
            created: self.now_seconds(),
            duration: duration.as_secs_f64(),
            dismissed: false,
            anim: 0.0,
        });
        self.request_repaint();
        id
    }

    /// Begin dismissing a toast early (it plays its exit animation).
    pub fn dismiss_toast(&mut self, id: u64) {
        if let Some(toast) = self.toasts.iter_mut().find(|t| t.id == id) {
            toast.dismissed = true;
            self.request_repaint();
        }
    }

    /// Advance lifetimes/animation, drop finished toasts, and emit the overlay
    /// boxes. Runs at the top of `end_frame`, after the caller's build pass, so
    /// toasts paint above everything else.
    pub(crate) fn render_toasts(&mut self) {
        if self.toasts.is_empty() {
            return;
        }
        let now = self.now_seconds();
        let rate = smooth_rate(self.theme.motion.menu_rate, self.animation_dt);
        let epsilon = self.theme.motion.epsilon;

        // Enter/exit animation toward alive (1) or leaving (0).
        for toast in &mut self.toasts {
            let leaving = toast.dismissed || (now - toast.created) >= toast.duration;
            let target = if leaving { 0.0 } else { 1.0 };
            toast.anim = animate_scalar(toast.anim, target, rate, epsilon);
        }
        // Drop toasts that have fully faded out.
        self.toasts.retain(|toast| {
            let leaving = toast.dismissed || (now - toast.created) >= toast.duration;
            !(leaving && toast.anim <= 0.02)
        });
        if self.toasts.is_empty() {
            return;
        }

        // Snapshot for the build pass (drops the borrow on `self.toasts`).
        let renders: Vec<ToastRender> = self
            .toasts
            .iter()
            .map(|toast| ToastRender {
                id: toast.id,
                level: toast.level,
                message: toast.message.clone(),
                alpha: toast.anim.clamp(0.0, 1.0),
                remaining: (1.0 - (now - toast.created) as f32 / toast.duration as f32)
                    .clamp(0.0, 1.0),
            })
            .collect();

        // One floating column pinned to the top-right corner stacks the cards;
        // the layout engine handles their (wrapped, variable) heights and gaps.
        // Narrower than the card's natural width when the window is
        // narrower still — otherwise a 340px toast on a 390px phone sits
        // 34px from the left edge and reads as a full-width banner, and on
        // anything narrower it runs off the screen entirely.
        let width = TOAST_WIDTH
            .min(self.size.width - TOAST_MARGIN * 2.0)
            .max(1.0);
        let x = self.size.width - width - TOAST_MARGIN;
        self.parent_stack.push(self.overlay_root);
        let stack = self.alloc_box(Some("###toast_stack"), UIBoxFlags::NONE);
        {
            let b = &mut self.boxes[stack.idx()];
            b.flags |= UIBoxFlags::FLOATING_X | UIBoxFlags::FLOATING_Y;
            b.fixed_position = Point::new(x, TOAST_MARGIN);
            b.child_layout_axis = Axis::Y;
            b.pref_size = [UISize::Pixels(width), UISize::ChildrenSum];
            b.child_gap = TOAST_GAP;
        }
        self.parent_stack.push(stack.idx());
        let mut dismiss: Vec<u64> = Vec::new();
        for render in &renders {
            if self.build_toast(render) {
                dismiss.push(render.id);
            }
        }
        self.parent_stack.pop();
        self.parent_stack.pop();

        for id in dismiss {
            self.dismiss_toast(id);
        }

        // Keep frames coming so the timer advances and toasts expire while idle.
        self.request_repaint();
    }

    /// Build one toast card (a child of the stack column). Returns `true` if its
    /// close cross was clicked this frame.
    fn build_toast(&mut self, r: &ToastRender) -> bool {
        let theme = &self.theme;
        let accent = color_mul_alpha(r.level.accent(theme), r.alpha);
        let bg = color_mul_alpha(theme.popover_bg, r.alpha);
        let border = color_mul_alpha(theme.border, r.alpha);
        let text = color_mul_alpha(theme.text, r.alpha);
        let muted = color_mul_alpha(theme.text_muted, r.alpha);
        let track = color_mul_alpha(r.level.accent(theme), r.alpha * 0.18);
        let radius = theme.radius + 2.0;
        let icon = r.level.icon();
        let message = r.message.clone();
        let remaining = r.remaining;

        let mut close_clicked = false;
        let pane = self.container(
            Some(&format!("###toast_{}", r.id)),
            Axis::Y,
            UIBoxFlags::DRAW_BACKGROUND | UIBoxFlags::DRAW_BORDER | UIBoxFlags::CLIP,
            |ui| {
                // Content: icon, wrapping message (grows), close cross.
                let row = ui.container(
                    Some(&format!("###toast_row_{}", r.id)),
                    Axis::X,
                    UIBoxFlags::NONE,
                    |ui| {
                        let glyph = ui.icon_label(icon);
                        ui.font_size(glyph, 22.0);
                        ui.text_color(glyph, accent);

                        let label = ui.wrapping_label(&message);
                        ui.width(label, UISize::Fill);
                        ui.text_color(label, text);

                        let close = ui.alloc_box(
                            Some(&format!("###toast_close_{}", r.id)),
                            UIBoxFlags::CLICKABLE | UIBoxFlags::DRAW_TEXT,
                        );
                        ui.set_display_string(close.idx(), ICON_CLOSE.to_string());
                        ui.boxes[close.idx()].style.font_icon = true;
                        ui.font_size(close, 18.0);
                        ui.text_center(close, true);
                        ui.text_color(close, if close.hover() { text } else { muted });
                        ui.width(close, UISize::Pixels(22.0));
                        ui.height(close, UISize::Pixels(22.0));
                        ui.cursor(close, OSCursor::Hand);
                        close_clicked = close.clicked();
                    },
                );
                ui.width(row, UISize::Fill);
                ui.height(row, UISize::ChildrenSum);
                ui.padding(row, 12.0, 10.0, 12.0, 14.0);
                ui.gap(row, 10.0);
                ui.align(row, MainAxisAlign::Start, CrossAxisAlign::Center);

                // Shrinking time line pinned to the base.
                let progtrack = ui.container(
                    Some(&format!("###toast_track_{}", r.id)),
                    Axis::X,
                    UIBoxFlags::DRAW_BACKGROUND,
                    |ui| {
                        let fill = ui.alloc_box(
                            Some(&format!("###toast_fill_{}", r.id)),
                            UIBoxFlags::DRAW_BACKGROUND,
                        );
                        ui.width(fill, UISize::ParentPct(remaining));
                        ui.height(fill, UISize::Pixels(PROGRESS_HEIGHT));
                        ui.background(fill, accent);
                    },
                );
                ui.width(progtrack, UISize::ParentPct(1.0));
                ui.height(progtrack, UISize::Pixels(PROGRESS_HEIGHT));
                ui.background(progtrack, track);
            },
        );

        let b = &mut self.boxes[pane.idx()];
        b.pref_size = [UISize::Fill, UISize::ChildrenSum];
        b.style.bg_color = bg;
        b.style.border_color = border;
        b.style.corner_radius = radius;

        close_clicked
    }
}
