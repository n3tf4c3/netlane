//! Opt-in rendering of the actual GPUI application, with synthetic data only.
use crate::{
    settings::AppSettings,
    theme::{ActiveTheme, Themes},
    workspace::{Page, Workspace},
};
use gpui::{App, WindowHandle, px, size};
use netlane_windows::desktop::{ProofPanelCommand, TrayAction};
use std::{path::PathBuf, time::Duration};
/// Only synthetic packets and explicit themes; no installed data or network.
pub fn packets(window: WindowHandle<Workspace>, directory: PathBuf, cx: &mut App) {
    std::fs::create_dir_all(&directory).expect("packet proof output");
    window.update(cx, |_, w, cx| {
        cx.spawn_in(w, async move |this, cx| {
            for (name, theme, query, select, width, height) in [
                ("01-packets-dark.png", "NetLane Dark", "UDP", true, 1320., 920.),
                ("02-packets-light-filter.png", "One Light", "50002", true, 1320., 920.),
                ("03-packets-minimum-empty.png", "NetLane Dark", "missing", false, 1000., 640.),
                ("06-packets-dark-details.png", "NetLane Dark", "50000", true, 1320., 920.),
            ] {
                this.update(cx, |ws, cx| {
                    Themes::set_active(cx, theme); ws.proof_packets(query, select, cx);
                    assert_eq!(ws.proof_packet_matches(cx), if query == "UDP" {6} else if query.starts_with("500") {1} else {0});
                }).unwrap();
                cx.update(|w, _| w.resize(size(px(width), px(height)))).unwrap();
                cx.background_executor().timer(Duration::from_millis(550)).await;
                cx.update(|w, cx| {w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join(name)).unwrap();}).unwrap();
            }
            for (name, theme) in [("04-brand-dark.png","NetLane Dark"),("05-brand-light.png","One Light")] {
                this.update(cx, |ws, cx| { ws.page=Page::Settings;Themes::set_active(cx,theme);cx.notify(); }).unwrap();
                cx.update(|w, _| w.resize(size(px(1320.),px(920.)))).unwrap();
                cx.background_executor().timer(Duration::from_millis(550)).await;
                cx.update(|w, cx| {w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join(name)).unwrap();}).unwrap();
            }
            for (name,theme,width,height) in [("07-codex-context-dark.png","NetLane Dark",1320.,920.),("08-codex-context-light.png","One Light",1320.,920.),("09-codex-minimum.png","NetLane Dark",1000.,640.)] {
                this.update(cx,|ws,cx| {Themes::set_active(cx,theme);ws.proof_codex(cx);}).unwrap();
                cx.update(|w,_|w.resize(size(px(width),px(height)))).unwrap();
                cx.background_executor().timer(Duration::from_millis(550)).await;
                this.update(cx,|ws,cx|assert!(ws.proof_codex_context(cx))).unwrap();
                cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join(name)).unwrap();}).unwrap();
            }
            for (name,theme) in [("10-long-samples-dark.png","NetLane Dark"),("11-baseline-light.png","One Light")] {
                this.update(cx,|ws,cx|{Themes::set_active(cx,theme);ws.proof_long_dump(cx);assert!(ws.proof_long_valid(cx));}).unwrap();
                cx.update(|w,_|w.resize(size(px(1320.),px(920.)))).unwrap();
                cx.background_executor().timer(Duration::from_millis(550)).await;
                cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join(name)).unwrap();}).unwrap();
            }
            this.update(cx,|ws,cx|{Themes::set_active(cx,"NetLane Dark");ws.proof_analyze_dump(cx);assert!(ws.proof_analyzed_valid(cx));}).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("12-dump-analysis-codex.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|{ws.proof_collection_start(cx);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(120)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("13-rotating-synthetic-running.png")).unwrap();}).unwrap();
            for _ in 0..40 {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if this.update(cx,|ws,cx|ws.proof_collection_valid(cx)).unwrap(){break;}
            }
            this.update(cx,|ws,cx|assert!(ws.proof_collection_valid(cx))).unwrap();
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("14-rotating-synthetic-complete.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|{ws.proof_packets("UDP",true,cx);ws.proof_exports(&directory.join("exports"),cx);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("15-derived-export.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|{ws.proof_long_dump(cx);ws.proof_analyze_dump(cx);ws.proof_reports(&directory.join("reports"),cx);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("16-report-snapshot.png")).unwrap();}).unwrap();
            cx.update(|w,_|w.resize(size(px(1540.),px(1000.)))).unwrap();
            this.update(cx,|ws,cx|ws.proof_interactive_chat(&directory.join("notebook"),cx)).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("17-interactive-chat.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|ws.proof_notebook_tab(cx)).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("18-investigation-notes.png")).unwrap();}).unwrap();
            cx.update(|w,_|w.resize(size(px(1000.),px(640.)))).unwrap();
            this.update(cx,|ws,cx|{ws.chat_open=true;Themes::set_active(cx,"One Light");cx.notify();}).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("19-notebook-minimum.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|{Themes::set_active(cx,"NetLane Dark");ws.proof_codex_recovery(&directory.join("recovered-notebook"),cx);}).unwrap();
            cx.update(|w,_|w.resize(size(px(1320.),px(920.)))).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("20-codex-recovery.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|ws.proof_open_resume(&directory.join("recovered-notebook"),cx)).unwrap();
            cx.update(|w,_|w.resize(size(px(1320.),px(1000.)))).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            this.update(cx,|ws,_|assert!(ws.chat_open)).unwrap();
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("21-reopened-investigation.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|{ws.proof_resume_send(&directory.join("reopened-notebook"),cx);Themes::set_active(cx,"One Light");}).unwrap();
            cx.update(|w,_|w.resize(size(px(1000.),px(920.)))).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            this.update(cx,|ws,_|assert!(ws.chat_open)).unwrap();
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("22-note-state-history.png")).unwrap();}).unwrap();
            std::fs::write(directory.join("resume-verified.txt"),"Three-file notebook reopened after receipt and semantic validation. Unsaved notebook and edited note block replacement; invalid import preserves current history. Opening and preparing bounded local resume excerpts send nothing. Review is required before explicit synthetic follow-up in a new conversation. Original turn contexts remain immutable; user state history is bounded and saved with UTC registration. No native picker, provider, account, real capture or network started.\n").unwrap();
            std::fs::write(directory.join("recovery-verified.txt"),"Synthetic logout and disconnect start distinct conversation ids, retain immutable turns and notes, and reject stale deltas. A full 16-packet selection rejects a seventeenth attachment, allows explicit replacement and removal, and preserves history. Pending model confirmation blocks sending and retains the question. No provider or network started.\n").unwrap();
            std::fs::write(directory.join("interactive-verified.txt"),"Synthetic follow-up turns retain immutable questions and contexts. New evidence conversations preserve the notebook. User annotation keeps the originating turn, starts unconfirmed, restores its context for discussion and requires review before sending. Markdown/JSON notebook and file hashes exported. Split packet/chat layout, notes tab and compact light layout rendered. No real inference or capture.\n").unwrap();
            std::fs::write(directory.join("verified.txt"),"Synthetic packet selection, search, empty result, compact viewport, dark/light branding and reviewed Codex context passed. Virtualized samples, full 10000-packet statistics, baseline comparison and simulated conversation with validated references passed. Synthetic rotating collection wrote 300 records, closed its source and analyzed completed PCAP segments. Derived PCAP/PCAPNG selection/filter exports and no-overwrite receipts passed. Local and assisted Markdown/HTML reports passed; changed question cannot replace the immutable last-turn snapshot, and local report requires current review. Clipboard was not changed. No inference, real capture, network, routing or installed profile accessed.\n").unwrap();
            this.update(cx, |ws, cx| { Themes::set_active(cx, "NetLane Dark"); ws.proof_recover_packet(&directory.join("anchor-fixtures"), cx); }).unwrap();
            cx.update(|w, _| w.resize(size(px(1320.), px(1100.)))).unwrap();
            for _ in 0..40 {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                if this.update(cx, |ws, cx| ws.proof_recovered_packet(cx)).unwrap() { break; }
            }
            this.update(cx, |ws, cx| assert!(ws.proof_recovered_packet(cx))).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w, cx| { w.refresh(); w.draw(cx).clear(cx); w.render_to_image().unwrap().save(directory.join("23-recovered-packet.png")).unwrap(); }).unwrap();
            std::fs::write(directory.join("anchor-verified.txt"), "Production background reader recovered an unsampled packet from a generated closed PCAP; full SHA-256 verified, preview limited to 2048 bytes, distributed samples unchanged. No network, real capture, account, inference or installed data accessed.\n").unwrap();
            this.update(cx, |ws, cx| ws.proof_bundle_save(&directory, cx)).unwrap();
            for _ in 0..50 {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                if this.update(cx, |ws, cx| ws.proof_bundle_ready(false, cx)).unwrap() { break; }
            }
            this.update(cx, |ws, cx| assert!(ws.proof_bundle_ready(false, cx))).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w, cx| { w.refresh(); w.draw(cx).clear(cx); w.render_to_image().unwrap().save(directory.join("24-investigation-package-saved.png")).unwrap(); }).unwrap();
            // Move only owned synthetic directories within this proof's artifact tree.
            std::fs::rename(directory.join("anchor-fixtures"), directory.join("anchor-fixtures-moved")).unwrap();
            let moved = directory.join("moved-package");
            std::fs::rename(directory.join("complete-package"), &moved).unwrap();
            this.update(cx, |ws, cx| ws.proof_bundle_open(&moved, cx)).unwrap();
            for _ in 0..50 {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                if this.update(cx, |ws, cx| ws.proof_bundle_ready(true, cx)).unwrap() { break; }
            }
            this.update(cx, |ws, cx| ws.proof_bundle_check(&moved, cx)).unwrap();
            cx.update(|w, _| w.resize(size(px(1540.), px(1100.)))).unwrap();
            cx.background_executor().timer(Duration::from_millis(550)).await;
            cx.update(|w, cx| { w.refresh(); w.draw(cx).clear(cx); w.render_to_image().unwrap().save(directory.join("25-investigation-package-reopened.png")).unwrap(); }).unwrap();
            std::fs::write(directory.join("bundle-verified.txt"), "Production background workers saved and reopened a moved plaintext package with full PCAP bytes, baseline, immutable synthetic turn and annotation, report receipts, source provenance and an unsent question. Originals unavailable at their old paths. Unsampled packet restored with bounded preview; pending edits and simulated active collection block replacement. Reopening requires review and creates a new provider conversation; no capture, account, network, inference, clipboard or native picker used.\n").unwrap();
            this.update_in(cx,|ws,w,cx|ws.request_close(w,cx)).unwrap();
        }).detach();
    }).unwrap();
}
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
            this.update(cx,|ws,cx|{ws.page=Page::Overview;Themes::set_active(cx,"NetLane Dark");ws.proof_quality_start(cx);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("15-quality-dark.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|{Themes::set_active(cx,"One Light");ws.page=Page::Overview;cx.notify();}).unwrap();
            cx.update(|w,_|w.resize(size(px(1000.),px(640.)))).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("16-quality-minimum.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|{ws.proof_quality_https(cx);ws.proof_quality_details(cx);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("17-quality-https-details.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|ws.proof_quality_stop(cx)).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("18-quality-stopped.png")).unwrap();}).unwrap();
            this.update_in(cx,|ws,w,cx|{ws.proof_default_select(cx);ws.request_close(w,cx);assert!(!ws.proof_exit_confirmation());ws.proof_desktop().proof_post_command(ProofPanelCommand::Minimize);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            this.update(cx,|ws,_|assert!(ws.proof_desktop().visible())).unwrap();
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("19-default-confirmation.png")).unwrap();}).unwrap();
            this.update(cx,|ws,cx|ws.proof_default_apply(cx)).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("20-default-wifi.png")).unwrap();}).unwrap();
            this.update_in(cx,|ws,w,cx|{ws.proof_default_busy(true);ws.request_close(w,cx);assert!(!ws.proof_exit_confirmation());ws.proof_desktop().proof_post_command(ProofPanelCommand::Minimize);}).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,cx|{assert!(ws.proof_desktop().visible());ws.proof_default_busy(false);ws.proof_default_restore(cx);Themes::set_active(cx,"One Dark Pro");}).unwrap();
            cx.background_executor().timer(Duration::from_millis(450)).await;
            cx.update(|w,cx|{w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap().save(directory.join("21-default-restored.png")).unwrap();}).unwrap();
            this.update_in(cx,|ws,w,cx|{ws.page=Page::Settings;Themes::set_active(cx,"One Dark Pro");ws.proof_open_theme_picker(w,cx);}).unwrap();
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
                ws.proof_quality_start(cx);
                ws.proof_edit_rule(cx);
                ws.proof_desktop().proof_post_command(ProofPanelCommand::Close);
                ws.policies[0].application_id.clone()
            }).unwrap();
            cx.background_executor().timer(Duration::from_millis(400)).await;
            this.update(cx,|ws,_|{assert!(!ws.proof_desktop().visible());assert!(ws.dirty);assert!(ws.proof_quality_active());ws.proof_desktop().proof_post_action(TrayAction::Open);}).unwrap();
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
            std::fs::write(directory.join("verified.txt"),"GPUI rendering: 21 scenes\nTheme preview, cancellation, confirmation and disk persistence passed.\nDemo rule edit/save passed.\nNative own-executable icon shared across Rules/Connections; missing and remote paths use fallback.\nManual quality Start/Stop, ICMP/HTTPS labels, synthetic window and minimum layout passed; details component rendered; quality survives tray hide.\nLarge text, compact layout and empty state captured at minimum window size.\nStartup path starts hidden and restores the same panel through its own tray callback.\nNative X/minimize/tray restore preserve the same rule and unsaved edits.\nMaximized restore, Exit cancellation, own-icon recreation, failure fallback passed.\nNew edits during simulated cleanup are reconfirmed; missing cleanup receipt blocks Exit.\nBusy session keeps minimized owner visible. Final X without tray exits safely.\nDefault connection selection, confirmation, Wi-Fi apply and automatic restoration use synthetic metrics; no recovery file written. Busy default operation and confirmation prevent exit/minimize.\nNo network, WFP session, UAC, real autorun or installed profile accessed.\n").unwrap();
            cx.background_executor().timer(Duration::from_millis(250)).await;
            this.update(cx,|ws,_|ws.proof_desktop().proof_post_command(ProofPanelCommand::Close)).unwrap();
        }).detach();
    }).unwrap();
}
