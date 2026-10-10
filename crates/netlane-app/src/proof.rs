//! Opt-in rendering of the actual GPUI application, with synthetic data only.
use crate::{
    settings::AppSettings,
    theme::{ActiveTheme, Themes},
    workspace::{Page, Workspace},
};
use gpui::{App, WindowHandle, px, size};
use netlane_windows::desktop::{ProofPanelCommand, TrayAction};
use std::{path::PathBuf, time::Duration};
pub fn start(window: WindowHandle<Workspace>, directory: PathBuf, cx: &mut App) {
    std::fs::create_dir_all(&directory).expect("proof output directory");
    window.update(cx,|_,w,cx|{
        cx.spawn_in(w,async move|this,cx|{
            this.update(cx,|ws,cx|ws.proof_prepare_icons(cx)).unwrap();
            this.update(cx,|ws,_|{assert!(ws.proof_desktop().available());assert!(!ws.proof_desktop().visible());ws.proof_desktop().proof_post_action(TrayAction::Open);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|assert!(ws.proof_desktop().visible())).unwrap();
            this.update(cx,|ws,cx|{ws.page=Page::Rules;cx.notify();}).unwrap();
            for _ in 0..40 {
                if this.update(cx,|ws,_|ws.proof_icons_ready()).unwrap() { break; }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
            this.update(cx,|ws,_|assert!(ws.proof_icons_ready())).unwrap();
            for (name,page,theme) in [
                ("01-overview-dark.png",Page::Overview,"NetLane Dark"),
                ("02-connections-dark.png",Page::Connections,"NetLane Dark"),
                ("03-rules-dark.png",Page::Rules,"NetLane Dark"),
                ("04-settings-dark.png",Page::Settings,"NetLane Dark"),
                ("05-overview-light.png",Page::Overview,"One Light"),
                ("06-connections-light.png",Page::Connections,"One Light"),
                ("07-settings-light.png",Page::Settings,"One Light"),
                ("08-diagnostics-dark.png",Page::Diagnostics,"One Dark Pro"),
            ]{
                this.update(cx,|ws,cx|{ws.page=page;Themes::set_active(cx,theme);cx.notify();}).unwrap();
                cx.background_executor().timer(Duration::from_millis(450)).await;
                cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join(name)).unwrap();}).unwrap();
            }
            // Changing a row to a remote path immediately removes its old image, without I/O.
            this.update(cx,|ws,cx|{ws.page=Page::Rules;ws.proof_icon_remote_fallback(cx);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("13-rules-icon-fallback.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|{ws.proof_restore_icon_fixture(cx);ws.page=Page::Connections;cx.notify();}).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            this.update(cx,|ws,_|assert!(ws.proof_icons_ready())).unwrap();
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("14-connections-icons.png")).unwrap();}).unwrap();
            this.update_in(cx,|ws,w,cx|{ws.page=Page::Settings;ws.proof_open_theme_picker(w,cx);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("09-theme-picker.png")).unwrap();}).unwrap();
            // Exercise real picker filtering, preview, cancellation, confirmation and persistence.
            this.update(cx,|ws,cx|ws.proof_theme_filter("Ayu",cx)).unwrap();
            cx.background_executor().timer(Duration::from_millis(250)).await;
            this.update(cx,|ws,cx|{assert!(cx.theme().name.starts_with("Ayu"));ws.proof_theme_cancel(cx);assert_eq!(cx.theme().name,"One Dark Pro");}).unwrap();
            this.update_in(cx,|ws,w,cx|ws.proof_open_theme_picker(w,cx)).unwrap();
            this.update(cx,|ws,cx|ws.proof_theme_filter("One Light",cx)).unwrap();
            cx.background_executor().timer(Duration::from_millis(250)).await;
            this.update(cx,|ws,cx|{ws.proof_theme_confirm(cx);assert_eq!(AppSettings::get(cx).theme.as_deref(),Some("One Light"));assert_eq!(cx.theme().name,"One Light");assert_eq!(ws.proof_saved_theme().as_deref(),Some("One Light"));}).unwrap();
            this.update(cx,|ws,cx|{ws.proof_edit_rule(cx);assert!(ws.dirty);ws.proof_save(cx);assert!(!ws.dirty);}).unwrap();
            cx.update(|w,_|w.resize(size(px(1000.),px(640.)))).unwrap();
            for (name,page,font,compact,empty) in [
                ("10-settings-large-text.png",Page::Settings,20,false,false),
                ("11-connections-compact.png",Page::Connections,13,true,false),
                ("12-connections-empty.png",Page::Connections,13,false,true),
            ] {
                this.update(cx,|ws,cx|{ws.page=page;ws.proof_appearance(font,compact,cx);if empty {ws.snapshot.connections.clear();}cx.notify();}).unwrap();
                cx.background_executor().timer(Duration::from_millis(450)).await;
                cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join(name)).unwrap();}).unwrap();
            }
            // Real HWND and tray callback tests, with an in-memory session fixture only.
            let same_rule = this.update(cx,|ws,cx|{
                assert!(ws.proof_desktop().available());
                ws.proof_edit_rule(cx);
                ws.proof_desktop().proof_post_command(ProofPanelCommand::Close);
                ws.policies[0].application_id.clone()
            }).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(!ws.proof_desktop().visible());assert!(ws.dirty);ws.proof_desktop().proof_post_action(TrayAction::Open);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(ws.proof_desktop().visible());assert_eq!(ws.policies[0].application_id,same_rule);assert!(ws.dirty);ws.proof_desktop().proof_post_command(ProofPanelCommand::Minimize);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(!ws.proof_desktop().visible());ws.proof_desktop().proof_post_action(TrayAction::Open);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(ws.proof_desktop().visible());ws.proof_desktop().proof_post_command(ProofPanelCommand::Maximize);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(ws.proof_desktop().proof_maximized());ws.proof_desktop().proof_post_command(ProofPanelCommand::Minimize);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(!ws.proof_desktop().visible());ws.proof_desktop().proof_post_action(TrayAction::Open);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(ws.proof_desktop().visible());assert!(ws.proof_desktop().proof_maximized());ws.proof_desktop().proof_post_action(TrayAction::Exit);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(300)).await;
            this.update(cx,|ws,cx|{assert!(ws.proof_exit_confirmation());assert!(ws.dirty);ws.proof_cancel_exit(cx);ws.proof_save(cx);ws.proof_desktop().proof_recreate_icon();}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update_in(cx,|ws,w,cx|{
                assert!(ws.proof_desktop().available());
                ws.proof_set_session(netlane_agent::SessionState{running:true,cleanup_confirmed:false,..Default::default()});
                ws.request_close(w,cx);
                assert!(ws.proof_exit_pending());
                ws.proof_edit_rule(cx); // New edit while the simulated Stop is pending.
            }).unwrap();
            cx.background_executor().timer(Duration::from_millis(200)).await;
            this.update(cx,|ws,_|ws.proof_set_session(netlane_agent::SessionState{cleanup_confirmed:true,..Default::default()})).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,cx|{assert!(!ws.proof_exit_pending());assert!(ws.proof_exit_confirmation());assert!(ws.dirty);ws.proof_cancel_exit(cx);ws.proof_save(cx);ws.proof_set_session(netlane_agent::SessionState::default());}).unwrap();
            this.update_in(cx,|ws,w,cx|ws.request_close(w,cx)).unwrap();
            cx.background_executor().timer(Duration::from_millis(300)).await;
            this.update(cx,|ws,_|{assert_eq!(ws.page as usize,Page::Diagnostics as usize);assert!(ws.proof_desktop().visible());ws.proof_set_session(netlane_agent::SessionState{busy:true,cleanup_confirmed:true,..Default::default()});ws.proof_desktop().proof_post_command(ProofPanelCommand::Minimize);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(ws.proof_desktop().visible());ws.proof_set_session(netlane_agent::SessionState{cleanup_confirmed:true,..Default::default()});ws.proof_desktop().proof_post_command(ProofPanelCommand::Close);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(!ws.proof_desktop().visible());ws.proof_desktop_failure();}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,cx|{assert!(ws.proof_desktop().visible());assert!(!ws.proof_desktop().available());ws.proof_edit_rule(cx);ws.proof_desktop().proof_post_command(ProofPanelCommand::Close);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(300)).await;
            this.update(cx,|ws,cx|{assert!(ws.proof_exit_confirmation());ws.proof_discard_failure(cx);assert!(ws.proof_exit_confirmation());ws.proof_cancel_exit(cx);ws.proof_save(cx);}).unwrap();
            std::fs::write(directory.join("verified.txt"),"GPUI rendering: 14 scenes\nTheme preview, cancellation, confirmation and disk persistence passed.\nDemo rule edit/save passed.\nNative own-executable icon shared across Rules/Connections; missing and remote paths use fallback.\nLarge text, compact layout and empty state captured at minimum window size.\nStartup path starts hidden and restores the same panel through its own tray callback.\nNative X/minimize/tray restore preserve the same rule and unsaved edits.\nMaximized restore, Exit cancellation, own-icon recreation, failure fallback passed.\nNew edits during simulated cleanup are reconfirmed; missing cleanup receipt blocks Exit.\nBusy session keeps minimized owner visible. Final X without tray exits safely.\nNo network, WFP session, UAC, real autorun or installed profile accessed.\n").unwrap();
            cx.background_executor().timer(Duration::from_millis(250)).await;
            this.update(cx,|ws,_|ws.proof_desktop().proof_post_command(ProofPanelCommand::Close)).unwrap();
        }).detach();
    }).unwrap();
}
