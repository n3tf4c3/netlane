use netlane_capture::{analysis::*, read};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read},
    sync::atomic::{AtomicBool, Ordering},
};

/// Generates a large dump in small records, never allocates the whole fixture.
struct Generated {
    record: Vec<u8>,
    position: usize,
    number: u64,
    count: u64,
    size: usize,
    deviation: bool,
}
impl Generated {
    fn new(count: u64, size: usize, deviation: bool) -> Self {
        let mut header = Vec::new();
        for value in [0xa1b2c3d4u32, 0x00040002, 0, 0, 65535, 1] {
            header.extend(value.to_le_bytes());
        }
        Self {
            record: header,
            position: 0,
            number: 0,
            count,
            size,
            deviation,
        }
    }
    fn next(&mut self) -> bool {
        if self.number == self.count {
            return false;
        }
        let i = self.number;
        self.number += 1;
        self.record.clear();
        self.position = 0;
        for value in [
            1_700_000_000 + i as u32,
            0,
            self.size as u32,
            self.size as u32,
        ] {
            self.record.extend(value.to_le_bytes());
        }
        let mut frame = vec![0; self.size];
        frame[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
        frame[14] = 0x45;
        frame[16..18].copy_from_slice(&((self.size - 14) as u16).to_be_bytes());
        frame[23] = 6;
        frame[26..30].copy_from_slice(&[192, 0, 2, 10]);
        let deviates = self.deviation && i >= self.count / 2;
        frame[30..34].copy_from_slice(&[198, 51, 100, if deviates { 99 } else { 20 }]);
        frame[34..36].copy_from_slice(&50000u16.to_be_bytes());
        frame[36..38].copy_from_slice(&443u16.to_be_bytes());
        frame[46] = 0x50;
        frame[47] = if deviates { 4 } else { 16 };
        self.record.extend(frame);
        true
    }
}
impl Read for Generated {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.position == self.record.len() && !self.next() {
            return Ok(0);
        }
        let n = out.len().min(self.record.len() - self.position);
        out[..n].copy_from_slice(&self.record[self.position..self.position + n]);
        self.position += n;
        Ok(n)
    }
}
#[test]
fn dump_over_128_mib_is_fully_analyzed_with_bounded_distributed_samples() {
    let progress = Progress::default();
    let stop = AtomicBool::new(false);
    let scan = read_dump(Generated::new(70_000, 2048, true), &stop, &progress).unwrap();
    assert!(scan.capture.file_bytes > netlane_capture::MAX_FILE_BYTES);
    assert_eq!(scan.report.packets, 70_000);
    assert_eq!(scan.report.tcp_resets, 35_000);
    assert_eq!(scan.report.captured_bytes, 70_000 * 2048);
    assert!(scan.capture.packets.len() <= MAX_SAMPLES);
    assert!(scan.capture.retained_bytes <= MAX_SAMPLES * SAMPLE_BYTES);
    assert!(scan.report.windows.len() <= MAX_WINDOWS);
    assert!(scan.report.window_seconds > 60);
    assert_eq!(
        scan.report.windows.iter().map(|w| w.packets).sum::<u64>(),
        70_000
    );
    assert_eq!(scan.capture.packets.first().unwrap().number, 1);
    assert_eq!(scan.capture.packets.last().unwrap().number, 70_000);
    assert!(
        scan.capture
            .packets
            .iter()
            .any(|p| p.number > 60_000 && p.number < 69_000)
    );
    assert!(
        scan.capture
            .packets
            .windows(2)
            .all(|w| w[0].number < w[1].number)
    );
    assert_eq!(progress.packets.load(Ordering::Relaxed), 70_000);
    assert_eq!(
        progress.bytes.load(Ordering::Relaxed),
        scan.capture.file_bytes
    );
    let mut source = Generated::new(70_000, 2048, true);
    let mut hash = Sha256::new();
    let mut bytes = [0; 8192];
    loop {
        let n = source.read(&mut bytes).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    assert_eq!(scan.capture.sha256, format!("{:X}", hash.finalize()));
}
#[test]
fn baseline_uses_all_records_and_references_late_deviations() {
    let stop = AtomicBool::new(false);
    let progress = Progress::default();
    let baseline = read_dump(Generated::new(100, 54, false), &stop, &progress).unwrap();
    let current = read_dump(Generated::new(1000, 54, true), &stop, &progress).unwrap();
    let findings = current.report.compare(&baseline.report);
    assert!(findings.iter().any(|f| f.kind == "tcp-rst"));
    let novel = findings.iter().find(|f| f.kind == "destino-novo").unwrap();
    assert_eq!(novel.packet_references, vec![501]);
    assert!(novel.description.contains("198.51.100.99"));
    assert!(baseline.report.compare(&baseline.report).is_empty());
    let repeated = read_dump(Generated::new(1000, 54, true), &stop, &Progress::default()).unwrap();
    assert_eq!(
        current
            .capture
            .packets
            .iter()
            .map(|p| p.number)
            .collect::<Vec<_>>(),
        repeated
            .capture
            .packets
            .iter()
            .map(|p| p.number)
            .collect::<Vec<_>>()
    );
    let mut incomplete = baseline.report.clone();
    incomplete.destination_overflow_packets = 1;
    assert!(
        !current
            .report
            .compare(&incomplete)
            .iter()
            .any(|f| f.kind == "destino-novo")
    );
    let mut short = baseline.report.clone();
    short.packets = 1;
    assert!(
        !current
            .report
            .compare(&short)
            .iter()
            .any(|f| f.kind == "destino-novo")
    );
}
#[test]
fn scan_cancellation_never_returns_a_complete_report() {
    struct CancelAfter<'a> {
        source: Generated,
        stop: &'a AtomicBool,
        reads: usize,
    }
    impl Read for CancelAfter<'_> {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            let n = self.source.read(b)?;
            if self.reads == 20 {
                self.stop.store(true, Ordering::Relaxed);
            }
            Ok(n)
        }
    }
    let stop = AtomicBool::new(false);
    let progress = Progress::default();
    let result = read_dump(
        CancelAfter {
            source: Generated::new(1000, 54, false),
            stop: &stop,
            reads: 0,
        },
        &stop,
        &progress,
    );
    assert!(result.is_err());
    assert!((1..1000).contains(&progress.packets.load(Ordering::Relaxed)));
}
#[test]
fn sparse_samples_keep_capture_lengths_and_payload_out_of_summaries() {
    let scan = read_dump(
        Generated::new(1000, 4096, true),
        &AtomicBool::new(false),
        &Progress::default(),
    )
    .unwrap();
    assert!(
        scan.capture
            .packets
            .iter()
            .all(|p| p.bytes.len() <= SAMPLE_BYTES && p.captured_len == 4096)
    );
    assert!(
        scan.summaries
            .iter()
            .all(|s| s.service == "https" && s.info.contains("bytes de dados"))
    );
    // The old API still keeps a contiguous prefix with exact packet bytes.
    let legacy = read(Generated::new(10, 4096, false), &AtomicBool::new(false)).unwrap();
    assert_eq!(legacy.packets[0].bytes.len(), 4096);
}
