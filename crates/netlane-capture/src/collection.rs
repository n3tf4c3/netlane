//! Bounded capture pipeline. Sources are opened only by an explicit caller.
//! A blocked disk or analyzer never creates an unbounded packet backlog.
use crate::analysis::{Progress as AnalysisProgress, Scan, load_dump};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub const BATCH_PACKETS: usize = 256;
pub const BATCH_BYTES: usize = 256 * 1024;
pub const BATCH_QUEUE: usize = 32;
/// Upper bound including reader, writer and queued batches, even for tiny frames.
pub const PACKET_QUEUE: usize = (BATCH_QUEUE + 2) * BATCH_PACKETS;
pub const ANALYSIS_QUEUE: usize = 2;
pub const FINALIZATION_QUEUE: usize = 2;
pub const FINALIZATION_WORKERS: usize = 2;
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
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BatchPoll {
    Ready,
    Idle,
    End,
}
/// Reusable, bounded PCAP records. Native sources copy directly into this buffer,
/// before the next native read invalidates their borrowed packet memory.
pub struct PacketBatch {
    bytes: Vec<u8>,
    ends: Vec<usize>,
}
impl Default for PacketBatch {
    fn default() -> Self {
        Self {
            bytes: Vec::with_capacity(BATCH_BYTES + 16 + 65535),
            ends: Vec::with_capacity(BATCH_PACKETS),
        }
    }
}
impl PacketBatch {
    pub fn len(&self) -> usize {
        self.ends.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }
    pub fn has_room(&self) -> bool {
        self.len() < BATCH_PACKETS && self.bytes.len() < BATCH_BYTES
    }
    /// Conservative native dispatch count, allowing one final bounded record
    /// across the byte threshold without consuming packets that cannot fit.
    pub fn read_limit(&self, snaplen: u32) -> usize {
        if !self.has_room() {
            return 0;
        }
        ((BATCH_BYTES - self.bytes.len()) / 16usize.saturating_add(snaplen as usize))
            .max(1)
            .min(BATCH_PACKETS - self.len())
    }
    pub fn push(
        &mut self,
        seconds: u32,
        micros: u32,
        original_len: u32,
        bytes: &[u8],
    ) -> Result<()> {
        ensure!(
            self.has_room() && bytes.len() <= 65535,
            "Lote de captura excedeu o limite."
        );
        for value in [seconds, micros, bytes.len() as u32, original_len] {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
        self.bytes.extend_from_slice(bytes);
        self.ends.push(self.bytes.len());
        Ok(())
    }
    fn clear(&mut self) {
        self.bytes.clear();
        self.ends.clear();
    }
    fn range(&self, start: usize, end: usize) -> &[u8] {
        let offset = if start == 0 { 0 } else { self.ends[start - 1] };
        &self.bytes[offset..self.ends[end - 1]]
    }
}
#[derive(Clone, Copy, Default, Debug, Serialize)]
pub struct DriverStats {
    pub received: u32,
    pub dropped: u32,
    pub interface_dropped: u32,
}
#[derive(Debug, Serialize)]
pub struct DriverTotals {
    pub received: u64,
    pub dropped: u64,
    pub interface_dropped: u64,
    pub samples: u64,
}
/// poll must be non-blocking; the implementation owns its native handle.
pub trait Source {
    fn link_type(&self) -> u16;
    fn poll(&mut self) -> Result<Poll>;
    fn stats(&mut self) -> Option<DriverStats>;
    /// Default preserves existing sources. Leave consumed records in the batch
    /// on Idle, End or error; the reader accounts and forwards them exactly once.
    fn poll_batch(&mut self, batch: &mut PacketBatch) -> Result<BatchPoll> {
        while batch.has_room() {
            match self.poll()? {
                Poll::Packet(record) => batch.push(
                    record.seconds,
                    record.micros,
                    record.original_len,
                    &record.bytes,
                )?,
                Poll::Idle => return Ok(BatchPoll::Idle),
                Poll::End => return Ok(BatchPoll::End),
            }
        }
        Ok(BatchPoll::Ready)
    }
    /// Bounded idle wait; native sources can use their driver event instead.
    fn wait(&mut self, timeout: Duration) {
        thread::sleep(timeout);
    }
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
    /// Records accepted by the writer, including an unfinished segment.
    pub buffered: AtomicU64,
    /// Records in flushed, synced and published segments only.
    pub written: AtomicU64,
    /// Logical byte budget consumed, including the unfinished segment.
    pub bytes: AtomicU64,
    pub queue_dropped: AtomicU64,
    pub segments: AtomicU64,
    pub analysis_skipped: AtomicU64,
    pub analysis_failed: AtomicU64,
    pub display_skipped: AtomicU64,
    pub disk_write_nanos: AtomicU64,
    pub hash_nanos: AtomicU64,
    pub segment_sync_nanos: AtomicU64,
    pub hash_handoff_nanos: AtomicU64,
    pub reader_poll_nanos: AtomicU64,
    pub reader_wait_nanos: AtomicU64,
    pub reader_buffer_nanos: AtomicU64,
    driver_received: AtomicU64,
    driver_dropped: AtomicU64,
    driver_interface_dropped: AtomicU64,
    driver_samples: AtomicU64,
}
impl Progress {
    /// Approximate live snapshot; final values are read after joining the source.
    pub fn driver_totals(&self) -> Option<DriverTotals> {
        (self.driver_samples.load(Ordering::Relaxed) > 0).then(|| DriverTotals {
            received: self.driver_received.load(Ordering::Relaxed),
            dropped: self.driver_dropped.load(Ordering::Relaxed),
            interface_dropped: self.driver_interface_dropped.load(Ordering::Relaxed),
            samples: self.driver_samples.load(Ordering::Relaxed),
        })
    }
}
#[derive(Debug, Serialize)]
pub struct Performance {
    pub disk_write_nanos: u64,
    pub hash_nanos: u64,
    pub segment_sync_nanos: u64,
    pub hash_handoff_nanos: u64,
    pub reader_poll_nanos: u64,
    pub reader_wait_nanos: u64,
    pub reader_buffer_nanos: u64,
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
    pub buffered: u64,
    pub written: u64,
    pub bytes: u64,
    pub queue_dropped: u64,
    pub unwritten: u64,
    pub driver_stats: Option<DriverStats>,
    /// One-second samples extend native 32-bit counters across wraparound.
    pub driver_totals: Option<DriverTotals>,
    pub analysis_skipped: u64,
    pub analysis_failed: u64,
    pub display_skipped: u64,
    pub segments: Vec<Segment>,
    pub performance: Performance,
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
    packets: Receiver<PacketBatch>,
    recycle: SyncSender<PacketBatch>,
    link_type: u16,
}
fn sample_driver(
    progress: &Progress,
    previous: &mut Option<DriverStats>,
    sample: Option<DriverStats>,
) {
    if let Some(current) = sample {
        let last = previous.unwrap_or_default();
        for (counter, now, before) in [
            (&progress.driver_received, current.received, last.received),
            (&progress.driver_dropped, current.dropped, last.dropped),
            (
                &progress.driver_interface_dropped,
                current.interface_dropped,
                last.interface_dropped,
            ),
        ] {
            counter.fetch_add(u64::from(now.wrapping_sub(before)), Ordering::Relaxed);
        }
        progress.driver_samples.fetch_add(1, Ordering::Relaxed);
        *previous = Some(current);
    }
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
        let (tx, rx) = mpsc::sync_channel(BATCH_QUEUE);
        let (recycle, buffers) = mpsc::sync_channel(BATCH_QUEUE + 1);
        // Fixed pool: queued + writer + reader. Never allocate on saturation.
        for _ in 0..=BATCH_QUEUE {
            recycle.send(PacketBatch::default())?;
        }
        let reader_recycle = recycle.clone();
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
                let mut previous_stats = None;
                let mut stats_at = Instant::now();
                let result = (|| {
                    let mut batch = PacketBatch::default();
                    while !signal.load(Ordering::Relaxed) {
                        let began = Instant::now();
                        let state = source.poll_batch(&mut batch);
                        progress
                            .reader_poll_nanos
                            .fetch_add(began.elapsed().as_nanos() as u64, Ordering::Relaxed);
                        let count = batch.len() as u64;
                        if count > 0 {
                            progress.seen.fetch_add(count, Ordering::Relaxed);
                            // Reserve a replacement before transferring ownership.
                            // Never wait for the disk to return a pooled buffer.
                            let began = Instant::now();
                            let replacement = buffers.try_recv();
                            progress
                                .reader_buffer_nanos
                                .fetch_add(began.elapsed().as_nanos() as u64, Ordering::Relaxed);
                            if let Ok(replacement) = replacement {
                                match tx.try_send(batch) {
                                    Ok(()) => {
                                        batch = replacement;
                                    }
                                    Err(TrySendError::Full(mut returned)) => {
                                        progress.queue_dropped.fetch_add(count, Ordering::Relaxed);
                                        returned.clear();
                                        batch = returned;
                                        let _ = reader_recycle.try_send(replacement);
                                    }
                                    Err(TrySendError::Disconnected(_)) => break,
                                }
                            } else {
                                progress.queue_dropped.fetch_add(count, Ordering::Relaxed);
                                batch.clear();
                            }
                        }
                        match state? {
                            BatchPoll::Ready => {}
                            BatchPoll::Idle => {
                                let began = Instant::now();
                                source.wait(Duration::from_millis(1));
                                progress.reader_wait_nanos.fetch_add(
                                    began.elapsed().as_nanos() as u64,
                                    Ordering::Relaxed,
                                );
                            }
                            BatchPoll::End => break,
                        }
                        if stats_at.elapsed() >= Duration::from_secs(1) {
                            sample_driver(&progress, &mut previous_stats, source.stats());
                            stats_at = Instant::now();
                        }
                    }
                    Ok(())
                })();
                let stats = source.stats();
                sample_driver(&progress, &mut previous_stats, stats);
                drop(source);
                result.map(|()| stats)
            })?;
        let mut producer = Self {
            stop,
            thread: Some(thread),
            packets: rx,
            recycle,
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
/// Hash the exact bytes handed to the writer, concurrently with disk I/O.
/// Three recycled buffers bound each segment's additional memory to < 1 MiB.
struct StreamHash {
    input: Option<SyncSender<Vec<u8>>>,
    buffers: Receiver<Vec<u8>>,
    worker: Option<JoinHandle<[u8; 32]>>,
}
impl StreamHash {
    fn start(progress: Arc<Progress>) -> Result<Self> {
        let (input, blocks) = mpsc::sync_channel::<Vec<u8>>(2);
        let (recycle, buffers) = mpsc::sync_channel(3);
        for _ in 0..3 {
            recycle.send(Vec::with_capacity(BATCH_BYTES + 16 + 65535))?;
        }
        let worker = thread::Builder::new()
            .name("netlane-pcap-hash".into())
            .spawn(move || {
                let mut hash = Sha256::new();
                for mut block in blocks {
                    let began = Instant::now();
                    hash.update(&block);
                    progress
                        .hash_nanos
                        .fetch_add(began.elapsed().as_nanos() as u64, Ordering::Relaxed);
                    block.clear();
                    if recycle.send(block).is_err() {
                        break;
                    }
                }
                hash.finalize().into()
            })?;
        Ok(Self {
            input: Some(input),
            buffers,
            worker: Some(worker),
        })
    }
    fn update(&mut self, bytes: &[u8]) -> Result<()> {
        let mut buffer = self.buffers.recv().context("O cálculo SHA-256 falhou.")?;
        buffer.extend_from_slice(bytes);
        self.input
            .as_ref()
            .unwrap()
            .send(buffer)
            .map_err(|_| anyhow::anyhow!("O cálculo SHA-256 falhou."))
    }
    fn finish(&mut self) -> Result<[u8; 32]> {
        drop(self.input.take());
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| anyhow::anyhow!("O cálculo SHA-256 falhou."))
    }
}
impl Drop for StreamHash {
    fn drop(&mut self) {
        drop(self.input.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct Writer {
    index: usize,
    file: BufWriter<File>,
    hash: StreamHash,
    path: PathBuf,
    bytes: u64,
    packets: u64,
    started: Instant,
    progress: Arc<Progress>,
}
impl Writer {
    fn open(
        directory: &Path,
        index: usize,
        link: u16,
        snaplen: u32,
        progress: Arc<Progress>,
    ) -> Result<Self> {
        let path = directory.join(format!("segment-{index:04}.pcap.part"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut writer = Self {
            index,
            file: BufWriter::with_capacity(1024 * 1024, file),
            hash: StreamHash::start(progress.clone())?,
            path,
            bytes: 0,
            packets: 0,
            started: Instant::now(),
            progress,
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
        let began = Instant::now();
        let result = self.file.write_all(bytes);
        self.progress
            .disk_write_nanos
            .fetch_add(began.elapsed().as_nanos() as u64, Ordering::Relaxed);
        result?;
        let began = Instant::now();
        self.hash.update(bytes)?;
        self.progress
            .hash_handoff_nanos
            .fetch_add(began.elapsed().as_nanos() as u64, Ordering::Relaxed);
        self.bytes += bytes.len() as u64;
        Ok(())
    }
    fn packets(&mut self, records: &[u8], snaplen: u32) -> Result<()> {
        let mut offset = 0;
        let mut count = 0;
        while offset < records.len() {
            let header = &records[offset..offset + 16];
            let value = |start| u32::from_le_bytes(header[start..start + 4].try_into().unwrap());
            let captured = value(8);
            if !(value(4) < 1_000_000
                && captured > 0
                && captured <= snaplen
                && captured <= value(12))
            {
                // Preserve the valid prefix as buffered, never published on error.
                self.write(&records[..offset])?;
                self.packets += count;
                anyhow::bail!("Registro inválido na fonte de captura.");
            }
            offset += 16 + captured as usize;
            count += 1;
        }
        self.write(records)?;
        self.packets += count;
        Ok(())
    }
    fn close(mut self) -> Result<Segment> {
        let began = Instant::now();
        self.file.flush()?;
        self.file.get_ref().sync_all()?;
        self.progress
            .segment_sync_nanos
            .fetch_add(began.elapsed().as_nanos() as u64, Ordering::Relaxed);
        drop(self.file);
        let digest = self.hash.finish()?;
        let target = self.path.with_extension("");
        ensure!(!target.exists(), "O segmento de destino já existe.");
        std::fs::rename(&self.path, &target)?;
        Ok(Segment {
            file: target.file_name().unwrap().to_string_lossy().into_owned(),
            packets: self.packets,
            bytes: self.bytes,
            sha256: digest.iter().map(|byte| format!("{byte:02X}")).collect(),
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
    let (finalize_tx, finalize_rx) = mpsc::sync_channel::<Writer>(FINALIZATION_QUEUE);
    let final_progress = progress.clone();
    let final_stop = stop.clone();
    let finalizer = thread::Builder::new()
        .name("netlane-pcap-sync".into())
        .spawn(move || -> Result<_> {
            let queue = Arc::new(Mutex::new(finalize_rx));
            let failed = Arc::new(AtomicBool::new(false));
            let mut workers = Vec::new();
            let (done_tx, done_rx) = mpsc::sync_channel(FINALIZATION_WORKERS);
            for index in 0..FINALIZATION_WORKERS {
                let queue = queue.clone();
                let failed = failed.clone();
                let progress = final_progress.clone();
                let stop = final_stop.clone();
                let done = done_tx.clone();
                workers.push(
                    thread::Builder::new()
                        .name(format!("netlane-pcap-sync-{index}"))
                        .spawn(move || -> Result<_> {
                            loop {
                                let writer = match queue
                                    .lock()
                                    .map_err(|_| anyhow::anyhow!("Fila de publicação falhou."))?
                                    .recv()
                                {
                                    Ok(writer) => writer,
                                    Err(_) => break,
                                };
                                let index = writer.index;
                                let result = if failed.load(Ordering::Relaxed) {
                                    Err("Publicação interrompida após erro; arquivo .part preservado.".into())
                                } else { match writer.close() {
                                    Ok(segment) => {
                                        progress
                                            .written
                                            .fetch_add(segment.packets, Ordering::Relaxed);
                                        progress.segments.fetch_add(1, Ordering::Relaxed);
                                        Ok(segment)
                                    }
                                    Err(failure) => {
                                        failed.store(true, Ordering::Relaxed);
                                        stop.store(true, Ordering::Relaxed);
                                        Err(format!("{failure:#}"))
                                    }
                                }};
                                if done.send((index, result)).is_err() { break; }
                            }
                            Ok(())
                        })?,
                );
            }
            drop(done_tx);
            let mut published = Vec::new();
            let mut error = None;
            let mut pending = BTreeMap::new();
            let mut next = 1;
            for (index, result) in done_rx {
                pending.insert(index, result);
                // Publication can finish out of order. Deliver analysis in capture
                // order so an older segment cannot replace a newer live view.
                while let Some(result) = pending.remove(&next) {
                    match result {
                        Ok(segment) => {
                            if analysis_tx.try_send(segment.clone()).is_err() { final_progress.analysis_skipped.fetch_add(1, Ordering::Relaxed); }
                            published.push(segment);
                        }
                        Err(failure) => { if error.is_none() { error = Some(failure); } }
                    }
                    next += 1;
                }
            }
            // If a worker panicked, preserve metadata for every successful file.
            for segment in pending.into_values().flatten() {
                final_progress.analysis_skipped.fetch_add(1, Ordering::Relaxed);
                published.push(segment);
            }
            drop(analysis_tx);
            for worker in workers {
                match worker.join() {
                    Ok(Ok(())) => {}
                    _ => {
                        error = Some("A publicação dos segmentos falhou.".into());
                    }
                }
            }
            published.sort_by(|a, b| a.file.cmp(&b.file));
            Ok((published, error))
        })?;
    let close_segment = |writer: Writer, scheduled: &mut usize| -> Result<()> {
        finalize_tx
            .send(writer)
            .map_err(|_| anyhow::anyhow!("A publicação dos segmentos falhou."))?;
        *scheduled += 1;
        Ok(())
    };
    let mut scheduled = 0;
    let result = (|| {
        let mut source = Producer::start(factory, stop.clone(), progress.clone())?;
        let mut writer = Some(Writer::open(
            &directory,
            1,
            source.link_type,
            options.snaplen,
            progress.clone(),
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
                    close_segment(writer.take().unwrap(), &mut scheduled)?;
                }
                match source.packets.recv_timeout(Duration::from_millis(20)) {
                    Ok(mut batch) => {
                        let batch_result = (|| -> Result<bool> {
                            let mut index = 0;
                            while index < batch.len() {
                                if stop.load(Ordering::Relaxed) {
                                    reason = "parada solicitada".into();
                                    return Ok(false);
                                }
                                if began.elapsed().as_secs() >= options.duration_seconds {
                                    reason = "limite de duração".into();
                                    return Ok(false);
                                }
                                let size = batch.range(index, index + 1).len() as u64;
                                if writer.as_ref().is_some_and(|w| {
                                    w.bytes + size > options.segment_bytes && w.packets > 0
                                }) {
                                    close_segment(writer.take().unwrap(), &mut scheduled)?;
                                }
                                if writer.is_none() {
                                    if scheduled >= MAX_SEGMENTS {
                                        reason = "limite de segmentos".into();
                                        return Ok(false);
                                    }
                                    if progress.bytes.load(Ordering::Relaxed) + 24 + size
                                        > options.total_bytes
                                    {
                                        reason = "limite de disco".into();
                                        return Ok(false);
                                    }
                                    writer = Some(Writer::open(
                                        &directory,
                                        scheduled + 1,
                                        source.link_type,
                                        options.snaplen,
                                        progress.clone(),
                                    )?);
                                }
                                let w = writer.as_mut().unwrap();
                                let header = if w.packets == 0 { 24 } else { 0 };
                                let remaining = (options.segment_bytes - w.bytes).min(
                                    options.total_bytes.saturating_sub(
                                        progress.bytes.load(Ordering::Relaxed) + header,
                                    ),
                                );
                                if size > remaining {
                                    reason = "limite de disco".into();
                                    return Ok(false);
                                }
                                // Split only at packet boundaries. One write/hash and two
                                // progress updates for the whole contiguous prefix.
                                let mut end = index + 1;
                                while end < batch.len()
                                    && batch.range(index, end + 1).len() as u64 <= remaining
                                {
                                    end += 1;
                                }
                                let before = w.bytes;
                                let before_packets = w.packets;
                                let result = w.packets(batch.range(index, end), options.snaplen);
                                let accepted = w.packets - before_packets;
                                progress.bytes.fetch_add(
                                    w.bytes - before + if accepted > 0 { header } else { 0 },
                                    Ordering::Relaxed,
                                );
                                progress.buffered.fetch_add(accepted, Ordering::Relaxed);
                                result?;
                                index = end;
                            }
                            Ok(true)
                        })();
                        batch.clear();
                        let _ = source.recycle.try_send(batch);
                        if !batch_result? {
                            break;
                        }
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
            Ok(())
        })();
        let closed = source.finish();
        source_closed = true;
        if writing.is_ok()
            && let Some(writer) = writer.take()
        {
            close_segment(writer, &mut scheduled)?;
        }
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
    drop(finalize_tx);
    let finalization_error = match finalizer.join() {
        Ok(Ok((published, error))) => {
            segments = published;
            error
        }
        _ => Some("A publicação dos segmentos falhou.".into()),
    };
    if analyzer.join().is_err() {
        progress.analysis_failed.fetch_add(1, Ordering::Relaxed);
    }
    if let Err(error) = result {
        reason = format!("erro: {error:#}");
    } else if let Some(error) = finalization_error {
        reason = format!("erro: {error}");
    }
    let seen = progress.seen.load(Ordering::Relaxed);
    let written = segments.iter().map(|s| s.packets).sum();
    let dropped = progress.queue_dropped.load(Ordering::Relaxed);
    let receipt = Receipt {
        options,
        start_unix_seconds: started,
        end_unix_seconds: unix_seconds(),
        reason,
        source_closed,
        seen,
        buffered: progress.buffered.load(Ordering::Relaxed),
        written,
        bytes: segments.iter().map(|s| s.bytes).sum(),
        queue_dropped: dropped,
        unwritten: seen.saturating_sub(written).saturating_sub(dropped),
        driver_stats,
        driver_totals: progress.driver_totals(),
        analysis_skipped: progress.analysis_skipped.load(Ordering::Relaxed),
        analysis_failed: progress.analysis_failed.load(Ordering::Relaxed),
        display_skipped: progress.display_skipped.load(Ordering::Relaxed),
        performance: Performance {
            disk_write_nanos: progress.disk_write_nanos.load(Ordering::Relaxed),
            hash_nanos: progress.hash_nanos.load(Ordering::Relaxed),
            segment_sync_nanos: progress.segment_sync_nanos.load(Ordering::Relaxed),
            hash_handoff_nanos: progress.hash_handoff_nanos.load(Ordering::Relaxed),
            reader_poll_nanos: progress.reader_poll_nanos.load(Ordering::Relaxed),
            reader_wait_nanos: progress.reader_wait_nanos.load(Ordering::Relaxed),
            reader_buffer_nanos: progress.reader_buffer_nanos.load(Ordering::Relaxed),
        },
        segments,
    };
    new_json(&directory.join("capture-final.json"), &receipt)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_capture_extends_driver_counter_wrap_without_losing_the_previous_sample() {
        let progress = Progress::default();
        let mut previous = None;
        sample_driver(
            &progress,
            &mut previous,
            Some(DriverStats {
                received: u32::MAX - 3,
                dropped: u32::MAX - 1,
                interface_dropped: 5,
            }),
        );
        sample_driver(&progress, &mut previous, None);
        sample_driver(
            &progress,
            &mut previous,
            Some(DriverStats {
                received: 7,
                dropped: 4,
                interface_dropped: 9,
            }),
        );
        assert_eq!(
            progress.driver_received.load(Ordering::Relaxed),
            u64::from(u32::MAX) + 8
        );
        assert_eq!(
            progress.driver_dropped.load(Ordering::Relaxed),
            u64::from(u32::MAX) + 5
        );
        assert_eq!(progress.driver_interface_dropped.load(Ordering::Relaxed), 9);
        assert_eq!(progress.driver_samples.load(Ordering::Relaxed), 2);
    }
}
