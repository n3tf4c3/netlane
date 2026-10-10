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
