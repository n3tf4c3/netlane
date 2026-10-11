//! Offline interoperable-file QA through the production whole-dump analyzer.
use anyhow::{Result, ensure};
use netlane_capture::analysis::{self, Progress};
use serde_json::json;
use std::{fs::OpenOptions, io::Write, path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("inspect_dump OUTPUT_NEW.json FILE..."))?,
    );
    ensure!(
        output.is_absolute() && !output.exists(),
        "Absolute new output required."
    );
    let paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    ensure!(
        !paths.is_empty() && paths.len() <= 4096,
        "One to 4096 local files required."
    );
    let mut reports = Vec::new();
    for path in paths {
        ensure!(path.is_absolute(), "Absolute input required.");
        let scan = analysis::load_dump(&path, &AtomicBool::new(false), &Progress::default())?;
        reports.push(json!({"path":path,"report":scan.report,"samples":scan.capture.packets.iter().map(|p|json!({"number":p.number,"captured":p.captured_len,"original":p.original_len,"retained":p.bytes.len(),"unix_nanos":p.timestamp.map(|t|t.unix_nanos().to_string())})).collect::<Vec<_>>()}));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    serde_json::to_writer_pretty(
        &mut file,
        &json!({"passed":true,"files":reports,"native_capture":false,"inference":false}),
    )?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    println!(
        "{} files inspected by the production analyzer.",
        reports.len()
    );
    Ok(())
}
