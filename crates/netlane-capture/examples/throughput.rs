//! Offline capture benchmark: production rotation, SHA-256, sync and analysis.
//! Never opens an adapter. Does not establish NIC/driver line-rate capacity.
use anyhow::{Result, ensure};
use netlane_capture::collection::{
    self, BatchPoll, DriverStats, Options, PacketBatch, Poll, Progress, Record, Source,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};
struct Traffic {
    start: Instant,
    generated: u64,
    rate: u64,
    frame: Vec<u8>,
    elapsed_ns: Arc<AtomicU64>,
}
impl Drop for Traffic {
    fn drop(&mut self) {
        self.elapsed_ns
            .store(self.start.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }
}
impl Source for Traffic {
    fn link_type(&self) -> u16 {
        1
    }
    fn stats(&mut self) -> Option<DriverStats> {
        None
    }
    fn poll(&mut self) -> Result<Poll> {
        let ready =
            (self.start.elapsed().as_nanos() * u128::from(self.rate) / 1_000_000_000) as u64;
        if self.generated >= ready {
            return Ok(Poll::Idle);
        }
        self.generated += 1;
        Ok(Poll::Packet(Record {
            seconds: 1_700_000_000 + (self.generated / self.rate) as u32,
            micros: (self.generated % self.rate * 1_000_000 / self.rate) as u32,
            original_len: self.frame.len() as u32,
            bytes: self.frame.clone(),
        }))
    }
    fn poll_batch(&mut self, batch: &mut PacketBatch) -> Result<BatchPoll> {
        // Model borrowed native packet storage: one copy into a pooled batch,
        // without a Vec allocation per packet. Arrival timing checked per batch.
        let ready =
            (self.start.elapsed().as_nanos() * u128::from(self.rate) / 1_000_000_000) as u64;
        while self.generated < ready && batch.has_room() {
            self.generated += 1;
            batch.push(
                1_700_000_000 + (self.generated / self.rate) as u32,
                (self.generated % self.rate * 1_000_000 / self.rate) as u32,
                self.frame.len() as u32,
                &self.frame,
            )?;
        }
        Ok(if self.generated >= ready {
            BatchPoll::Idle
        } else {
            BatchPoll::Ready
        })
    }
}
#[derive(Serialize)]
struct Measurement {
    synthetic: bool,
    nic_capacity_proven: bool,
    requested_pps: u64,
    captured_frame_bytes: u32,
    requested_payload_gbps: f64,
    capture_seconds: f64,
    total_seconds_including_analysis: f64,
    observed_pps: f64,
    published_pps: f64,
    published_payload_gbps: f64,
    published_pcap_gbps: f64,
    expected_arrivals_at_source_close: u64,
    source_backlog_not_polled: u64,
    requested_rate_sustained: bool,
    reconciled: bool,
    hashes_verified: bool,
    receipt: collection::Receipt,
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 4,
        "Use throughput PASTA_NOVA SEGUNDOS PPS BYTES (offline somente)."
    );
    let directory = PathBuf::from(&args[0]);
    let seconds: u64 = args[1].parse()?;
    let rate: u64 = args[2].parse()?;
    let bytes: u32 = args[3].parse()?;
    ensure!(
        (1..=60).contains(&seconds)
            && (1..=20_000_000).contains(&rate)
            && (64..=65535).contains(&bytes),
        "Carga inválida."
    );
    // Fictitious Ethernet/IPv4/UDP frame. Exact byte count; no snaplen reduction.
    let mut frame = vec![0xa5; bytes as usize];
    frame[12..14].copy_from_slice(&[0x08, 0x00]);
    frame[14] = 0x45;
    frame[16..18].copy_from_slice(&((bytes - 14) as u16).to_be_bytes());
    frame[20..22].copy_from_slice(&[0, 0]);
    frame[23] = 17;
    frame[26..34].copy_from_slice(&[192, 0, 2, 1, 192, 0, 2, 2]);
    frame[38..40].copy_from_slice(&((bytes - 34) as u16).to_be_bytes());
    let elapsed_ns = Arc::new(AtomicU64::new(0));
    let timer = elapsed_ns.clone();
    let config = Options {
        interface: "synthetic-throughput-no-network".into(),
        snaplen: bytes,
        segment_bytes: 128 * 1024 * 1024,
        segment_seconds: 60,
        total_bytes: 8 * 1024 * 1024 * 1024,
        duration_seconds: seconds,
        ..Options::default()
    };
    let (tx, rx) = mpsc::sync_channel(2);
    let consumer = std::thread::spawn(move || while rx.recv().is_ok() {});
    let start = Instant::now();
    let receipt = collection::run(
        directory.clone(),
        config,
        move || {
            Ok(Traffic {
                start: Instant::now(),
                generated: 0,
                rate,
                frame,
                elapsed_ns: timer,
            })
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(Progress::default()),
        tx,
    )?;
    consumer
        .join()
        .map_err(|_| anyhow::anyhow!("Consumer failed"))?;
    let total_seconds = start.elapsed().as_secs_f64();
    let capture_seconds = elapsed_ns.load(Ordering::Relaxed) as f64 / 1e9;
    ensure!(
        capture_seconds > 0.0 && receipt.source_closed && !receipt.reason.starts_with("erro:"),
        "Coleta falhou: {}",
        receipt.reason
    );
    let reconciled = receipt.seen == receipt.written + receipt.queue_dropped + receipt.unwritten
        && receipt.written == receipt.segments.iter().map(|s| s.packets).sum::<u64>()
        && receipt.bytes == receipt.segments.iter().map(|s| s.bytes).sum::<u64>();
    ensure!(reconciled, "Contadores não conciliados.");
    let mut buffer = vec![0; 1024 * 1024];
    for segment in &receipt.segments {
        let mut file = File::open(directory.join(&segment.file))?;
        ensure!(
            file.metadata()?.len() == segment.bytes,
            "Tamanho divergente."
        );
        let mut hash = Sha256::new();
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        ensure!(
            format!("{:X}", hash.finalize()) == segment.sha256,
            "SHA divergente."
        );
    }
    let result = Measurement {
        synthetic: true,
        nic_capacity_proven: false,
        requested_pps: rate,
        captured_frame_bytes: bytes,
        requested_payload_gbps: rate as f64 * f64::from(bytes) * 8.0 / 1e9,
        capture_seconds,
        total_seconds_including_analysis: total_seconds,
        observed_pps: receipt.seen as f64 / capture_seconds,
        published_pps: receipt.written as f64 / capture_seconds,
        published_payload_gbps: receipt.written as f64 * f64::from(bytes) * 8.0
            / capture_seconds
            / 1e9,
        published_pcap_gbps: receipt.bytes as f64 * 8.0 / capture_seconds / 1e9,
        expected_arrivals_at_source_close: (capture_seconds * rate as f64) as u64,
        source_backlog_not_polled: ((capture_seconds * rate as f64) as u64)
            .saturating_sub(receipt.seen),
        requested_rate_sustained: receipt.queue_dropped == 0
            && receipt.seen as f64 / capture_seconds >= rate as f64 * 0.99,
        reconciled,
        hashes_verified: true,
        receipt,
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("throughput.json"))?;
    serde_json::to_writer_pretty(&mut file, &result)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    println!(
        "{:.0} published pps, {:.3} Gbps payload; seen={}, written={}, dropped={}, unwritten={}",
        result.published_pps,
        result.published_payload_gbps,
        result.receipt.seen,
        result.receipt.written,
        result.receipt.queue_dropped,
        result.receipt.unwritten
    );
    Ok(())
}
