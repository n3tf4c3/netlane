use anyhow::Result;
use gpui::{App, Global};
use netlane_core::persistence::{atomic_write, read_bounded, without_bom};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Option<String>,
    pub font_size: u8,
    pub compact: bool,
    pub sidebar_width: u16,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: None,
            font_size: 13,
            compact: true,
            sidebar_width: 200,
        }
    }
}
pub struct AppSettings {
    pub value: Settings,
    path: PathBuf,
}
impl Global for AppSettings {}
impl AppSettings {
    pub fn load(path: PathBuf) -> Result<Self> {
        let mut value: Settings = match read_bounded(&path, 4096)? {
            Some(data) => serde_json::from_slice(without_bom(&data))?,
            None => Settings::default(),
        };
        value.font_size = value.font_size.clamp(10, 20);
        value.sidebar_width = value.sidebar_width.clamp(180, 340);
        Ok(Self { value, path })
    }
    pub fn get(cx: &App) -> &Settings {
        &cx.global::<Self>().value
    }
    pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) -> Result<()> {
        let old = cx.global::<Self>();
        let mut value = old.value.clone();
        change(&mut value);
        value.font_size = value.font_size.clamp(10, 20);
        value.sidebar_width = value.sidebar_width.clamp(180, 340);
        atomic_write(&old.path, &serde_json::to_vec_pretty(&value)?, true)?;
        cx.global_mut::<Self>().value = value;
        cx.refresh_windows();
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_do_not_touch_legacy_data_and_bound_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appearance.json");
        let settings = AppSettings::load(path.clone()).unwrap();
        assert!(!path.exists());
        assert_eq!(settings.value.font_size, 13);
        assert!(settings.value.compact);
        assert_eq!(settings.value.sidebar_width, 200);
        std::fs::write(
            &path,
            r#"{"font_size":255,"sidebar_width":1,"theme":"One Light"}"#,
        )
        .unwrap();
        let settings = AppSettings::load(path).unwrap();
        assert_eq!(settings.value.font_size, 20);
        assert_eq!(settings.value.sidebar_width, 180);
    }
}
