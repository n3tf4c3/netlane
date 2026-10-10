#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod assets;
mod codex_view;
mod collection_view;
mod demo;
mod desktop;
mod executable_icons;
mod lifecycle;
mod packet_view;
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
    let startup = args.len() == 1 && args[0].eq_ignore_ascii_case("--startup");
    anyhow::ensure!(
        !args.iter().any(|a| a.eq_ignore_ascii_case("--startup")) || startup,
        "--startup deve ser usado sozinho."
    );
    let _presence = netlane_windows::startup::StartupPresence::current()?;
    if startup && _presence.already_running {
        return Ok(());
    }
    let default_profile = !args.iter().any(|a| a == "--profile");
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
    #[cfg(feature = "gui-test")]
    let start_in_tray = startup || proof.is_some();
    #[cfg(feature = "gui-test")]
    let packet_proof = args
        .iter()
        .position(|a| a == "--packet-proof")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    #[cfg(feature = "gui-test")]
    if packet_proof.is_some() {
        anyhow::ensure!(
            demo && proof.is_none(),
            "A prova de pacotes exige --demo e deve ser executada separadamente."
        );
    }
    #[cfg(not(feature = "gui-test"))]
    let start_in_tray = startup;
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
                    focus: !start_in_tray,
                    show: !start_in_tray,
                    ..Default::default()
                },
                move |w, cx| cx.new(|cx| Workspace::new(profile, demo, default_profile, w, cx)),
            );
            match result {
                Ok(window) => {
                    let _ = window.update(cx, |workspace, w, cx| {
                        let weak = cx.entity().downgrade();
                        w.on_window_should_close(cx, move |w, cx| {
                            let _ = weak.update(cx, |ws, cx| ws.request_hide(w, cx));
                            false
                        });
                        workspace.install_desktop(start_in_tray, w, cx);
                    });
                    #[cfg(feature = "gui-test")]
                    if let Some(directory) = proof {
                        proof::start(window, directory, cx);
                    }
                    #[cfg(feature = "gui-test")]
                    if let Some(directory) = packet_proof {
                        proof::packets(window, directory, cx);
                    }
                    if !start_in_tray {
                        cx.activate(true);
                    }
                }
                Err(e) => {
                    eprintln!("Janela: {e:#}");
                    cx.quit();
                }
            }
        });
    Ok(())
}
