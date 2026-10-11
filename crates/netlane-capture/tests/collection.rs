use netlane_capture::{
    analysis::{Progress as AnalysisProgress, load_dump},
    collection::*,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
struct Fake {
    remaining: u32,
    index: u32,
    slow: bool,
    closed: Arc<AtomicBool>,
    fail: bool,
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}
impl Source for Fake {
    fn link_type(&self) -> u16 {
        1
    }
    fn poll(&mut self) -> anyhow::Result<Poll> {
        if self.remaining == 0 {
            if self.fail {
                anyhow::bail!("synthetic source error");
            }
            return Ok(Poll::End);
        }
        if self.slow {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.remaining -= 1;
        self.index += 1;
        let mut bytes = netlane_capture::example().packets.remove(0).bytes;
        bytes.resize(128, 0);
        Ok(Poll::Packet(Record {
            seconds: 1_700_000_000 + self.index,
            micros: 123,
            original_len: 128,
            bytes,
        }))
    }
    fn stats(&mut self) -> Option<DriverStats> {
        Some(DriverStats {
            received: self.index,
            dropped: 3,
            interface_dropped: 0,
        })
    }
}
fn options() -> Options {
    Options {
        interface: "synthetic-no-network".into(),
        snaplen: 128,
        segment_bytes: 4096,
        total_bytes: 256 * 1024,
        duration_seconds: 60,
        ..Options::default()
    }
}

#[test]
fn borrowed_batches_preserve_every_byte_and_timestamp_across_rotation() {
    struct Borrowed {
        index: u32,
    }
    impl Source for Borrowed {
        fn link_type(&self) -> u16 {
            1
        }
        fn stats(&mut self) -> Option<DriverStats> {
            None
        }
        fn poll(&mut self) -> anyhow::Result<Poll> {
            panic!("must use the bulk adapter")
        }
        fn poll_batch(&mut self, batch: &mut PacketBatch) -> anyhow::Result<BatchPoll> {
            while self.index < 600 && batch.has_room() {
                let size = if self.index.is_multiple_of(3) {
                    9000
                } else {
                    128
                };
                batch.push(
                    1_700_000_000,
                    self.index,
                    size + 17,
                    &vec![self.index as u8; size as usize],
                )?;
                self.index += 1;
            }
            Ok(if self.index == 600 {
                BatchPoll::End
            } else {
                BatchPoll::Ready
            })
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("borrowed");
    let mut config = options();
    config.snaplen = 9000;
    config.segment_bytes = 256 * 1024;
    config.total_bytes = 4 * 1024 * 1024;
    let (tx, rx) = mpsc::sync_channel(2);
    let consumer = std::thread::spawn(move || while rx.recv().is_ok() {});
    let receipt = run(
        directory.clone(),
        config.clone(),
        || Ok(Borrowed { index: 0 }),
        Arc::new(AtomicBool::new(false)),
        Arc::new(Progress::default()),
        tx,
    )
    .unwrap();
    consumer.join().unwrap();
    assert_eq!(receipt.written, 600);
    assert_eq!(receipt.queue_dropped + receipt.unwritten, 0);
    assert!(receipt.source_closed && !receipt.reason.starts_with("erro:"));
    let mut index = 0u32;
    for (number, segment) in receipt.segments.iter().enumerate() {
        assert_eq!(segment.file, format!("segment-{:04}.pcap", number + 1));
        assert!(segment.bytes <= config.segment_bytes);
        let dump =
            netlane_capture::load(&directory.join(&segment.file), &AtomicBool::new(false)).unwrap();
        assert_eq!(dump.sha256, segment.sha256);
        for packet in dump.packets {
            let size = if index.is_multiple_of(3) { 9000 } else { 128 };
            assert_eq!(packet.bytes, vec![index as u8; size]);
            assert_eq!(packet.original_len, size as u32 + 17);
            assert_eq!(
                packet.timestamp.unwrap().unix_nanos(),
                1_700_000_000_000_000_000 + i128::from(index) * 1000
            );
            index += 1;
        }
    }
    assert_eq!(index, 600);
}

struct InvalidRecord {
    index: u32,
}
impl Source for InvalidRecord {
    fn link_type(&self) -> u16 {
        1
    }
    fn stats(&mut self) -> Option<DriverStats> {
        None
    }
    fn poll(&mut self) -> anyhow::Result<Poll> {
        self.index += 1;
        if self.index > 2 {
            return Ok(Poll::End);
        }
        Ok(Poll::Packet(Record {
            seconds: 1_700_000_000,
            micros: if self.index == 2 { 1_000_000 } else { 0 },
            original_len: 128,
            bytes: vec![0; 128],
        }))
    }
}

#[test]
fn invalid_record_does_not_publish_buffered_packets_as_saved() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("invalid");
    let progress = Arc::new(Progress::default());
    let (tx, _) = mpsc::sync_channel(1);
    let receipt = run(
        dir.clone(),
        options(),
        || Ok(InvalidRecord { index: 0 }),
        Arc::new(AtomicBool::new(false)),
        progress.clone(),
        tx,
    )
    .unwrap();
    assert!(receipt.reason.contains("Registro inválido"));
    assert_eq!(receipt.buffered, 1);
    assert_eq!(receipt.written, 0);
    assert_eq!(receipt.unwritten, 2);
    assert_eq!(receipt.bytes, 0);
    assert!(receipt.segments.is_empty());
    assert!(receipt.source_closed && dir.join("segment-0001.pcap.part").exists());
    assert_eq!(progress.written.load(Ordering::Relaxed), 0);
}

#[test]
fn finalization_collision_preserves_previous_file_and_counts_only_published_segments() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("collision");
    let target = dir.join("segment-0002.pcap");
    let collision = target.clone();
    let (tx, _) = mpsc::sync_channel(1);
    let mut config = options();
    config.segment_bytes = 184; // one 128-byte packet per segment
    let receipt = run(
        dir.clone(),
        config,
        move || {
            std::fs::write(collision, b"existing evidence")?;
            Ok(Fake {
                remaining: 2,
                index: 0,
                slow: true,
                closed: Arc::new(AtomicBool::new(false)),
                fail: false,
            })
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(Progress::default()),
        tx,
    )
    .unwrap();
    assert!(receipt.reason.contains("destino já existe"));
    assert_eq!(receipt.buffered, 2);
    assert_eq!(receipt.written, 1);
    assert_eq!(receipt.unwritten, 1);
    assert_eq!(receipt.segments.len(), 1);
    assert_eq!(receipt.bytes, receipt.segments[0].bytes);
    assert_eq!(std::fs::read(target).unwrap(), b"existing evidence");
    assert!(dir.join("segment-0002.pcap.part").exists());
}

#[cfg(windows)]
#[test]
fn flush_failure_never_claims_buffered_packets_are_saved() {
    use std::{fs::File, os::windows::io::AsRawHandle, sync::Mutex};
    use windows_sys::Win32::{
        Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx},
        System::IO::OVERLAPPED,
    };
    struct LockedSource {
        directory: PathBuf,
        lock: Arc<Mutex<Option<File>>>,
        index: u32,
    }
    impl Source for LockedSource {
        fn link_type(&self) -> u16 {
            1
        }
        fn stats(&mut self) -> Option<DriverStats> {
            None
        }
        fn poll(&mut self) -> anyhow::Result<Poll> {
            let mut held = self.lock.lock().unwrap();
            if held.is_none() {
                let path = self.directory.join("segment-0001.pcap.part");
                if !path.exists() {
                    return Ok(Poll::Idle);
                }
                let file = File::options().read(true).write(true).open(path)?;
                // SAFETY: a live temporary file handle and initialized OVERLAPPED; synchronous lock.
                let locked = unsafe {
                    LockFileEx(
                        file.as_raw_handle(),
                        LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                        0,
                        u32::MAX,
                        u32::MAX,
                        &mut OVERLAPPED::default(),
                    )
                };
                if locked == 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                *held = Some(file);
            }
            if self.index == 2 {
                return Ok(Poll::Idle);
            }
            self.index += 1;
            Ok(Poll::Packet(Record {
                seconds: 1_700_000_000,
                micros: 0,
                original_len: 128,
                bytes: vec![0; 128],
            }))
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("locked");
    let source_dir = dir.clone();
    // External lock outlives the source, including asynchronous publication.
    let lock = Arc::new(Mutex::new(None));
    let source_lock = lock.clone();
    let mut config = options();
    config.duration_seconds = 1;
    let (tx, _) = mpsc::sync_channel(1);
    let receipt = run(
        dir.clone(),
        config,
        move || {
            Ok(LockedSource {
                directory: source_dir,
                lock: source_lock,
                index: 0,
            })
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(Progress::default()),
        tx,
    )
    .unwrap();
    assert!(receipt.reason.starts_with("erro:"));
    assert_eq!(receipt.seen, 2);
    assert_eq!(receipt.buffered, 2);
    assert_eq!(receipt.written, 0);
    assert_eq!(receipt.unwritten, 2);
    assert_eq!(receipt.bytes, 0);
    assert!(receipt.source_closed && receipt.segments.is_empty());
    assert!(dir.join("segment-0001.pcap.part").exists());
}
#[test]
fn rotating_capture_files_are_complete_hashed_and_analyzed_without_overwriting() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("capture");
    let closed = Arc::new(AtomicBool::new(false));
    let flag = closed.clone();
    let (tx, rx) = mpsc::sync_channel(4);
    let progress = Arc::new(Progress::default());
    let receipt = run(
        dir.clone(),
        options(),
        move || {
            Ok(Fake {
                remaining: 300,
                index: 0,
                slow: true,
                closed: flag,
                fail: false,
            })
        },
        Arc::new(AtomicBool::new(false)),
        progress.clone(),
        tx,
    )
    .unwrap();
    assert!(closed.load(Ordering::Relaxed));
    assert!(receipt.source_closed);
    assert_eq!(receipt.written, 300);
    assert_eq!(receipt.queue_dropped, 0);
    assert_eq!(
        receipt.bytes,
        receipt.segments.iter().map(|s| s.bytes).sum::<u64>()
    );
    assert!(receipt.segments.len() > 1);
    assert_eq!(
        receipt.seen,
        receipt.written + receipt.queue_dropped + receipt.unwritten
    );
    let mut records = 0;
    for segment in &receipt.segments {
        assert!(segment.bytes <= options().segment_bytes);
        let dump = load_dump(
            &dir.join(&segment.file),
            &AtomicBool::new(false),
            &AnalysisProgress::default(),
        )
        .unwrap();
        assert_eq!(dump.report.sha256, segment.sha256);
        assert_eq!(dump.report.packets, segment.packets);
        records += segment.packets;
    }
    assert_eq!(records, 300);
    let scans: Vec<_> = rx.try_iter().collect();
    assert!(!scans.is_empty());
    assert!(scans.len() <= 4);
    assert_eq!(receipt.analysis_failed, 0);
    assert_eq!(
        scans.len() as u64 + receipt.display_skipped + receipt.analysis_skipped,
        receipt.segments.len() as u64
    );
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("capture-final.json")).unwrap()).unwrap();
    assert_eq!(saved["written"], 300);
    let original = std::fs::read(dir.join("capture-final.json")).unwrap();
    let (tx, _) = mpsc::sync_channel(1);
    assert!(
        run::<Fake>(
            dir.clone(),
            options(),
            || panic!("must not reopen"),
            Arc::new(AtomicBool::new(false)),
            Arc::new(Progress::default()),
            tx
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(dir.join("capture-final.json")).unwrap(),
        original
    );
}
#[test]
fn disk_limit_and_backpressure_account_for_every_seen_record_and_close_source() {
    let temp = tempfile::tempdir().unwrap();
    let closed = Arc::new(AtomicBool::new(false));
    let flag = closed.clone();
    let (tx, _) = mpsc::sync_channel(1);
    let mut config = options();
    config.total_bytes = 8192;
    let receipt = run(
        temp.path().join("limited"),
        config.clone(),
        move || {
            Ok(Fake {
                remaining: 20_000,
                index: 0,
                slow: false,
                closed: flag,
                fail: false,
            })
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(Progress::default()),
        tx,
    )
    .unwrap();
    assert_eq!(receipt.reason, "limite de disco");
    assert!(receipt.bytes <= config.total_bytes);
    assert!(closed.load(Ordering::Relaxed));
    assert!(receipt.source_closed);
    assert_eq!(
        receipt.seen,
        receipt.written + receipt.queue_dropped + receipt.unwritten
    );
    assert!(receipt.written < 20_000);
}
#[test]
fn source_errors_keep_closed_segments_and_cancellation_never_opens_a_source() {
    let temp = tempfile::tempdir().unwrap();
    let closed = Arc::new(AtomicBool::new(false));
    let flag = closed.clone();
    let (tx, _) = mpsc::sync_channel(1);
    let dir = temp.path().join("failed");
    let receipt = run(
        dir.clone(),
        options(),
        move || {
            Ok(Fake {
                remaining: 2,
                index: 0,
                slow: true,
                closed: flag,
                fail: true,
            })
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(Progress::default()),
        tx,
    )
    .unwrap();
    assert!(receipt.reason.contains("synthetic source error"));
    assert!(receipt.source_closed && closed.load(Ordering::Relaxed));
    assert_eq!(receipt.segments[0].packets, 2);
    assert!(dir.join("capture-final.json").exists());
    let (tx, _) = mpsc::sync_channel(1);
    let target = temp.path().join("cancelled");
    assert!(
        run::<Fake>(
            target.clone(),
            options(),
            || panic!("cancelled must not open"),
            Arc::new(AtomicBool::new(true)),
            Arc::new(Progress::default()),
            tx
        )
        .is_err()
    );
    assert!(!target.exists());
    let (tx, _) = mpsc::sync_channel(1);
    assert!(
        run::<Fake>(
            PathBuf::from("relative"),
            options(),
            || panic!("invalid"),
            Arc::new(AtomicBool::new(false)),
            Arc::new(Progress::default()),
            tx
        )
        .is_err()
    );
}
#[test]
fn stop_while_idle_closes_source_and_finishes_without_waiting_for_a_packet() {
    struct Idle(Arc<AtomicBool>);
    impl Drop for Idle {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }
    impl Source for Idle {
        fn link_type(&self) -> u16 {
            1
        }
        fn poll(&mut self) -> anyhow::Result<Poll> {
            Ok(Poll::Idle)
        }
        fn stats(&mut self) -> Option<DriverStats> {
            None
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let closed = Arc::new(AtomicBool::new(false));
    let flag = closed.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = stop.clone();
    let (tx, _) = mpsc::sync_channel(1);
    let clock = Instant::now();
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(75));
        cancel.store(true, Ordering::Relaxed);
    });
    let result = run(
        temp.path().join("idle"),
        options(),
        move || Ok(Idle(flag)),
        stop,
        Arc::new(Progress::default()),
        tx,
    )
    .unwrap();
    stopper.join().unwrap();
    assert_eq!(result.reason, "parada solicitada");
    assert!(result.source_closed && closed.load(Ordering::Relaxed));
    assert!(clock.elapsed() < Duration::from_secs(3));
    assert_eq!(result.segments[0].packets, 0);
}
