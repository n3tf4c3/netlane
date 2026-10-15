use crate::theme::ActiveTheme;
use gpui::{Context, Hsla, IntoElement, Render, SharedString, Window, div, px, svg};
pub fn icon(name: &'static str, size: f32, color: Hsla) -> impl IntoElement {
    let path = match name {
        "add" => "icons/add.svg",
        "check" => "icons/check.svg",
        "close" => "icons/close.svg",
        "delete" => "icons/delete.svg",
        "desktop_windows" => "icons/desktop_windows.svg",
        "lan" => "icons/lan.svg",
        "menu" => "icons/menu.svg",
        "play_arrow" => "icons/play_arrow.svg",
        "save" => "icons/save.svg",
        "folder_open" => "icons/folder_open.svg",
        "code" => "icons/code.svg",
        "send" => "icons/send.svg",
        "remove" => "icons/remove.svg",
        "crop_square" => "icons/crop_square.svg",
        "filter_none" => "icons/filter_none.svg",
        "auto_awesome" => "icons/auto_awesome.svg",
        "visibility" => "icons/visibility.svg",
        "keyboard_double_arrow_left" => "icons/keyboard_double_arrow_left.svg",
        "keyboard_double_arrow_right" => "icons/keyboard_double_arrow_right.svg",
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
/// Compact icon-only button with a hover label, as in the termIAX panels.
pub fn icon_button(
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    tip: impl Into<SharedString>,
    active: bool,
    cx: &gpui::App,
) -> gpui::Stateful<gpui::Div> {
    use gpui::prelude::*;
    let t = cx.theme();
    let tip = tip.into();
    div()
        .id(id)
        .size(px(28.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .cursor_pointer()
        .when(active, |d| d.bg(t.selected))
        .hover(|d| d.bg(t.hover))
        .tooltip(move |_, cx| cx.new(|_| Tip(tip.clone())).into())
        .child(icon(
            glyph,
            16.,
            if active { t.accent } else { t.text_muted },
        ))
}
/// One-line hover label, e.g. for icon-only navigation.
pub struct Tip(pub SharedString);
impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui::prelude::*;
        let t = cx.theme();
        div()
            .px(px(8.))
            .py(px(4.))
            .rounded(px(4.))
            .bg(t.elevated)
            .border_1()
            .border_color(t.border)
            .text_size(px(12.))
            .text_color(t.text)
            .child(self.0.clone())
    }
}
