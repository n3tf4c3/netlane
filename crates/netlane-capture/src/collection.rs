//! Bounded capture pipeline. Sources are opened only by an explicit caller.
//! A blocked disk or analyzer never creates an unbounded packet backlog.
use crate::analysis::{Progress as AnalysisProgress, Scan, load_dump};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub const PACKET_QUEUE: usize = 128;
pub const ANALYSIS_QUEUE: usize = 2;
pub const MAX_SEGMENTS: usize = 4096;
#[derive(Clone, Debug)]
pub struct Record {
    pub seconds: u32,
    pub micros: u32,
    pub original_len: u32,
    pub bytes: Vec<u8>,
}
pub enum Poll {
    Packet(Record),
    Idle,
    End,
}
#[derive(Clone, Copy, Default, Debug, Serialize)]
pub struct DriverStats {
    pub received: u32,
    pub dropped: u32,
    pub interface_dropped: u32,
}
/// poll must be non-blocking; the implementation owns its native handle.
pub trait Source {
    fn link_type(&self) -> u16;
    fn poll(&mut self) -> Result<Poll>;
    fn stats(&mut self) -> Option<DriverStats>;
}
#[derive(Clone, Debug, Serialize)]
pub struct Options {
    pub interface: String,
    pub filter: String,
    pub snaplen: u32,
    pub segment_bytes: u64,
    pub segment_seconds: u64,
    pub total_bytes: u64,
    pub duration_seconds: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            interface: String::new(),
            filter: String::new(),
            snaplen: 2048,
            segment_bytes: 64 * 1024 * 1024,
            segment_seconds: 60,
            total_bytes: 4 * 1024 * 1024 * 1024,
            duration_seconds: 24 * 60 * 60,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.interface.is_empty()
                && self.interface.len() <= 512
                && !self.interface.chars().any(char::is_control),
            "Escolha uma interface de captura."
        );
        ensure!(
            self.filter.len() <= 512 && !self.filter.chars().any(char::is_control),
            "Filtro BPF inválido."
        );
        ensure!(
            (64..=65535).contains(&self.snaplen)
                && self.segment_bytes >= 24 + 16 + u64::from(self.snaplen)
                && self.segment_bytes <= 128 * 1024 * 1024
                && self.total_bytes >= self.segment_bytes
                && self.total_bytes <= 64 * 1024 * 1024 * 1024
                && (1..=3600).contains(&self.segment_seconds)
                && (1..=7 * 24 * 3600).contains(&self.duration_seconds),
            "Limites de coleta inválidos."
        );
        Ok(())
    }
}
#[derive(Default)]
pub struct Progress {
    pub seen: AtomicU64,
    pub written: AtomicU64,
    pub bytes: AtomicU64,
    pub queue_dropped: AtomicU64,
    pub segments: AtomicU64,
    pub analysis_skipped: AtomicU64,
    pub analysis_failed: AtomicU64,
    pub display_skipped: AtomicU64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Segment {
    pub file: String,
    pub packets: u64,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize)]
pub struct Receipt {
    pub options: Options,
    pub start_unix_seconds: u64,
    pub end_unix_seconds: u64,
    pub reason: String,
    pub source_closed: bool,
    pub seen: u64,
    pub written: u64,
    pub bytes: u64,
    pub queue_dropped: u64,
    pub unwritten: u64,
    pub driver_stats: Option<DriverStats>,
    pub analysis_skipped: u64,
    pub analysis_failed: u64,
    pub display_skipped: u64,
    pub segments: Vec<Segment>,
}
fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn new_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
struct Producer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<Option<DriverStats>>>>,
    packets: Receiver<Record>,
    link_type: u16,
}
impl Drop for Producer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Producer {
    fn start<S: Source + 'static>(
        factory: impl FnOnce() -> Result<S> + Send + 'static,
        stop: Arc<AtomicBool>,
        progress: Arc<Progress>,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(PACKET_QUEUE);
        let (setup_tx, setup_rx) = mpsc::sync_channel(1);
        let signal = stop.clone();
        let thread = thread::Builder::new()
            .name("netlane-pcap-read".into())
            .spawn(move || {
                let mut source = match factory() {
                    Ok(source) => source,
                    Err(error) => {
                        let _ = setup_tx.send(Err(error.to_string()));
                        return Err(error);
                    }
                };
                if setup_tx.send(Ok(source.link_type())).is_err() {
                    return Ok(source.stats());
                }
                let result = (|| {
                    while !signal.load(Ordering::Relaxed) {
                        match source.poll()? {
                            Poll::Packet(packet) => {
                                progress.seen.fetch_add(1, Ordering::Relaxed);
                                match tx.try_send(packet) {
                                    Ok(()) => {}
                                    Err(TrySendError::Full(_)) => {
                                        progress.queue_dropped.fetch_add(1, Ordering::Relaxed);
                                    }
                                    Err(TrySendError::Disconnected(_)) => break,
                                }
                            }
                            Poll::Idle => thread::sleep(Duration::from_millis(10)),
                            Poll::End => break,
                        }
                    }
                    Ok(())
                })();
                let stats = source.stats();
                drop(source);
                result.map(|()| stats)
            })?;
        let mut producer = Self {
            stop,
            thread: Some(thread),
            packets: rx,
            link_type: 0,
        };
        producer.link_type = setup_rx
            .recv()
            .context("A fonte de captura não iniciou.")?
            .map_err(anyhow::Error::msg)?;
        Ok(producer)
    }
    fn finish(&mut self) -> Result<Option<DriverStats>> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread
            .take()
            .unwrap()
            .join()
            .map_err(|_| anyhow::anyhow!("A fonte de captura falhou."))?
    }
}
struct Writer {
    file: BufWriter<File>,
    hash: Sha256,
    path: PathBuf,
    bytes: u64,
    packets: u64,
    started: Instant,
}
impl Writer {
    fn open(directory: &Path, index: usize, link: u16, snaplen: u32) -> Result<Self> {
        let path = directory.join(format!("segment-{index:04}.pcap.part"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut writer = Self {
            file: BufWriter::with_capacity(64 * 1024, file),
            hash: Sha256::new(),
            path,
            bytes: 0,
            packets: 0,
            started: Instant::now(),
        };
        let mut header = Vec::with_capacity(24);
        header.extend(0xa1b2c3d4u32.to_le_bytes());
        header.extend(2u16.to_le_bytes());
        header.extend(4u16.to_le_bytes());
        header.extend([0; 8]);
        header.extend(snaplen.to_le_bytes());
        header.extend(u32::from(link).to_le_bytes());
        writer.write(&header)?;
        Ok(writer)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.file.write_all(bytes)?;
        self.hash.update(bytes);
        self.bytes += bytes.len() as u64;
        Ok(())
    }
    fn packet(&mut self, p: &Record, snaplen: u32) -> Result<()> {
        ensure!(
            p.micros < 1_000_000
                && !p.bytes.is_empty()
                && p.bytes.len() <= snaplen as usize
                && p.bytes.len() <= p.original_len as usize,
            "Registro inválido na fonte de captura."
        );
        for value in [p.seconds, p.micros, p.bytes.len() as u32, p.original_len] {
            self.write(&value.to_le_bytes())?;
        }
        self.write(&p.bytes)?;
        self.packets += 1;
        Ok(())
    }
    fn close(mut self) -> Result<Segment> {
        self.file.flush()?;
        self.file.get_ref().sync_all()?;
        drop(self.file);
        let target = self.path.with_extension("");
        ensure!(!target.exists(), "O segmento de destino já existe.");
        std::fs::rename(&self.path, &target)?;
        Ok(Segment {
            file: target.file_name().unwrap().to_string_lossy().into_owned(),
            packets: self.packets,
            bytes: self.bytes,
            sha256: format!("{:X}", self.hash.finalize()),
        })
    }
}
/// New absolute directory only. Never deletes or overwrites earlier captures.
/// Completed segments are valid PCAP files; .part files are never analyzed.
pub fn run<S: Source + 'static>(
    directory: PathBuf,
    options: Options,
    factory: impl FnOnce() -> Result<S> + Send + 'static,
    stop: Arc<AtomicBool>,
    progress: Arc<Progress>,
    events: SyncSender<Scan>,
) -> Result<Receipt> {
    options.validate()?;
    ensure!(
        directory.is_absolute() && !directory.exists(),
        "Escolha uma pasta nova absoluta para a coleta."
    );
    ensure!(
        !stop.load(Ordering::Relaxed),
        "Coleta cancelada antes de abrir a fonte."
    );
    std::fs::create_dir(&directory)?;
    new_json(&directory.join("capture-start.json"), &options)?;
    let began = Instant::now();
    let started = unix_seconds();
    let mut segments = Vec::new();
    let mut driver_stats = None;
    let mut source_closed = false;
    let mut reason = "fonte encerrada".to_string();
    let (analysis_tx, analysis_rx) = mpsc::sync_channel::<Segment>(ANALYSIS_QUEUE);
    let analysis_directory = directory.clone();
    let analysis_progress = progress.clone();
    let analyzer = thread::Builder::new()
        .name("netlane-pcap-analyze".into())
        .spawn(move || {
            for segment in analysis_rx {
                let result = load_dump(
                    &analysis_directory.join(&segment.file),
                    &AtomicBool::new(false),
                    &AnalysisProgress::default(),
                );
                match result {
                    Ok(scan) if scan.report.sha256 == segment.sha256 => {
                        if events.try_send(scan).is_err() {
                            analysis_progress
                                .display_skipped
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    _ => {
                        analysis_progress
                            .analysis_failed
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })?;
    let close_segment = |writer: Writer, segments: &mut Vec<Segment>| -> Result<()> {
        let segment = writer.close()?;
        progress.segments.fetch_add(1, Ordering::Relaxed);
        if analysis_tx.try_send(segment.clone()).is_err() {
            progress.analysis_skipped.fetch_add(1, Ordering::Relaxed);
        }
        segments.push(segment);
        Ok(())
    };
    let result = (|| {
        let mut source = Producer::start(factory, stop.clone(), progress.clone())?;
        let mut writer = Some(Writer::open(
            &directory,
            1,
            source.link_type,
            options.snaplen,
        )?);
        let writing = (|| {
            loop {
                if stop.load(Ordering::Relaxed) {
                    reason = "parada solicitada".into();
                    break;
                }
                if began.elapsed().as_secs() >= options.duration_seconds {
                    reason = "limite de duração".into();
                    break;
                }
                if writer.as_ref().is_some_and(|w| {
                    w.packets > 0 && w.started.elapsed().as_secs() >= options.segment_seconds
                }) {
                    close_segment(writer.take().unwrap(), &mut segments)?;
                }
                match source.packets.recv_timeout(Duration::from_millis(20)) {
                    Ok(packet) => {
                        let size = 16 + packet.bytes.len() as u64;
                        if writer.as_ref().is_some_and(|w| {
                            w.bytes + size > options.segment_bytes && w.packets > 0
                        }) {
                            close_segment(writer.take().unwrap(), &mut segments)?;
                        }
                        if writer.is_none() {
                            if segments.len() >= MAX_SEGMENTS {
                                reason = "limite de segmentos".into();
                                break;
                            }
                            if progress.bytes.load(Ordering::Relaxed) + 24 + size
                                > options.total_bytes
                            {
                                reason = "limite de disco".into();
                                break;
                            }
                            writer = Some(Writer::open(
                                &directory,
                                segments.len() + 1,
                                source.link_type,
                                options.snaplen,
                            )?);
                        }
                        let w = writer.as_mut().unwrap();
                        if progress.bytes.load(Ordering::Relaxed)
                            + size
                            + if w.packets == 0 { 24 } else { 0 }
                            > options.total_bytes
                        {
                            reason = "limite de disco".into();
                            break;
                        }
                        let before = w.bytes;
                        w.packet(&packet, options.snaplen)?;
                        progress.bytes.fetch_add(
                            w.bytes - before + if w.packets == 1 { 24 } else { 0 },
                            Ordering::Relaxed,
                        );
                        progress.written.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        if stop.load(Ordering::Relaxed) {
                            reason = "parada solicitada".into();
                        }
                        break;
                    }
                }
            }
            if let Some(writer) = writer.take() {
                close_segment(writer, &mut segments)?;
            }
            Ok(())
        })();
        let closed = source.finish();
        source_closed = true;
        match closed {
            Ok(stats) => driver_stats = stats,
            Err(error) => {
                if writing.is_ok() {
                    return Err(error);
                }
            }
        }
        writing
    })();
    stop.store(true, Ordering::Relaxed);
    drop(analysis_tx);
    if analyzer.join().is_err() {
        progress.analysis_failed.fetch_add(1, Ordering::Relaxed);
    }
    if let Err(error) = result {
        reason = format!("erro: {error:#}");
    }
    let seen = progress.seen.load(Ordering::Relaxed);
    let written = progress.written.load(Ordering::Relaxed);
    let dropped = progress.queue_dropped.load(Ordering::Relaxed);
    let receipt = Receipt {
        options,
        start_unix_seconds: started,
        end_unix_seconds: unix_seconds(),
        reason,
        source_closed,
        seen,
        written,
        bytes: segments.iter().map(|s| s.bytes).sum(),
        queue_dropped: dropped,
        unwritten: seen.saturating_sub(written).saturating_sub(dropped),
        driver_stats,
        analysis_skipped: progress.analysis_skipped.load(Ordering::Relaxed),
        analysis_failed: progress.analysis_failed.load(Ordering::Relaxed),
        display_skipped: progress.display_skipped.load(Ordering::Relaxed),
        segments,
    };
    new_json(&directory.join("capture-final.json"), &receipt)?;
    Ok(receipt)
}
