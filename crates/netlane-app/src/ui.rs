use gpui::{Hsla, IntoElement, SharedString, px, svg};
pub fn icon(name: &'static str, size: f32, color: Hsla) -> impl IntoElement {
    let path = match name {
        "add" => "icons/add.svg",
        "check" => "icons/check.svg",
        "close" => "icons/close.svg",
        "delete" => "icons/delete.svg",
        "desktop_windows" => "icons/desktop_windows.svg",
        "lan" => "icons/lan.svg",
        "palette" => "icons/palette.svg",
        "refresh" => "icons/refresh.svg",
        "search" => "icons/search.svg",
        "settings" => "icons/settings.svg",
        "stop" => "icons/stop.svg",
        "power_settings_new" => "icons/power_settings_new.svg",
        "warning" => "icons/warning.svg",
        "tune" => "icons/tune.svg",
        "arrow_downward" => "icons/arrow_downward.svg",
        "arrow_upward" => "icons/arrow_upward.svg",
        "check_box" => "icons/check_box.svg",
        "check_box_outline_blank" => "icons/check_box_outline_blank.svg",
        _ => "icons/info.svg",
    };
    use gpui::prelude::*;
    svg()
        .path(SharedString::new_static(path))
        .size(px(size))
        .flex_none()
        .text_color(color)
}
