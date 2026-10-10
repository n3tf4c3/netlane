//! Incremental, bounded analysis of an entire dump. Samples never define totals.
use crate::{
    CaptureFile, Interface, Packet, Summary,
    reader::{Limits, PacketSink, read_with},
};
use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BinaryHeap},
    fs::File,
    io::{BufReader, Read},
    path::Path,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

pub const MAX_DUMP_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub const MAX_DUMP_RECORDS: u64 = 100_000_000;
pub const MAX_SAMPLES: usize = 512;
pub const SAMPLE_BYTES: usize = 2048;
pub const MAX_DESTINATIONS: usize = 4096;
pub const MAX_WINDOWS: usize = 1024;

#[derive(Default)]
pub struct Progress {
    pub bytes: AtomicU64,
    pub packets: AtomicU64,
    pub resets: AtomicU64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Destination {
    pub packets: u64,
    pub first_packet: u64,
    pub file_offset: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Window {
    pub start_unix_seconds: String,
    pub packets: u64,
    pub bytes: u64,
    pub resets: u64,
    pub first_packet: u64,
    pub last_packet: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    pub sha256: String,
    pub packets: u64,
    pub captured_bytes: u64,
    pub timed_packets: u64,
    pub timed_bytes: u64,
    pub without_timestamp: u64,
    pub first_unix_nanos: Option<String>,
    pub last_unix_nanos: Option<String>,
    pub tcp_packets: u64,
    pub tcp_resets: u64,
    pub protocols: BTreeMap<String, u64>,
    pub destinations: BTreeMap<String, Destination>,
    pub destination_overflow_packets: u64,
    pub window_seconds: u64,
    pub windows: Vec<Window>,
    pub sample_packets: usize,
    pub sample_policy: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub kind: String,
    pub description: String,
    pub packet_references: Vec<u64>,
}
impl Report {
    pub fn duration_seconds(&self) -> Option<f64> {
        let first = self.first_unix_nanos.as_ref()?.parse::<i128>().ok()?;
        let last = self.last_unix_nanos.as_ref()?.parse::<i128>().ok()?;
        (last > first).then_some((last - first) as f64 / 1e9)
    }
    pub fn byte_rate(&self) -> Option<f64> {
        Some(self.timed_bytes as f64 / self.duration_seconds()?)
    }
    /// Explicit heuristics, not a learned detector or a verdict about an attack.
    pub fn compare(&self, baseline: &Self) -> Vec<Finding> {
        let mut findings = Vec::new();
        if let Some((rate, reference)) = self.byte_rate().zip(baseline.byte_rate())
            && self.timed_packets >= 20
            && baseline.timed_packets >= 20
            && reference > 0.
            && rate >= reference * 3.
        {
            findings.push(Finding { kind: "volume".into(), description: format!(
                "Taxa média de bytes capturados {:.1}× a referência (limiar 3×). Duração calculada entre primeiro e último horário observado.", rate/reference),
                packet_references: self.windows.first().map_or_else(Vec::new, |w|vec![w.first_packet, w.last_packet]) });
        }
        let share = self.tcp_resets as f64 / self.tcp_packets.max(1) as f64;
        let base_share = baseline.tcp_resets as f64 / baseline.tcp_packets.max(1) as f64;
        if self.tcp_packets >= 20
            && baseline.tcp_packets >= 20
            && share >= 0.1
            && share >= (base_share * 3.).max(base_share + 0.05)
        {
            findings.push(Finding {kind:"tcp-rst".into(),description:format!(
                "RST em {:.1}% dos pacotes TCP; referência {:.1}%. Pode indicar encerramentos/recusas; não identifica sozinho a causa.",share*100.,base_share*100.),
                packet_references:self.windows.iter().find(|w|w.resets>0).map_or_else(Vec::new,|w|vec![w.first_packet,w.last_packet])});
        }
        // An incomplete inventory cannot prove that a destination was absent.
        if baseline.packets >= 20 && baseline.destination_overflow_packets == 0 {
            for (destination, stat) in &self.destinations {
                if findings.len() >= 12 {
                    break;
                }
                if stat.packets >= 20 && !baseline.destinations.contains_key(destination) {
                    findings.push(Finding {kind:"destino-novo".into(),description:format!(
                        "Destino {destination} não observado na referência; {} pacotes no dump atual. Ausência na referência não comprova comportamento malicioso.",stat.packets),
                        packet_references:vec![stat.first_packet]});
                }
            }
        }
        findings
    }
    pub fn summary_text(&self, baseline: Option<&Self>) -> String {
        let mut text = format!(
            "Análise incremental do arquivo inteiro: {} pacotes / {} bytes capturados. {} amostras para visualização; {}. {} pacotes sem horário. Janelas de {} s (até {}). Inventário de destinos limitado a {}; {} pacotes no grupo excedente.\nProtocolos: {:?}. TCP: {} pacotes, {} com RST.\n",
            self.packets,
            self.captured_bytes,
            self.sample_packets,
            self.sample_policy,
            self.without_timestamp,
            self.window_seconds,
            MAX_WINDOWS,
            MAX_DESTINATIONS,
            self.destination_overflow_packets,
            self.protocols,
            self.tcp_packets,
            self.tcp_resets
        );
        if let Some(baseline) = baseline {
            text.push_str(&format!("Referência SHA-256 {}: {} pacotes. Filtro, ponto de coleta e duração podem ser diferentes; comparação heurística, não diagnóstico.\n",baseline.sha256,baseline.packets));
            for finding in self.compare(baseline) {
                text.push_str(&format!(
                    "{} · {} · pacotes {:?}\n",
                    finding.kind, finding.description, finding.packet_references
                ));
            }
        }
        text
    }
}
pub struct Scan {
    pub capture: CaptureFile,
    pub summaries: Vec<Summary>,
    pub report: Report,
}
struct Sample {
    packet: Packet,
    summary: Summary,
}
struct Analyzer<'a> {
    progress: &'a Progress,
    report: Report,
    windows: BTreeMap<i128, Window>,
    samples: BTreeMap<u64, Sample>,
    ranks: BinaryHeap<(u64, u64)>,
    first: Option<Sample>,
    last: Option<Sample>,
    first_time: Option<i128>,
    last_time: Option<i128>,
}
fn sample(packet: &Packet, summary: &Summary) -> Sample {
    let retained = Packet {
        number: packet.number,
        file_offset: packet.file_offset,
        interface: packet.interface,
        timestamp: packet.timestamp,
        original_len: packet.original_len,
        captured_len: packet.captured_len,
        bytes: packet.bytes[..packet.bytes.len().min(SAMPLE_BYTES)].to_vec(),
    };
    Sample {
        packet: retained,
        summary: summary.clone(),
    }
}
fn rank(mut n: u64) -> u64 {
    // Deterministic reservoir priorities; independent of payload/content.
    n = n.wrapping_add(0x9e3779b97f4a7c15);
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d049bb133111eb);
    n ^ (n >> 31)
}
fn host(endpoint: &str) -> String {
    if let Some(rest) = endpoint.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest).into()
    } else if endpoint.matches(':').count() == 1 {
        endpoint.split(':').next().unwrap_or(endpoint).into()
    } else {
        endpoint.into()
    }
}
fn merge(target: &mut Window, source: Window) {
    target.packets += source.packets;
    target.bytes += source.bytes;
    target.resets += source.resets;
    if target.first_packet == 0 || source.first_packet < target.first_packet {
        target.first_packet = source.first_packet;
    }
    target.last_packet = target.last_packet.max(source.last_packet);
}
impl PacketSink for Analyzer<'_> {
    fn progress(&mut self, bytes: u64) {
        self.progress.bytes.store(bytes, Ordering::Relaxed);
    }
    fn packet(&mut self, p: &Packet, interface: &Interface) -> Result<()> {
        let s = p.summary(interface.link_type);
        let reset = s.protocol == "TCP" && s.info.split_whitespace().any(|f| f == "RST");
        self.report.packets += 1;
        self.report.captured_bytes += u64::from(p.captured_len);
        self.report.tcp_packets += u64::from(s.protocol == "TCP");
        self.report.tcp_resets += u64::from(reset);
        *self.report.protocols.entry(s.protocol.into()).or_default() += 1;
        let destination = host(&s.destination);
        if destination != "—" {
            if self.report.destinations.contains_key(&destination)
                || self.report.destinations.len() < MAX_DESTINATIONS
            {
                let stat = self
                    .report
                    .destinations
                    .entry(destination)
                    .or_insert(Destination {
                        first_packet: p.number,
                        file_offset: p.file_offset,
                        ..Destination::default()
                    });
                stat.packets += 1;
            } else {
                self.report.destination_overflow_packets += 1;
            }
        }
        if let Some(time) = p.timestamp {
            let ns = time.unix_nanos();
            self.first_time = Some(self.first_time.map_or(ns, |first| first.min(ns)));
            self.last_time = Some(self.last_time.map_or(ns, |last| last.max(ns)));
            self.report.timed_packets += 1;
            self.report.timed_bytes += u64::from(p.captured_len);
            let key = ns.div_euclid(i128::from(self.report.window_seconds) * 1_000_000_000);
            let window = self.windows.entry(key).or_default();
            merge(
                window,
                Window {
                    packets: 1,
                    bytes: u64::from(p.captured_len),
                    resets: u64::from(reset),
                    first_packet: p.number,
                    last_packet: p.number,
                    ..Window::default()
                },
            );
            while self.windows.len() > MAX_WINDOWS {
                self.report.window_seconds = self
                    .report
                    .window_seconds
                    .checked_mul(2)
                    .context("Intervalo temporal excessivo")?;
                let mut coarser = BTreeMap::new();
                for (key, window) in std::mem::take(&mut self.windows) {
                    merge(coarser.entry(key.div_euclid(2)).or_default(), window);
                }
                self.windows = coarser;
            }
        } else {
            self.report.without_timestamp += 1;
        }
        if self.first.is_none() {
            self.first = Some(sample(p, &s));
        }
        self.last = Some(sample(p, &s));
        let priority = (rank(p.number), p.number);
        if self.samples.len() < MAX_SAMPLES - 2
            || self.ranks.peek().is_some_and(|largest| priority < *largest)
        {
            if self.samples.len() >= MAX_SAMPLES - 2 {
                let (_, number) = self.ranks.pop().unwrap();
                self.samples.remove(&number);
            }
            self.ranks.push(priority);
            self.samples.insert(p.number, sample(p, &s));
        }
        self.progress
            .packets
            .store(self.report.packets, Ordering::Relaxed);
        self.progress
            .resets
            .store(self.report.tcp_resets, Ordering::Relaxed);
        Ok(())
    }
}
pub fn read_dump<R: Read>(reader: R, cancel: &AtomicBool, progress: &Progress) -> Result<Scan> {
    let mut analyzer=Analyzer {progress,report:Report {window_seconds:60,sample_policy:"prioridades determinísticas distribuídas + primeiro/último; até 2048 bytes por amostra".into(),..Report::default()},
        windows:BTreeMap::new(),samples:BTreeMap::new(),ranks:BinaryHeap::new(),first:None,last:None,first_time:None,last_time:None};
    let mut capture = read_with(
        reader,
        cancel,
        Limits {
            bytes: MAX_DUMP_BYTES,
            records: MAX_DUMP_RECORDS,
            retain: false,
        },
        Some(&mut analyzer),
    )?;
    for value in [analyzer.first.take(), analyzer.last.take()]
        .into_iter()
        .flatten()
    {
        analyzer.samples.insert(value.packet.number, value);
    }
    analyzer.report.sha256 = capture.sha256.clone();
    analyzer.report.sample_packets = analyzer.samples.len();
    analyzer.report.first_unix_nanos = analyzer.first_time.map(|n| n.to_string());
    analyzer.report.last_unix_nanos = analyzer.last_time.map(|n| n.to_string());
    analyzer.report.windows = analyzer
        .windows
        .into_iter()
        .map(|(key, mut window)| {
            window.start_unix_seconds =
                (key * i128::from(analyzer.report.window_seconds)).to_string();
            window
        })
        .collect();
    let mut summaries = Vec::new();
    for sample in analyzer.samples.into_values() {
        capture.retained_bytes += sample.packet.bytes.len();
        capture.packets.push(sample.packet);
        summaries.push(sample.summary);
    }
    capture.omitted_packets = capture.total_packets - capture.packets.len() as u64;
    Ok(Scan {
        capture,
        summaries,
        report: analyzer.report,
    })
}
pub fn load_dump(path: &Path, cancel: &AtomicBool, progress: &Progress) -> Result<Scan> {
    let file = File::open(path).context("Não foi possível abrir o dump")?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= MAX_DUMP_BYTES,
        "Escolha um dump comum de até 64 GiB."
    );
    let scan = read_dump(BufReader::new(&file), cancel, progress)?;
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len()
            && after.len() == scan.capture.file_bytes
            && before.modified().ok() == after.modified().ok(),
        "O dump mudou durante a análise. Use uma coleta fechada."
    );
    Ok(scan)
}
