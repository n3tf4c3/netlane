//! Explicit, development-only Npcap/GPUI QA. Fixed loopback and dummy UDP traffic.
//! Never available in ordinary builds or enabled by demo/visual proof flags.
use crate::workspace::Workspace;
use anyhow::{Result, ensure};
use gpui::{App, WindowHandle};
use netlane_capture::collection::Options;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::UdpSocket,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
const LOOPBACK: &str = "\\Device\\NPF_Loopback";
pub struct Plan {
    output: PathBuf,
    seconds: u64,
}
impl Plan {
    pub fn parse(
        args: &[String],
        profile: &Path,
        demo: bool,
        other_proof: bool,
    ) -> Result<Option<Self>> {
        if !args.iter().any(|a| a == "--loopback-qa") {
            ensure!(
                !args
                    .iter()
                    .any(|a| a == "--qa-target" || a == "--qa-seconds"),
                "Opções QA exigem --loopback-qa."
            );
            return Ok(None);
        }
        ensure!(
            !demo && !other_proof && args.len() == 8,
            "QA nativo separado: --loopback-qa PASTA_NOVA --qa-target HOST --qa-seconds SEGUNDOS --profile PASTA_NOVA/profile."
        );
        let value = |key: &str| -> Result<&str> {
            ensure!(
                args.iter().filter(|a| a.as_str() == key).count() == 1,
                "Opção obrigatória/única: {key}"
            );
            let index = args.iter().position(|a| a == key).unwrap();
            ensure!(index % 2 == 0, "Opções QA devem ser pares.");
            Ok(args
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("Valor ausente: {key}"))?)
        };
        let output = PathBuf::from(value("--loopback-qa")?);
        let target = value("--qa-target")?;
        ensure!(
            target.eq_ignore_ascii_case(&std::env::var("COMPUTERNAME")?),
            "O alvo QA precisa corresponder a este PC."
        );
        let seconds = value("--qa-seconds")?.parse()?;
        ensure!(
            (60..=3600).contains(&seconds),
            "Coleta QA: 60 a 3600 segundos."
        );
        ensure!(
            Path::new(value("--profile")?) == profile && profile == output.join("profile"),
            "Perfil exclusivo obrigatório sob a pasta QA."
        );
        ensure!(
            output.is_absolute() && !output.exists() && !profile.exists(),
            "QA exige pasta/perfil novos."
        );
        ensure!(
            output.starts_with(std::env::current_dir()?.join("artifacts"))
                && !output
                    .components()
                    .any(|c| matches!(c, Component::ParentDir)),
            "QA deve ficar sob artifacts local."
        );
        for ancestor in output.ancestors().filter(|p| p.exists()) {
            let metadata = fs::symlink_metadata(ancestor)?;
            ensure!(!metadata.file_type().is_symlink(), "QA não aceita links.");
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "QA não aceita reparse points."
                );
            }
        }
        Ok(Some(Self { output, seconds }))
    }
}
fn write_new(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
struct Traffic {
    port: u16,
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<Result<Value>>>,
}
impl Traffic {
    fn start() -> Result<Self> {
        // Own an ephemeral port before capture, avoiding Hyper-V exclusions.
        let receiver = UdpSocket::bind("127.0.0.1:0")?;
        let destination = receiver.local_addr()?;
        receiver.set_nonblocking(true)?;
        let sender = UdpSocket::bind("127.0.0.1:0")?;
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let task = thread::spawn(move || {
            let mut sent = 0u64;
            let mut received = 0u64;
            let mut buffer = [0u8; 512];
            let mut payload = [b'Q'; 128];
            let began = Instant::now();
            while !signal.load(Ordering::Relaxed) {
                payload[..8].copy_from_slice(&sent.to_le_bytes());
                sender.send_to(&payload, destination)?;
                sent += 1;
                loop {
                    match receiver.recv_from(&mut buffer) {
                        Ok((128, address)) if address.ip().is_loopback() => received += 1,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        other => anyhow::bail!("Unexpected test datagram: {other:?}"),
                    }
                }
                // Bounded 100/s; never burst to catch up after scheduling delays.
                thread::sleep(Duration::from_millis(10));
            }
            Ok(
                json!({"sent":sent,"received":received,"elapsed_seconds":began.elapsed().as_secs_f64(),"payload_bytes":128,"maximum_rate_per_second":100}),
            )
        });
        Ok(Self {
            port: destination.port(),
            stop,
            task: Some(task),
        })
    }
    fn finish(&mut self) -> Result<Value> {
        self.stop.store(true, Ordering::Relaxed);
        self.task
            .take()
            .unwrap()
            .join()
            .map_err(|_| anyhow::anyhow!("Traffic task panicked"))?
    }
}
impl Drop for Traffic {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}
pub fn start(window: WindowHandle<Workspace>, plan: Plan, cx: &mut App) {
    window.update(cx, |_, w, cx| {
        cx.spawn_in(w, async move |this, cx| {
            let result: Result<()> = async {
                fs::create_dir(&plan.output)?;
                let devices = cx.background_executor().spawn(async { netlane_windows::npcap::qa_devices() }).await?;
                write_new(&plan.output.join("devices.json"), &devices.iter().map(|d| json!({"name":d.name,"description":d.description})).collect::<Vec<_>>())?;
                let loopback = devices.iter().find(|d| d.name == LOOPBACK).or_else(|| devices.iter().find(|d| d.description == "Npcap Loopback Adapter")).ok_or_else(|| anyhow::anyhow!("Npcap loopback indisponível."))?.name.clone();
                let mut traffic = Traffic::start()?;
                let filter = format!("udp and host 127.0.0.1 and port {}", traffic.port);
                write_new(&plan.output.join("scope.json"), &json!({"host":std::env::var("COMPUTERNAME")?,"runtime":netlane_windows::npcap::qa_runtime()?,"interface":loopback,"bpf":filter,"seconds":plan.seconds,"max_disk_mib":64,"rate_limit":100,"profile_isolated":true,"routing":false,"inference":false}))?;
                let mut outcomes = Vec::new();
                for (name, seconds, segment_bytes, total_bytes, stop_after, unavailable) in [
                    ("short", 10, 1048576, 67108864, None, false),
                    ("stop", 120, 1048576, 67108864, Some(3), false),
                    ("disk", 120, 4096, 8192, None, false),
                    ("unavailable", 10, 1048576, 67108864, None, true),
                    ("long", plan.seconds, 1048576, 67108864, None, false),
                ] {
                    let parent = plan.output.join(name);
                    fs::create_dir(&parent)?;
                    let options = Options {
                        interface: if unavailable { "\\Device\\NPF_{00000000-0000-0000-0000-000000000000}".into() } else { loopback.clone() },
                        filter: filter.clone(), snaplen: 512, segment_bytes, segment_seconds: 5, total_bytes, duration_seconds: seconds,
                    };
                    this.update(cx, |ws, cx| ws.qa_collection_start(parent.clone(), options, cx))?;
                    let began = Instant::now();
                    let mut previous = began;
                    let mut maximum_gap = 0f64;
                    let mut samples = 0u64;
                    let mut stopped = false;
                    let mut heartbeat = OpenOptions::new().write(true).create_new(true).open(parent.join("heartbeat.jsonl"))?;
                    loop {
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                        let now = Instant::now();
                        maximum_gap = maximum_gap.max(now.duration_since(previous).as_secs_f64());
                        previous = now;
                        samples += 1;
                        let state = this.update(cx, |ws, cx| ws.qa_collection_state(cx))?;
                        writeln!(heartbeat, "{}", json!({"seconds":began.elapsed().as_secs_f64(),"state":state}))?;
                        heartbeat.flush()?;
                        // Render every heartbeat, including when the QA window is
                        // hidden. The resource measurement includes layout/draw.
                        cx.update(|w, cx| { w.refresh(); w.draw(cx).clear(cx); })?;
                        if samples == 2 || (!state["running"].as_bool().unwrap_or(false)) {
                            cx.update(|w, cx| -> Result<()> {
                                w.refresh(); w.draw(cx).clear(cx);
                                w.render_to_image()?.save(parent.join(format!("scene-{samples}.png")))?;
                                Ok(())
                            })??;
                        }
                        if !state["running"].as_bool().unwrap_or(false) {
                            if unavailable {
                                ensure!(state["receipt"]["source_closed"] == false && state["receipt"]["written"] == 0 && state["receipt"]["reason"].as_str().unwrap_or("").starts_with("erro:"), "Interface indisponível não foi informada.");
                            } else {
                                let receipt = &state["receipt"];
                                let expected_reason = if stop_after.is_some() { "parada solicitada" } else if name == "disk" { "limite de disco" } else { "limite de duração" };
                                ensure!(receipt["source_closed"] == true && receipt["reason"] == expected_reason && receipt["written"].as_u64().unwrap_or(0) > 0, "Coleta {name} não cumpriu fechamento/motivo/gravação: {state}");
                                ensure!(receipt["queue_dropped"] == 0 && receipt["unwritten"].as_u64().unwrap_or(u64::MAX) <= 129 && receipt["analysis_failed"] == 0 && receipt["analysis_skipped"] == 0, "Perda/falha na coleta {name}: {receipt}");
                                ensure!(receipt["driver_stats"]["dropped"] == 0 && receipt["driver_stats"]["interface_dropped"] == 0, "Driver reportou perdas ou não retornou estatísticas.");
                                ensure!(receipt["written"] == receipt["buffered"], "Contadores finais divergentes.");
                                ensure!(state["displayed_packets"].as_u64().unwrap_or(0) > 0, "Segmento não chegou à tabela GPUI.");
                            }
                            let outcome = json!({"case":name,"elapsed_seconds":began.elapsed().as_secs_f64(),"ui_heartbeats":samples,"maximum_ui_gap_seconds":maximum_gap,"state":state});
                            write_new(&parent.join("outcome.json"), &outcome)?;
                            outcomes.push(outcome);
                            break;
                        }
                        if !stopped && (stop_after.is_some_and(|limit| began.elapsed().as_secs() >= limit) || began.elapsed().as_secs() > seconds + 10) {
                            this.update(cx, |ws, cx| ws.qa_collection_stop(cx))?;
                            stopped = true;
                        }
                        ensure!(began.elapsed().as_secs() < seconds + 30, "Coleta excedeu prazo de fechamento.");
                    }
                }
                let traffic_result = traffic.finish()?;
                write_new(&plan.output.join("verification.json"), &json!({"passed":true,"cases":outcomes,"traffic":traffic_result,"native_npcap":true,"production_gui_pipeline":true,"manual_file_picker":false,"ai_login":false}))?;
                Ok(())
            }.await;
            if let Err(error) = result {
                let _ = this.update(cx, |ws, cx| ws.qa_collection_stop(cx));
                // Allow the non-blocking native reader and pending disk writes to close.
                for _ in 0..30 {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    if this.update(cx, |ws, cx| ws.qa_collection_state(cx)["running"] == false).unwrap_or(true) { break; }
                }
                let _ = write_new(&plan.output.join("failure.json"), &json!({"passed":false,"error":format!("{error:#}")}));
                eprintln!("Loopback QA: {error:#}");
            }
            let _ = cx.update(|_, cx| cx.quit());
        }).detach();
    }).expect("loopback QA window");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qa_never_activates_with_demo_and_requires_explicit_target_profile_and_duration() {
        let args: Vec<String> = [
            "--loopback-qa",
            "C:\\Codes\\netlane\\artifacts\\unused-qa",
            "--qa-target",
            "OTHER-PC",
            "--qa-seconds",
            "1200",
            "--profile",
            "C:\\Codes\\netlane\\artifacts\\unused-qa\\profile",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let profile = PathBuf::from(&args[7]);
        assert!(Plan::parse(&args, &profile, true, false).is_err());
        assert!(Plan::parse(&args, &profile, false, true).is_err());
        assert!(Plan::parse(&args, &profile, false, false).is_err());
        assert!(
            Plan::parse(
                &["--qa-seconds".into(), "1200".into()],
                &profile,
                false,
                false
            )
            .is_err()
        );
        assert!(
            Plan::parse(&["--demo".into()], &profile, true, false)
                .unwrap()
                .is_none()
        );
    }
}
