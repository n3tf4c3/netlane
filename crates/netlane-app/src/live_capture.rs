//! Live capture for the Packets page: one explicit interface, packets delivered to
//! the table as they arrive. Diagnostic mode keeps them only in memory, within fixed
//! bounds, until the user saves. Long mode records every packet into rotating PCAP
//! segments on disk and the table shows a recent window.
use netlane_capture::collection::{
    self, BatchPoll, DriverStats, PacketBatch, Poll, Record, Source,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Bytes kept per packet; also the snaplen requested from Npcap.
pub const SNAPLEN: u32 = 2048;
pub const MAX_PACKETS: usize = 20_000;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
const BATCH: usize = 256;
const FLUSH: Duration = Duration::from_millis(100);

pub enum Event {
    Started {
        link_type: u16,
    },
    Packets(Vec<Record>),
    Finished {
        reason: String,
        driver: Option<DriverStats>,
    },
    Failed(String),
}

/// Long-capture durations offered in the toolbar, in minutes.
pub const DURATIONS: [u64; 6] = [15, 60, 4 * 60, 8 * 60, 24 * 60, 72 * 60];
pub fn duration_label(minutes: u64) -> String {
    if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{} h", minutes / 60)
    }
}

/// Where and how a long capture is being recorded.
pub struct Recording {
    pub directory: PathBuf,
    pub progress: Arc<collection::Progress>,
    pub minutes: u64,
}

pub struct LiveCapture {
    stop: Arc<AtomicBool>,
    pub events: Receiver<Event>,
    /// Packets discarded because the table fell behind the reader.
    pub queue_dropped: Arc<AtomicU64>,
    pub recording: Option<Recording>,
}
impl Drop for LiveCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl LiveCapture {
    /// The source is opened on the worker, which owns its native handle until it ends.
    pub fn start<S, F>(open: F) -> anyhow::Result<Self>
    where
        S: Source,
        F: FnOnce() -> anyhow::Result<S> + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let queue_dropped = Arc::new(AtomicU64::new(0));
        let (tx, events) = mpsc::sync_channel(64);
        let (worker_stop, worker_dropped) = (stop.clone(), queue_dropped.clone());
        thread::Builder::new()
            .name("netlane-live-capture".into())
            .spawn(move || run(open, &worker_stop, &worker_dropped, &tx))?;
        Ok(Self {
            stop,
            events,
            queue_dropped,
            recording: None,
        })
    }
    /// Long mode: the collection pipeline owns the source and writes every packet
    /// to `directory` (new, absolute); a copy of each packet feeds the table.
    pub fn record<S, F>(
        directory: PathBuf,
        options: collection::Options,
        open: F,
    ) -> anyhow::Result<Self>
    where
        S: Source + 'static,
        F: FnOnce() -> anyhow::Result<S> + Send + 'static,
    {
        options.validate()?;
        let minutes = options.duration_seconds / 60;
        let stop = Arc::new(AtomicBool::new(false));
        let queue_dropped = Arc::new(AtomicU64::new(0));
        let progress = Arc::new(collection::Progress::default());
        let (tx, events) = mpsc::sync_channel(64);
        let (worker_stop, worker_progress, worker_directory) =
            (stop.clone(), progress.clone(), directory.clone());
        let tee_dropped = queue_dropped.clone();
        thread::Builder::new()
            .name("netlane-long-capture".into())
            .spawn(move || {
                let tee_tx = tx.clone();
                let factory = move || {
                    let inner = open()?;
                    let _ = tee_tx.send(Event::Started {
                        link_type: inner.link_type(),
                    });
                    Ok(Tee {
                        inner,
                        tx: tee_tx,
                        dropped: tee_dropped,
                        pending: Vec::new(),
                        since: Instant::now(),
                    })
                };
                // Segment analyses are not shown while recording; dropping the
                // receiver only marks them as not displayed.
                let (segments, _) = mpsc::sync_channel(1);
                let event = match collection::run(
                    worker_directory,
                    options,
                    factory,
                    worker_stop,
                    worker_progress,
                    segments,
                ) {
                    Ok(receipt) => Event::Finished {
                        reason: format!(
                            "{} · {} pacotes gravados em {} arquivos PCAP.",
                            receipt.reason,
                            receipt.written,
                            receipt.segments.len()
                        ),
                        driver: receipt.driver_stats,
                    },
                    Err(e) => {
                        Event::Failed(format!("{e:#}. Arquivos já gravados foram preservados."))
                    }
                };
                let _ = tx.send(event);
            })?;
        Ok(Self {
            stop,
            events,
            queue_dropped,
            recording: Some(Recording {
                directory,
                progress,
                minutes,
            }),
        })
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run<S: Source>(
    open: impl FnOnce() -> anyhow::Result<S>,
    stop: &AtomicBool,
    dropped: &AtomicU64,
    tx: &SyncSender<Event>,
) {
    let mut source = match open() {
        Ok(source) => source,
        Err(e) => {
            let _ = tx.send(Event::Failed(format!("{e:#}")));
            return;
        }
    };
    if tx
        .send(Event::Started {
            link_type: source.link_type(),
        })
        .is_err()
    {
        return;
    }
    let mut batch = Vec::new();
    let mut since = Instant::now();
    let reason = loop {
        if stop.load(Ordering::Relaxed) {
            break "Captura interrompida.";
        }
        match source.poll() {
            Ok(Poll::Packet(record)) => {
                batch.push(record);
                if batch.len() >= BATCH || since.elapsed() >= FLUSH {
                    flush(&mut batch, tx, dropped);
                    since = Instant::now();
                }
            }
            Ok(Poll::Idle) => {
                flush(&mut batch, tx, dropped);
                since = Instant::now();
                source.wait(FLUSH);
            }
            Ok(Poll::End) => break "A interface encerrou a captura.",
            Err(e) => {
                flush(&mut batch, tx, dropped);
                let _ = tx.send(Event::Failed(format!("{e:#}")));
                return;
            }
        }
    };
    flush(&mut batch, tx, dropped);
    let driver = source.stats();
    drop(source);
    let _ = tx.send(Event::Finished {
        reason: reason.into(),
        driver,
    });
}

/// Never blocks the reader: a full queue discards the batch and counts it.
fn flush(batch: &mut Vec<Record>, tx: &SyncSender<Event>, dropped: &AtomicU64) {
    if batch.is_empty() {
        return;
    }
    let packets = std::mem::take(batch);
    let count = packets.len() as u64;
    if let Err(TrySendError::Full(_)) = tx.try_send(Event::Packets(packets)) {
        dropped.fetch_add(count, Ordering::Relaxed);
    }
}

/// Passes every record to the recorder unchanged and copies it for the table.
/// The copy never blocks the reader; a full table queue drops and counts.
struct Tee<S> {
    inner: S,
    tx: SyncSender<Event>,
    dropped: Arc<AtomicU64>,
    pending: Vec<Record>,
    since: Instant,
}
impl<S> Tee<S> {
    fn flush_if_due(&mut self, idle: bool) {
        if idle || self.pending.len() >= BATCH || self.since.elapsed() >= FLUSH {
            flush(&mut self.pending, &self.tx, &self.dropped);
            self.since = Instant::now();
        }
    }
}
impl<S: Source> Source for Tee<S> {
    fn link_type(&self) -> u16 {
        self.inner.link_type()
    }
    fn poll(&mut self) -> anyhow::Result<Poll> {
        let poll = self.inner.poll()?;
        match &poll {
            Poll::Packet(record) => {
                self.pending.push(record.clone());
                self.flush_if_due(false);
            }
            Poll::Idle | Poll::End => self.flush_if_due(true),
        }
        Ok(poll)
    }
    fn poll_batch(&mut self, batch: &mut PacketBatch) -> anyhow::Result<BatchPoll> {
        let before = batch.len();
        let state = self.inner.poll_batch(batch);
        for index in before..batch.len() {
            let (seconds, micros, original_len, bytes) = batch.record(index);
            self.pending.push(Record {
                seconds,
                micros,
                original_len,
                bytes: bytes.to_vec(),
            });
        }
        self.flush_if_due(!matches!(state, Ok(BatchPoll::Ready)));
        state
    }
    fn stats(&mut self) -> Option<DriverStats> {
        self.inner.stats()
    }
    fn wait(&mut self, timeout: Duration) {
        self.inner.wait(timeout);
    }
}

/// PCAP 2.4 global header with microsecond timestamps.
pub fn pcap_header(link_type: u16, snaplen: u32) -> Vec<u8> {
    let mut header = Vec::with_capacity(24);
    for value in [
        0xa1b2_c3d4u32,
        0x0004_0002,
        0,
        0,
        snaplen,
        u32::from(link_type),
    ] {
        header.extend(value.to_le_bytes());
    }
    header
}

/// Demonstration source: the built-in fictitious packets, re-stamped with the
/// current time, about twenty per second. Opens no socket, driver or file.
pub struct Synthetic {
    packets: Vec<Vec<u8>>,
    index: usize,
    next: Instant,
}
impl Synthetic {
    pub fn new() -> Self {
        Self {
            packets: netlane_capture::example()
                .packets
                .into_iter()
                .map(|p| p.bytes)
                .collect(),
            index: 0,
            next: Instant::now(),
        }
    }
}
impl Source for Synthetic {
    fn link_type(&self) -> u16 {
        1
    }
    fn poll(&mut self) -> anyhow::Result<Poll> {
        if Instant::now() < self.next {
            return Ok(Poll::Idle);
        }
        self.next += Duration::from_millis(50);
        let bytes = self.packets[self.index % self.packets.len()].clone();
        self.index += 1;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Ok(Poll::Packet(Record {
            seconds: now.as_secs() as u32,
            micros: now.subsec_micros(),
            original_len: bytes.len() as u32,
            bytes,
        }))
    }
    fn stats(&mut self) -> Option<DriverStats> {
        Some(DriverStats {
            received: self.index as u32,
            ..Default::default()
        })
    }
    fn wait(&mut self, timeout: Duration) {
        thread::sleep(
            self.next
                .saturating_duration_since(Instant::now())
                .min(timeout),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn synthetic_capture_streams_packets_and_stops_with_a_reason() {
        let live = LiveCapture::start(|| Ok(Synthetic::new())).unwrap();
        let mut received = 0;
        let mut started = false;
        let deadline = Instant::now() + Duration::from_secs(5);
        while received < 5 && Instant::now() < deadline {
            match live.events.recv_timeout(Duration::from_millis(200)) {
                Ok(Event::Started { link_type }) => started = link_type == 1,
                Ok(Event::Packets(packets)) => received += packets.len(),
                Ok(_) => panic!("captura terminou antes do pedido de parada"),
                Err(_) => {}
            }
        }
        assert!(started && received >= 5);
        live.stop();
        let finished = loop {
            match live.events.recv_timeout(Duration::from_secs(5)).unwrap() {
                Event::Finished { reason, driver } => break (reason, driver),
                Event::Packets(_) => {}
                _ => panic!("evento inesperado ao parar"),
            }
        };
        assert_eq!(finished.0, "Captura interrompida.");
        assert!(finished.1.is_some());
    }

    #[test]
    fn long_mode_records_segments_and_mirrors_packets_to_the_table() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("NetLane-teste");
        let options = collection::Options {
            interface: "synthetic".into(),
            duration_seconds: 15 * 60,
            ..collection::Options::default()
        };
        let live =
            LiveCapture::record(directory.clone(), options, || Ok(Synthetic::new())).unwrap();
        assert_eq!(live.recording.as_ref().unwrap().minutes, 15);
        let mut shown = 0;
        let deadline = Instant::now() + Duration::from_secs(5);
        while shown < 10 && Instant::now() < deadline {
            if let Ok(Event::Packets(packets)) =
                live.events.recv_timeout(Duration::from_millis(200))
            {
                shown += packets.len();
            }
        }
        assert!(shown >= 10);
        live.stop();
        let reason = loop {
            match live.events.recv_timeout(Duration::from_secs(10)).unwrap() {
                Event::Finished { reason, .. } => break reason,
                Event::Failed(e) => panic!("{e}"),
                _ => {}
            }
        };
        // The table mirrors packets as they are read; one read at the moment of
        // stopping may stay out of the files and is counted by the recorder.
        let progress = &live.recording.as_ref().unwrap().progress;
        let written = progress.written.load(Ordering::Relaxed);
        let seen = progress.seen.load(Ordering::Relaxed);
        assert!(seen as usize >= shown, "{seen} lidos, {shown} exibidos");
        assert!(
            written > 0 && written <= seen,
            "{written} gravados, {seen} lidos"
        );
        assert!(reason.contains("arquivos PCAP"), "{reason}");
        let segments: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|e| e == "pcap"))
            .collect();
        let total: u64 = segments
            .iter()
            .map(|path| {
                netlane_capture::load(path, &AtomicBool::new(false))
                    .unwrap()
                    .total_packets
            })
            .sum();
        assert!(!segments.is_empty());
        assert_eq!(total, written);
        assert!(directory.join("capture-final.json").exists());
    }

    #[test]
    fn open_failure_is_reported_without_starting() {
        let live = LiveCapture::start::<Synthetic, _>(|| anyhow::bail!("sem permissão")).unwrap();
        match live.events.recv_timeout(Duration::from_secs(5)).unwrap() {
            Event::Failed(message) => assert!(message.contains("sem permissão")),
            _ => panic!("falha de abertura não reportada"),
        }
    }

    #[test]
    fn header_and_records_form_a_readable_pcap() {
        let mut file = pcap_header(1, SNAPLEN);
        let bytes = netlane_capture::example().packets[0].bytes.clone();
        for value in [1_791_590_400u32, 5, bytes.len() as u32, bytes.len() as u32] {
            file.extend(value.to_le_bytes());
        }
        file.extend(&bytes);
        let capture = netlane_capture::read(Cursor::new(file), &AtomicBool::new(false)).unwrap();
        assert_eq!(capture.total_packets, 1);
        assert_eq!(capture.interfaces[0].link_type, 1);
        assert_eq!(capture.packets[0].bytes, bytes);
    }
}
