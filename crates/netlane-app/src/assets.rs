use anyhow::Result;
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;
pub struct Assets;
static ICONS: &[(&str, &[u8])] = &[
    (
        "icons/add.svg",
        include_bytes!("../../../assets/icons/add.svg"),
    ),
    (
        "icons/check.svg",
        include_bytes!("../../../assets/icons/check.svg"),
    ),
    (
        "icons/close.svg",
        include_bytes!("../../../assets/icons/close.svg"),
    ),
    (
        "icons/delete.svg",
        include_bytes!("../../../assets/icons/delete.svg"),
    ),
    (
        "icons/desktop_windows.svg",
        include_bytes!("../../../assets/icons/desktop_windows.svg"),
    ),
    (
        "icons/info.svg",
        include_bytes!("../../../assets/icons/info.svg"),
    ),
    (
        "icons/lan.svg",
        include_bytes!("../../../assets/icons/lan.svg"),
    ),
    (
        "icons/palette.svg",
        include_bytes!("../../../assets/icons/palette.svg"),
    ),
    (
        "icons/refresh.svg",
        include_bytes!("../../../assets/icons/refresh.svg"),
    ),
    (
        "icons/search.svg",
        include_bytes!("../../../assets/icons/search.svg"),
    ),
    (
        "icons/settings.svg",
        include_bytes!("../../../assets/icons/settings.svg"),
    ),
    (
        "icons/stop.svg",
        include_bytes!("../../../assets/icons/stop.svg"),
    ),
    (
        "icons/power_settings_new.svg",
        include_bytes!("../../../assets/icons/power_settings_new.svg"),
    ),
    (
        "icons/warning.svg",
        include_bytes!("../../../assets/icons/warning.svg"),
    ),
    (
        "icons/tune.svg",
        include_bytes!("../../../assets/icons/tune.svg"),
    ),
    (
        "icons/arrow_downward.svg",
        include_bytes!("../../../assets/icons/arrow_downward.svg"),
    ),
    (
        "icons/arrow_upward.svg",
        include_bytes!("../../../assets/icons/arrow_upward.svg"),
    ),
    (
        "icons/check_box.svg",
        include_bytes!("../../../assets/icons/check_box.svg"),
    ),
    (
        "icons/check_box_outline_blank.svg",
        include_bytes!("../../../assets/icons/check_box_outline_blank.svg"),
    ),
];
pub static THEMES: &[&str] = &[
    include_str!("../../../assets/themes/netlane-dark.json"),
    include_str!("../../../assets/themes/one/one.json"),
    include_str!("../../../assets/themes/one-dark-pro/one-dark-pro.json"),
    include_str!("../../../assets/themes/ayu/ayu.json"),
    include_str!("../../../assets/themes/gruvbox/gruvbox.json"),
];
pub static FONT: &[u8] =
    include_bytes!("../../../assets/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf");
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, data)| Cow::Borrowed(*data)))
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::new_static(name))
            .collect())
    }
}
