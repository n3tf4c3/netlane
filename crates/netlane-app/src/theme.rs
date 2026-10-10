//! Zed-format color themes mapped to the same design tokens as termIAX.
use anyhow::{Context as _, Result, bail};
use gpui::{App, Global, Hsla, Rgba, SharedString};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
pub const UI_FONT: &str = "Segoe UI";
pub const MONO_FONT: &str = "JetBrains Mono";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Dark,
    Light,
}
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: SharedString,
    pub appearance: Appearance,
    pub chrome: Hsla,
    pub editor: Hsla,
    pub panel: Hsla,
    pub elevated: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub border: Hsla,
    pub divider: Hsla,
    pub text: Hsla,
    pub text_strong: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub text_disabled: Hsla,
    pub text_on_accent: Hsla,
    pub accent: Hsla,
    pub focus: Hsla,
    pub selection: Hsla,
    pub ok: Hsla,
    pub warning: Hsla,
    pub error: Hsla,
}
#[derive(Deserialize)]
struct Family {
    themes: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    name: String,
    appearance: String,
    style: HashMap<String, Value>,
}
fn hex(value: u32) -> Hsla {
    gpui::rgb(value).into()
}
fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.,
    }
    .into()
}
pub fn parse_family(json: &str) -> Result<Vec<Theme>> {
    if json.len() > 512 * 1024 {
        bail!("Tema maior que 512 KiB.");
    }
    let family: Family = serde_json::from_str(json)?;
    if family.themes.is_empty() || family.themes.len() > 64 {
        bail!("Família de temas vazia ou muito grande.");
    }
    family
        .themes
        .iter()
        .map(|entry| {
            if entry.name.trim().is_empty() || entry.name.chars().count() > 100 {
                bail!("Nome de tema inválido.");
            }
            let color = |keys: &[&str]| {
                keys.iter().find_map(|k| {
                    entry
                        .style
                        .get(*k)
                        .and_then(Value::as_str)
                        .and_then(|s| Rgba::try_from(s).ok())
                        .map(Into::into)
                })
            };
            let text = color(&["text"]).context("Tema sem cor de texto.")?;
            let editor =
                color(&["editor.background", "background"]).context("Tema sem cor de fundo.")?;
            let background = color(&["background"]).unwrap_or(editor);
            let panel = color(&["panel.background", "surface.background"]).unwrap_or(background);
            let border = color(&["border"]).unwrap_or_else(|| mix(panel, text, 0.2));
            let accent =
                color(&["text.accent", "icon.accent", "border.focused"]).unwrap_or(hex(0x5AA0F2));
            Ok(Theme {
                name: entry.name.clone().into(),
                appearance: match entry.appearance.as_str() {
                    "dark" => Appearance::Dark,
                    "light" => Appearance::Light,
                    _ => bail!("Aparência de tema inválida."),
                },
                chrome: color(&["title_bar.background", "status_bar.background"])
                    .unwrap_or(background),
                editor,
                panel,
                elevated: color(&["elevated_surface.background", "surface.background"])
                    .unwrap_or(panel),
                hover: color(&["ghost_element.hover", "element.hover"])
                    .unwrap_or_else(|| mix(panel, text, 0.06)),
                selected: color(&["ghost_element.selected", "element.selected"])
                    .unwrap_or_else(|| mix(panel, text, 0.12)),
                border,
                divider: color(&["border.variant"]).unwrap_or(border),
                text,
                text_strong: color(&["termiax.text.strong"]).unwrap_or(text),
                text_muted: color(&["text.muted"]).unwrap_or_else(|| mix(text, panel, 0.35)),
                text_faint: color(&["text.placeholder"]).unwrap_or_else(|| mix(text, panel, 0.55)),
                text_disabled: color(&["text.disabled"]).unwrap_or_else(|| mix(text, panel, 0.55)),
                text_on_accent: color(&["termiax.text.on_accent"]).unwrap_or_else(|| {
                    if accent.l > 0.6 {
                        hex(0x111418)
                    } else {
                        hex(0xFFFFFF)
                    }
                }),
                accent,
                focus: color(&["border.focused"]).unwrap_or(accent),
                selection: accent.opacity(0.35),
                ok: color(&["success"]).unwrap_or(hex(0x69C27E)),
                warning: color(&["warning"]).unwrap_or(hex(0xE9B452)),
                error: color(&["error"]).unwrap_or(hex(0xF47B74)),
            })
        })
        .collect()
}
pub struct Themes {
    pub all: Vec<Arc<Theme>>,
    pub active: Arc<Theme>,
}
impl Global for Themes {}
impl Themes {
    pub fn load_embedded() -> Self {
        let all: Vec<_> = crate::assets::THEMES
            .iter()
            .flat_map(|s| parse_family(s).expect("valid bundled theme"))
            .map(Arc::new)
            .collect();
        let active = all[0].clone();
        Self { all, active }
    }
    pub fn set_active(cx: &mut App, name: &str) {
        if let Some(theme) = cx
            .global::<Self>()
            .all
            .iter()
            .find(|t| t.name == name)
            .cloned()
        {
            cx.global_mut::<Self>().active = theme;
            cx.refresh_windows();
        }
    }
}
pub trait ActiveTheme {
    fn theme(&self) -> &Arc<Theme>;
}
impl ActiveTheme for App {
    fn theme(&self) -> &Arc<Theme> {
        &self.global::<Themes>().active
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_themes_have_dark_light_and_termiax_tokens() {
        let themes = Themes::load_embedded();
        assert_eq!(themes.active.name, "NetLane Dark");
        assert_eq!(themes.active.editor, hex(0x1e1e1e));
        assert_eq!(themes.active.panel, hex(0x212121));
        assert!(themes.all.iter().any(|t| t.name == "One Dark Pro"));
        assert!(themes.all.iter().any(|t| t.appearance == Appearance::Light));
        let names: std::collections::HashSet<_> =
            themes.all.iter().map(|t| t.name.clone()).collect();
        assert_eq!(names.len(), themes.all.len());
    }
    #[test]
    fn imported_data_is_bounded_and_cannot_load_code() {
        assert!(parse_family("{").is_err());
        assert!(parse_family(&" ".repeat(512 * 1024 + 1)).is_err());
        assert!(
            parse_family(
                r#"{"themes":[{"name":"x","appearance":"dark","style":{"text":"C:\\evil.dll"}}]}"#
            )
            .is_err()
        );
    }
}
