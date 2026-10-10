#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod assets;
mod demo;
#[cfg(feature = "gui-test")]
mod proof;
mod settings;
mod text_input;
mod theme;
mod theme_picker;
mod ui;
mod workspace;
use crate::{settings::AppSettings, theme::Themes, workspace::Workspace};
use gpui::{App, Bounds, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size};
use std::{borrow::Cow, path::PathBuf};
fn main() {
    if let Err(e) = run() {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}
fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let demo = args.iter().any(|a| a == "--demo");
    let profile = args
        .iter()
        .position(|a| a == "--profile")
        .map(|i| {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("--profile exige uma pasta."))
        })
        .transpose()?
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if demo {
                std::env::temp_dir().join("NetLane-Rust-Demo")
            } else {
                std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir)
                    .join("NetLane-Rust")
            }
        });
    anyhow::ensure!(
        profile.is_absolute(),
        "A pasta de perfil deve ser absoluta."
    );
    let settings = AppSettings::load(profile.join("appearance.json"))?;
    #[cfg(feature = "gui-test")]
    let proof = args
        .iter()
        .position(|a| a == "--gui-proof")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    #[cfg(feature = "gui-test")]
    if proof.is_some() {
        anyhow::ensure!(demo, "A prova visual exige --demo.");
    }
    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            if let Err(e) = cx
                .text_system()
                .add_fonts(vec![Cow::Borrowed(assets::FONT)])
            {
                eprintln!("Fonte: {e:#}");
            }
            cx.set_global(Themes::load_embedded());
            cx.set_global(settings);
            if let Some(name) = AppSettings::get(cx).theme.clone() {
                Themes::set_active(cx, &name);
            }
            text_input::bind_keys(cx);
            theme_picker::bind_keys(cx);
            workspace::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1320.), px(820.)), cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("NetLane".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(1000.), px(640.))),
                    ..Default::default()
                },
                move |w, cx| cx.new(|cx| Workspace::new(profile, demo, w, cx)),
            );
            match result {
                Ok(window) => {
                    let _ = window.update(cx, |workspace, w, cx| {
                        let weak = cx.entity().downgrade();
                        w.on_window_should_close(cx, move |w, cx| {
                            let _ = weak.update(cx, |ws, cx| ws.request_close(w, cx));
                            false
                        });
                        let _ = workspace;
                    });
                    #[cfg(feature = "gui-test")]
                    if let Some(directory) = proof {
                        proof::start(window, directory, cx);
                    }
                    cx.activate(true);
                }
                Err(e) => {
                    eprintln!("Janela: {e:#}");
                    cx.quit();
                }
            }
        });
    Ok(())
}
