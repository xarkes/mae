use mae::imui::{IMUI, UIBoxHandle, UISize};

pub fn app_root(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle
        .width(ui, UISize::ParentPct(1.0))
        .height(ui, UISize::ParentPct(1.0))
        .padding_all(ui, theme.pad_md)
        .gap(ui, theme.gap_md)
        .background(ui, theme.app_bg)
}

pub fn content(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle
        .width(ui, UISize::Fill)
        .height(ui, UISize::ParentPct(1.0))
        .padding_all(ui, theme.pad_lg)
        .gap(ui, theme.gap_md)
        .background(ui, theme.panel_bg)
        .border_color(ui, theme.border_muted)
        .corner_radius(ui, theme.radius)
}

pub fn panel(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle
        .padding_all(ui, theme.pad_lg)
        .gap(ui, theme.gap_md)
        .background(ui, theme.surface_bg)
        .border_color(ui, theme.border)
        .corner_radius(ui, theme.radius)
}

pub fn panel_alt(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle
        .padding_all(ui, theme.pad_lg)
        .gap(ui, theme.gap_md)
        .background(ui, theme.surface_active)
        .border_color(ui, theme.border)
        .corner_radius(ui, theme.radius)
}

pub fn toolbar(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle
        .height(ui, UISize::Pixels(theme.toolbar_h))
        .gap(ui, theme.gap_md)
}

pub fn title(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle
        .text_color(ui, theme.text)
        .height(ui, UISize::Pixels(theme.control_h))
}

pub fn muted(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle.text_color(ui, theme.text_muted)
}

pub fn accent_text(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle.text_color(ui, theme.text_accent)
}

pub fn button(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle
        .height(ui, UISize::Pixels(theme.control_h))
        .corner_radius(ui, theme.radius)
        .background(ui, theme.surface_bg)
        .border_color(ui, theme.border)
}

pub fn toggle(ui: &mut IMUI, handle: UIBoxHandle, enabled: bool) -> UIBoxHandle {
    let theme = *ui.theme();
    button(ui, handle).background(
        ui,
        if enabled {
            theme.accent
        } else {
            theme.surface_bg
        },
    )
}

pub fn popover(ui: &mut IMUI, handle: UIBoxHandle) -> UIBoxHandle {
    let theme = *ui.theme();
    handle
        .padding_all(ui, theme.pad_sm)
        .gap(ui, theme.gap_sm)
        .background(ui, theme.popover_bg)
        .border_color(ui, theme.border)
        .corner_radius(ui, theme.radius)
}
