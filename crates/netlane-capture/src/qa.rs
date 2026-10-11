//! Reproducible offline QA. This module has no driver, socket or AI provider.
use crate::{Format, analysis, collection, export::Selection};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub struct FixtureOptions {
    pub records: u32,
    pub packet_bytes: u32,
}
impl Default for FixtureOptions {
    fn default() -> Self {
        Self {
            records: 70_000,
            packet_bytes: 2112,
        }
    }
}
impl FixtureOptions {
    fn validate(self) -> Result<()> {
        ensure!(
            (2000..=500_000).contains(&self.records)
                && self.records.is_multiple_of(2)
                && (64..=65535).contains(&self.packet_bytes)
                && 24 + u64::from(self.records) * (16 + u64::from(self.packet_bytes))
                    <= 1024 * 1024 * 1024,
            "Fixtures: quantidade par de 2000..500000, pacotes 64..65535 bytes e arquivo de até 1 GiB."
        );
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub valid: bool,
    pub packets: u64,
    pub tcp_packets: u64,
    pub tcp_resets: u64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixtures {
    pub schema: u32,
    pub synthetic: bool,
    pub files: Vec<Fixture>,
    pub deviation_first_packet: u64,
}

fn local_absolute(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute() && (!cfg!(windows) || !path.to_string_lossy().starts_with("\\\\")),
        "Use um caminho absoluto local, sem compartilhamento de rede."
    );
    for parent in path.ancestors() {
        match fs::symlink_metadata(parent) {
            Ok(meta) => {
                ensure!(
                    !meta.file_type().is_symlink(),
                    "Links não são aceitos no kit QA."
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        meta.file_attributes() & 0x400 == 0,
                        "Reparse points não são aceitos no kit QA."
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn new_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer_pretty(&mut f, value)?;
    f.write_all(b"\n")?;
    f.sync_all()?;
    Ok(())
}
fn fingerprint(path: &Path) -> Result<(u64, String)> {
    let f = File::open(path)?;
    let before = f.metadata()?;
    ensure!(before.is_file(), "Arquivo comum obrigatório.");
    let mut r = BufReader::new(&f);
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut bytes = 0;
    loop {
        let n = r.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(bytes <= 1024 * 1024 * 1024, "Fixture acima de 1 GiB.");
        hash.update(&buffer[..n]);
    }
    let after = f.metadata()?;
    ensure!(
        before.len() == bytes
            && after.len() == bytes
            && before.modified().ok() == after.modified().ok(),
        "Fixture mudou durante a leitura."
    );
    Ok((bytes, format!("{:X}", hash.finalize())))
}
/// Documentation IPs, empty MACs and zero checksums: deliberately synthetic frames.
fn frame(index: u64, size: usize, deviates: bool) -> Vec<u8> {
    let tcp = index.is_multiple_of(2);
    let mut b = vec![0x51; size];
    b[..54].fill(0);
    b[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
    b[14] = 0x45;
    b[16..18].copy_from_slice(&((size - 14) as u16).to_be_bytes());
    b[22] = 64;
    b[23] = if tcp { 6 } else { 17 };
    b[26..30].copy_from_slice(&[192, 0, 2, 10]);
    b[30..34].copy_from_slice(&[198, 51, 100, if deviates { 99 } else { 20 }]);
    b[34..36].copy_from_slice(&50000u16.to_be_bytes());
    b[36..38].copy_from_slice(&(if tcp { 443u16 } else { 53u16 }).to_be_bytes());
    if tcp {
        b[46] = 0x50;
        b[47] = if deviates { 4 } else { 16 };
    } else {
        b[38..40].copy_from_slice(&((size - 34) as u16).to_be_bytes());
    }
    b
}
fn pcap(path: &Path, records: u32, size: u32, anomaly: bool) -> Result<()> {
    let mut w = BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);
    for word in [0xa1b2c3d4u32, 0x00040002, 0, 0, size, 1] {
        w.write_all(&word.to_le_bytes())?;
    }
    for n in 0..records {
        for word in [1_791_590_400 + n / 1000, (n % 1000) * 1000, size, size] {
            w.write_all(&word.to_le_bytes())?;
        }
        w.write_all(&frame(
            u64::from(n),
            size as usize,
            anomaly && n >= records / 2,
        ))?;
    }
    w.flush()?;
    w.get_ref().sync_all()?;
    Ok(())
}
pub fn generate(directory: &Path, options: FixtureOptions) -> Result<Fixtures> {
    local_absolute(directory)?;
    options.validate()?;
    fs::create_dir(directory).context("Fixtures exigem pasta nova")?;
    pcap(&directory.join("baseline.pcap"), 1000, 128, false)?;
    pcap(
        &directory.join("deviation.pcap"),
        options.records,
        options.packet_bytes,
        true,
    )?;
    let source = crate::load(&directory.join("baseline.pcap"), &AtomicBool::new(false))?;
    Selection::new(&source, &[0, 3, 8])?.save(&directory.join("pcapng"), Format::PcapNg)?;
    let normal = fs::read(directory.join("baseline.pcap"))?;
    let mut short = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("truncated.pcap"))?;
    short.write_all(&normal[..normal.len() - 1])?;
    short.sync_all()?;
    let mut bad = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("invalid.pcap"))?;
    bad.write_all(b"Not a packet capture; synthetic QA rejection case.")?;
    bad.sync_all()?;
    let mut files = Vec::new();
    for (name, valid, packets, tcp, rst) in [
        ("baseline.pcap", true, 1000, 500, 0),
        (
            "deviation.pcap",
            true,
            u64::from(options.records),
            u64::from(options.records) / 2,
            u64::from(options.records) / 4,
        ),
        ("pcapng/selected.pcapng", true, 3, 2, 0),
        ("truncated.pcap", false, 0, 0, 0),
        ("invalid.pcap", false, 0, 0, 0),
    ] {
        let (bytes, sha256) = fingerprint(&directory.join(name))?;
        files.push(Fixture {
            path: name.into(),
            bytes,
            sha256,
            valid,
            packets,
            tcp_packets: tcp,
            tcp_resets: rst,
        });
    }
    let manifest = Fixtures {
        schema: 1,
        synthetic: true,
        files,
        deviation_first_packet: u64::from(options.records) / 2 + 1,
    };
    new_json(&directory.join("expected.json"), &manifest)?;
    Ok(manifest)
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Stress {
    pub seconds: u64,
    pub limit_mib: u64,
    pub packets_per_second: u64,
}
impl Default for Stress {
    fn default() -> Self {
        Self {
            seconds: 2,
            limit_mib: 16,
            packets_per_second: 20_000,
        }
    }
}
impl Stress {
    fn validate(self) -> Result<()> {
        ensure!(
            (1..=86400).contains(&self.seconds)
                && (4..=4096).contains(&self.limit_mib)
                && (100..=50_000).contains(&self.packets_per_second),
            "Estresse: 1..86400 s, 4..4096 MiB, 100..50000 pacotes/s."
        );
        Ok(())
    }
}
#[derive(Serialize)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub elapsed_ms: u128,
    pub error: Option<String>,
}
#[derive(Serialize)]
pub struct Run {
    pub schema: u32,
    pub passed: bool,
    pub synthetic_only: bool,
    pub real_capture_started: bool,
    pub inference_sent: bool,
    pub elapsed_ms: u128,
    pub stress: Stress,
    pub checks: Vec<Check>,
}
fn check(checks: &mut Vec<Check>, name: &str, work: impl FnOnce() -> Result<()>) -> bool {
    let start = Instant::now();
    let result = work();
    let passed = result.is_ok();
    checks.push(Check {
        name: name.into(),
        passed,
        elapsed_ms: start.elapsed().as_millis(),
        error: result.err().map(|e| format!("{e:#}")),
    });
    passed
}
struct Synthetic {
    start: Instant,
    rate: u64,
    generated: u64,
    closed: Arc<AtomicBool>,
}
impl Drop for Synthetic {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}
impl collection::Source for Synthetic {
    fn link_type(&self) -> u16 {
        1
    }
    fn poll(&mut self) -> Result<collection::Poll> {
        let ready = self.start.elapsed().as_micros() * u128::from(self.rate) / 1_000_000;
        if u128::from(self.generated) >= ready {
            return Ok(collection::Poll::Idle);
        }
        let n = self.generated;
        self.generated += 1;
        Ok(collection::Poll::Packet(collection::Record {
            seconds: 1_791_590_400 + (n / 1000) as u32,
            micros: ((n % 1000) * 1000) as u32,
            original_len: 128,
            bytes: frame(n, 128, n >= self.rate),
        }))
    }
    fn stats(&mut self) -> Option<collection::DriverStats> {
        None
    }
}
fn stress(directory: &Path, limits: Stress) -> Result<()> {
    let closed = Arc::new(AtomicBool::new(false));
    let proof = closed.clone();
    let (tx, rx) = mpsc::sync_channel(2);
    // Intentional slow display consumer. Preview skips must not lose saved segment files.
    let collector = std::thread::spawn(move || {
        let mut previews = 0;
        while let Ok(_scan) = rx.recv() {
            previews += 1;
            std::thread::sleep(Duration::from_millis(25));
        }
        previews
    });
    let options = collection::Options {
        interface: "synthetic-qa-no-network".into(),
        snaplen: 128,
        segment_bytes: 1024 * 1024,
        segment_seconds: 1,
        total_bytes: limits.limit_mib * 1024 * 1024,
        duration_seconds: limits.seconds,
        ..Default::default()
    };
    let result = collection::run(
        directory.to_path_buf(),
        options,
        move || {
            Ok(Synthetic {
                start: Instant::now(),
                rate: limits.packets_per_second,
                generated: 0,
                closed,
            })
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(collection::Progress::default()),
        tx,
    );
    let previews = collector
        .join()
        .map_err(|_| anyhow::anyhow!("Consumidor de prévias falhou."))?;
    let receipt = result?;
    new_json(
        &directory.join("offline-stress.json"),
        &serde_json::json!({"synthetic_only":true,"previews":previews,"receipt":receipt}),
    )?;
    ensure!(
        proof.load(Ordering::Relaxed) && receipt.source_closed,
        "Fonte sintética não fechou."
    );
    ensure!(
        !receipt.reason.starts_with("erro:"),
        "Coletor retornou erro: {}",
        receipt.reason
    );
    ensure!(
        receipt.written > 0 && receipt.analysis_failed == 0,
        "Nenhum pacote salvo ou análise de segmento falhou."
    );
    ensure!(
        receipt.seen == receipt.written + receipt.queue_dropped + receipt.unwritten,
        "Contadores não conciliam."
    );
    ensure!(
        receipt.bytes <= limits.limit_mib * 1024 * 1024,
        "Limite de disco excedido."
    );
    let mut packets = 0;
    let mut bytes = 0;
    for segment in &receipt.segments {
        let path = directory.join(&segment.file);
        let (size, sha) = fingerprint(&path)?;
        let scan = analysis::load_dump(
            &path,
            &AtomicBool::new(false),
            &analysis::Progress::default(),
        )?;
        ensure!(
            size == segment.bytes
                && sha == segment.sha256
                && scan.report.sha256 == sha
                && scan.report.packets == segment.packets
                && size <= 1024 * 1024,
            "Segmento salvo divergente."
        );
        packets += segment.packets;
        bytes += size;
    }
    ensure!(
        packets == receipt.written && bytes == receipt.bytes,
        "Segmentos não conciliam com o recibo."
    );
    Ok(())
}

const FILES: [&str; 5] = [
    "baseline.pcap",
    "deviation.pcap",
    "pcapng/selected.pcapng",
    "truncated.pcap",
    "invalid.pcap",
];
fn manifest(root: &Path) -> Result<Fixtures> {
    local_absolute(root)?;
    let path = root.join("expected.json");
    local_absolute(&path)?;
    ensure!(
        fs::metadata(&path)?.len() <= 65536,
        "Manifesto QA grande demais."
    );
    let data: Fixtures = serde_json::from_reader(File::open(path)?)?;
    ensure!(
        data.schema == 1 && data.synthetic && data.files.len() == FILES.len(),
        "Manifesto QA incompatível."
    );
    for (n, file) in data.files.iter().enumerate() {
        ensure!(
            file.path == FILES[n]
                && file.valid == (n < 3)
                && file.bytes <= 1024 * 1024 * 1024
                && file.sha256.len() == 64
                && file.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Entrada QA inválida."
        );
        local_absolute(&root.join(&file.path))?;
    }
    Ok(data)
}

/// Executes only synthetic disk/analysis workloads. A pass never approves live capture or AI.
pub fn verify(root: &Path, output: &Path, limits: Stress) -> Result<Run> {
    let start = Instant::now();
    local_absolute(output)?;
    limits.validate()?;
    fs::create_dir(output).context("Resultados exigem pasta nova")?;
    let mut checks = Vec::new();
    let mut data = None;
    let ready = check(&mut checks, "fixture-integrity", || {
        let parsed = manifest(root)?;
        for file in &parsed.files {
            let (bytes, sha) = fingerprint(&root.join(&file.path))?;
            ensure!(
                bytes == file.bytes && sha.eq_ignore_ascii_case(&file.sha256),
                "Fixture divergente: {}",
                file.path
            );
        }
        data = Some(parsed);
        Ok(())
    });
    if ready {
        let data = data.unwrap();
        let mut baseline = None;
        let mut deviation = None;
        for (n, file) in data.files.iter().enumerate() {
            check(&mut checks, &format!("import-{}", file.path), || {
                let progress = analysis::Progress::default();
                let result =
                    analysis::load_dump(&root.join(&file.path), &AtomicBool::new(false), &progress);
                if !file.valid {
                    ensure!(result.is_err(), "Dump inválido foi aceito.");
                    return Ok(());
                }
                let scan = result?;
                ensure!(
                    scan.report.packets == file.packets
                        && scan.report.tcp_packets == file.tcp_packets
                        && scan.report.tcp_resets == file.tcp_resets
                        && scan.report.sha256.eq_ignore_ascii_case(&file.sha256),
                    "Totais do dump divergentes."
                );
                ensure!(
                    progress.packets.load(Ordering::Relaxed) == file.packets
                        && scan.capture.packets.len() <= analysis::MAX_SAMPLES
                        && scan.capture.retained_bytes
                            <= analysis::MAX_SAMPLES * analysis::SAMPLE_BYTES
                        && scan
                            .capture
                            .packets
                            .iter()
                            .all(|p| p.bytes.len() <= analysis::SAMPLE_BYTES)
                        && scan.report.windows.len() <= analysis::MAX_WINDOWS
                        && scan.report.destinations.len() <= analysis::MAX_DESTINATIONS,
                    "Análise não respeitou os limites."
                );
                new_json(&output.join(format!("analysis-{n}.json")), &scan.report)?;
                if n == 0 {
                    baseline = Some(scan);
                } else if n == 1 {
                    deviation = Some(scan);
                }
                Ok(())
            });
        }
        check(&mut checks, "baseline-and-deviation", || {
            let base = baseline.as_ref().context("Baseline não carregada")?;
            let dump = deviation.as_ref().context("Desvio não carregado")?;
            let findings = dump.report.compare(&base.report);
            ensure!(
                findings.iter().any(|f| f.kind == "tcp-rst")
                    && findings.iter().any(|f| f.kind == "destino-novo"
                        && f.packet_references.contains(&data.deviation_first_packet)),
                "Desvio conhecido não identificado."
            );
            let ratio = dump.report.byte_rate().context("Taxa do dump ausente")?
                / base
                    .report
                    .byte_rate()
                    .context("Taxa da baseline ausente")?;
            ensure!(
                findings.iter().any(|f| f.kind == "volume") == (ratio >= 3.),
                "Limiar de volume divergente."
            );
            new_json(&output.join("findings.json"), &findings)?;
            Ok(())
        });
        check(&mut checks, "derived-export-and-no-overwrite", || {
            let dump = deviation.as_ref().context("Desvio não carregado")?;
            let selection = Selection::new(&dump.capture, &[0, dump.capture.packets.len() - 1])?;
            for (folder, format) in [
                ("derived-pcap", Format::Pcap),
                ("derived-pcapng", Format::PcapNg),
            ] {
                let target = output.join(folder);
                let receipt = selection.save(&target, format)?;
                let reopened = crate::load(&target.join(&receipt.file), &AtomicBool::new(false))?;
                ensure!(
                    reopened.total_packets == 2
                        && reopened.sha256 == receipt.sha256
                        && reopened.packets.iter().zip(&receipt.packets).all(|(p, m)| p
                            .bytes
                            .len()
                            == m.exported_bytes
                            && p.original_len == m.original_bytes),
                    "Exportação divergente."
                );
                ensure!(
                    selection.save(&target, format).is_err(),
                    "Exportação sobrescreveu a anterior."
                );
                ensure!(
                    fingerprint(&target.join(&receipt.file))?.1 == receipt.sha256,
                    "Rejeição alterou exportação anterior."
                );
            }
            Ok(())
        });
        check(&mut checks, "cancelled-import", || {
            ensure!(
                analysis::load_dump(
                    &root.join("deviation.pcap"),
                    &AtomicBool::new(true),
                    &analysis::Progress::default()
                )
                .is_err(),
                "Importação ignorou cancelamento."
            );
            Ok(())
        });
        check(&mut checks, "originals-preserved", || {
            for file in &data.files {
                let (bytes, sha) = fingerprint(&root.join(&file.path))?;
                ensure!(
                    bytes == file.bytes && sha.eq_ignore_ascii_case(&file.sha256),
                    "Original alterado: {}",
                    file.path
                );
            }
            Ok(())
        });
    }
    // Invalid fixtures cannot authorize any additional work, even synthetic collection.
    if ready {
        check(&mut checks, "bounded-offline-collection", || {
            stress(&output.join("collection"), limits)
        });
    }
    let run = Run {
        schema: 1,
        passed: checks.iter().all(|c| c.passed),
        synthetic_only: true,
        real_capture_started: false,
        inference_sent: false,
        elapsed_ms: start.elapsed().as_millis(),
        stress: limits,
        checks,
    };
    new_json(&output.join("verification.json"), &run)?;
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_contract_and_failure_receipts() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("fixtures");
        // An odd half catches the boundary between ACK and RST frame generation.
        generate(
            &root,
            FixtureOptions {
                records: 2002,
                packet_bytes: 2112,
            },
        )?;
        let limits = Stress {
            seconds: 1,
            limit_mib: 4,
            packets_per_second: 1000,
        };
        let run = verify(&root, &dir.path().join("result"), limits)?;
        assert!(run.passed, "{}", serde_json::to_string(&run)?);
        assert!(generate(&root, FixtureOptions::default()).is_err());
        assert!(verify(&root, &dir.path().join("result"), limits).is_err());
        fs::write(root.join("invalid.pcap"), b"changed")?;
        let failed = verify(&root, &dir.path().join("tampered"), limits)?;
        assert!(!failed.passed && failed.checks.len() == 1);
        assert!(!dir.path().join("tampered/collection").exists());
        assert!(dir.path().join("tampered/verification.json").is_file());
        Ok(())
    }
    #[test]
    fn fixture_manifest_cannot_escape_root() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("fixtures");
        let mut data = generate(
            &root,
            FixtureOptions {
                records: 2000,
                packet_bytes: 128,
            },
        )?;
        data.files[0].path = "../private.pcap".into();
        fs::write(root.join("expected.json"), serde_json::to_vec(&data)?)?;
        assert!(manifest(&root).is_err());
        assert!(
            FixtureOptions {
                records: 500000,
                packet_bytes: 65535
            }
            .validate()
            .is_err()
        );
        Ok(())
    }
}
