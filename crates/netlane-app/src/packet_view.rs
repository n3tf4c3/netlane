//! Local import. Only selected metadata can enter an explicitly reviewed context.
use crate::{
    codex_view::{CodexView, bundle::InvestigationRestored},
    collection_view::{CollectionView, SegmentReady},
    live_capture::{self, LiveCapture},
    scrollbar::ScrollbarExt,
    settings::AppSettings,
    text_input::TextInput,
    theme::{ActiveTheme, MONO_FONT},
    ui::{Tip, icon, icon_button},
};
use gpui::{
    App, Context, Entity, FontWeight, Render, Subscription, UniformListScrollHandle, Window, div,
    prelude::*, px, uniform_list,
};
use netlane_capture::analysis::{Progress, Report, Scan};
use netlane_capture::{CaptureFile, Summary, collection::Record, lookup::RecoveredPacket};
use netlane_windows::npcap::Device;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Inspection {
    capture: CaptureFile,
    summaries: Vec<Summary>,
    search: Vec<String>,
    report: Option<Report>,
    collection_note: Option<String>,
    source: Option<std::path::PathBuf>,
    sample_count: usize,
}
impl Inspection {
    #[cfg(test)]
    fn new(capture: CaptureFile) -> Self {
        let summaries: Vec<_> = capture
            .packets
            .iter()
            .map(|p| p.summary(capture.interfaces[p.interface].link_type))
            .collect();
        let search = summaries
            .iter()
            .zip(&capture.packets)
            .map(|(s, p)| {
                format!(
                    "{} {} {} {} {} {}",
                    p.number,
                    s.protocol,
                    s.source,
                    s.destination,
                    s.info,
                    capture.interfaces[p.interface].name
                )
                .to_lowercase()
            })
            .collect();
        Self {
            sample_count: capture.packets.len(),
            capture,
            summaries,
            search,
            report: None,
            collection_note: None,
            source: None,
        }
    }
    fn scan(scan: Scan) -> Self {
        let search = scan
            .summaries
            .iter()
            .zip(&scan.capture.packets)
            .map(|(s, p)| {
                format!(
                    "{} {} {} {} {} {} {}",
                    p.number,
                    s.protocol,
                    s.source,
                    s.destination,
                    s.info,
                    s.service,
                    scan.capture.interfaces[p.interface].name
                )
                .to_lowercase()
            })
            .collect();
        Self {
            sample_count: scan.capture.packets.len(),
            capture: scan.capture,
            summaries: scan.summaries,
            search,
            report: Some(scan.report),
            collection_note: None,
            source: scan.source,
        }
    }
    /// Empty live capture; packets are appended as the worker delivers them.
    fn live(link_type: u16, interface: String) -> Self {
        Self {
            capture: CaptureFile {
                format: netlane_capture::Format::Pcap,
                sha256: String::new(),
                file_bytes: 24,
                interfaces: vec![netlane_capture::Interface {
                    section: 0,
                    section_id: 0,
                    link_type,
                    snaplen: live_capture::SNAPLEN,
                    name: interface,
                    resolution: 6,
                    offset_seconds: 0,
                }],
                packets: Vec::new(),
                total_packets: 0,
                omitted_packets: 0,
                truncated_packets: 0,
                auxiliary_blocks: 0,
                retained_bytes: 0,
            },
            summaries: Vec::new(),
            search: Vec::new(),
            report: None,
            collection_note: None,
            source: None,
            sample_count: 0,
        }
    }
    /// Appends one record to the table and to the PCAP image `file`.
    /// Returns false, without changes, once the memory bounds are reached.
    fn push_live(&mut self, record: Record, file: &mut Vec<u8>) -> bool {
        if self.capture.packets.len() >= live_capture::MAX_PACKETS
            || self.capture.retained_bytes + record.bytes.len() > live_capture::MAX_BYTES
        {
            return false;
        }
        let file_offset = file.len() as u64;
        for value in [
            record.seconds,
            record.micros,
            record.bytes.len() as u32,
            record.original_len,
        ] {
            file.extend(value.to_le_bytes());
        }
        file.extend(&record.bytes);
        self.append(record, file_offset);
        self.capture.file_bytes = file.len() as u64;
        true
    }
    /// Long mode: the full capture is on disk, so the table keeps a window of the
    /// most recent packets. Returns how many of the oldest rows were dropped.
    fn push_window(&mut self, record: Record) -> usize {
        self.append(record, 0);
        let mut excess = self
            .capture
            .packets
            .len()
            .saturating_sub(live_capture::MAX_PACKETS);
        let packets = &self.capture.packets;
        let mut bytes = self.capture.retained_bytes
            - packets[..excess]
                .iter()
                .map(|p| p.bytes.len())
                .sum::<usize>();
        // Always keep the newest packet, even if it alone exceeds the byte bound.
        while bytes > live_capture::MAX_BYTES && excess + 1 < packets.len() {
            bytes -= packets[excess].bytes.len();
            excess += 1;
        }
        if excess > 0 {
            let removed: usize = self
                .capture
                .packets
                .drain(..excess)
                .map(|p| p.bytes.len())
                .sum();
            self.summaries.drain(..excess);
            self.search.drain(..excess);
            self.capture.retained_bytes -= removed;
            self.capture.omitted_packets += excess as u64;
            self.sample_count = self.capture.packets.len();
        }
        excess
    }
    fn append(&mut self, record: Record, file_offset: u64) {
        let number = self.capture.total_packets + 1;
        let captured_len = record.bytes.len() as u32;
        let packet = netlane_capture::Packet {
            number,
            file_offset,
            interface: 0,
            timestamp: Some(netlane_capture::Timestamp {
                ticks: u64::from(record.seconds) * 1_000_000 + u64::from(record.micros),
                resolution: 6,
                offset_seconds: 0,
            }),
            original_len: record.original_len,
            captured_len,
            bytes: record.bytes,
        };
        let summary = packet.summary(self.capture.interfaces[0].link_type);
        self.search.push(
            format!(
                "{} {} {} {} {} {} {}",
                number,
                summary.protocol,
                summary.source,
                summary.destination,
                summary.info,
                summary.service,
                self.capture.interfaces[0].name
            )
            .to_lowercase(),
        );
        if packet.captured_len < packet.original_len {
            self.capture.truncated_packets += 1;
        }
        self.capture.retained_bytes += packet.bytes.len();
        self.capture.packets.push(packet);
        self.summaries.push(summary);
        self.capture.total_packets = number;
        self.sample_count = self.capture.packets.len();
    }
    /// Reviewed-context shape sent to Codex: aggregates of the whole capture plus
    /// metadata of the chosen rows. Never packet bytes or paths.
    fn evidence(&self, indexes: &[usize], baseline: Option<&Report>) -> netlane_ai::Context {
        let data = self;
        netlane_ai::Context {
            investigation_id: uuid::Uuid::new_v4().to_string(),
            sha256: data.capture.sha256.clone(),
            format: data.capture.format.label().into(),
            total_packets: data.capture.total_packets,
            inspected_packets: data
                .report
                .as_ref()
                .map_or(data.capture.packets.len(), |r| r.packets as usize),
            omitted_packets: if data.report.is_some() {
                0
            } else {
                data.capture.omitted_packets
            },
            analysis: data
                .report
                .as_ref()
                .map(|report| netlane_ai::AnalysisEvidence {
                    summary: format!(
                        "{}Período completo: {} a {} (UTC, nanossegundos Unix).\n{} pacote recuperado manualmente além das amostras distribuídas; até 2048 bytes retidos, sem payload no contexto.\n{}",
                        report.summary_text(baseline),
                        report.first_unix_nanos.as_deref().unwrap_or("ausente"),
                        report.last_unix_nanos.as_deref().unwrap_or("ausente"),
                        data.capture.packets.len().saturating_sub(data.sample_count),
                        data.collection_note.as_deref().unwrap_or("")
                    ),
                    analyzed_packets: report.packets,
                    sample_packets: report.sample_packets,
                    baseline_sha256: baseline.map(|r| r.sha256.clone()),
                    packet_references: baseline.map_or_else(Vec::new, |b| {
                        report
                            .compare(b)
                            .into_iter()
                            .flat_map(|f| f.packet_references)
                            .take(24)
                            .collect()
                    }),
                }),
            packets: indexes
                .iter()
                .map(|&index| {
                    let packet = &data.capture.packets[index];
                    let summary = &data.summaries[index];
                    let interface = &data.capture.interfaces[packet.interface];
                    netlane_ai::PacketEvidence {
                        number: packet.number,
                        interface: interface.name.clone(),
                        section: interface.section,
                        link_type: interface.link_type,
                        unix_nanos: packet.timestamp.map(|ts| ts.unix_nanos().to_string()),
                        captured_bytes: packet.captured_len as usize,
                        original_bytes: packet.original_len,
                        protocol: summary.protocol.into(),
                        source: summary.source.clone(),
                        destination: summary.destination.clone(),
                        observation: summary.info.clone(),
                    }
                })
                .collect(),
        }
    }
    fn matches(&self, query: &str) -> Vec<usize> {
        self.search
            .iter()
            .take(self.sample_count)
            .enumerate()
            .filter(|(_, s)| s.contains(query))
            .map(|(i, _)| i)
            .collect()
    }
    fn source(&self) -> Option<netlane_ai::bundle::Source> {
        let mut source = netlane_ai::bundle::Source::new(self.source.clone()?, &self.capture);
        source.provenance = self.collection_note.as_ref().map(|note| {
            if note.len() <= 4096 {
                return note.clone();
            }
            let mut end = 4000;
            while !note.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}… origem limitada a um trecho no pacote.", &note[..end])
        });
        Some(source)
    }
    fn install_recovered(&mut self, recovered: RecoveredPacket) -> anyhow::Result<usize> {
        anyhow::ensure!(
            recovered.sha256 == self.capture.sha256
                && recovered.packet.number > 0
                && recovered.packet.number <= self.capture.total_packets
                && recovered.packet.interface < self.capture.interfaces.len(),
            "O pacote recuperado não pertence ao dump atual."
        );
        // At most one additional 2 KiB preview. The distributed sample stays intact.
        self.capture.packets.truncate(self.sample_count);
        self.summaries.truncate(self.sample_count);
        self.search.truncate(self.sample_count);
        let index = if let Some(index) = self
            .capture
            .packets
            .iter()
            .position(|p| p.number == recovered.packet.number)
        {
            index
        } else {
            self.capture.packets.push(recovered.packet);
            self.summaries.push(recovered.summary);
            self.sample_count
        };
        self.capture.retained_bytes = self.capture.packets.iter().map(|p| p.bytes.len()).sum();
        self.capture.omitted_packets =
            self.capture.total_packets - self.capture.packets.len() as u64;
        Ok(index)
    }
}

pub struct PacketView {
    demo: bool,
    data: Option<Inspection>,
    label: String,
    status: String,
    filter: Entity<TextInput>,
    packet_number: Entity<TextInput>,
    lookup_number: Option<u64>,
    matching: Vec<usize>,
    selected: Option<usize>,
    show_bytes: bool,
    raw: bool,
    baseline: Option<Report>,
    baseline_source: Option<netlane_ai::bundle::Source>,
    progress: Arc<Progress>,
    busy: bool,
    generation: u64,
    cancel: Arc<AtomicBool>,
    dialog: Arc<AtomicBool>,
    codex: Entity<CodexView>,
    collection: Entity<CollectionView>,
    collection_open: bool,
    capture_active: Arc<AtomicBool>,
    devices: Vec<Device>,
    device: Option<usize>,
    picker: bool,
    listing: bool,
    bpf: Entity<TextInput>,
    live: Option<LiveCapture>,
    /// PCAP image of the live capture, kept for analysis and "Salvar captura".
    live_file: Vec<u8>,
    list_scroll: UniformListScrollHandle,
    follow: bool,
    /// Long mode records to `captures` for the chosen duration.
    long_mode: bool,
    duration_index: usize,
    captures: std::path::PathBuf,
    last_recording: Option<std::path::PathBuf>,
    /// Codex following: last snapshot time, packets covered, snapshot in progress.
    follow_at: Option<std::time::Instant>,
    follow_seen: u64,
    follow_busy: bool,
    _subscription: Subscription,
    _collection_subscription: Subscription,
    _bundle_subscription: Subscription,
}
impl Drop for PacketView {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.dialog.store(false, Ordering::Relaxed);
    }
}
impl PacketView {
    pub fn new(
        demo: bool,
        dialog: Arc<AtomicBool>,
        codex: Entity<CodexView>,
        profile: std::path::PathBuf,
        capture_active: Arc<AtomicBool>,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter =
            cx.new(|cx| TextInput::new("", "Protocolo, endereço, porta, interface ou número", cx));
        let packet_number = cx.new(|cx| TextInput::new("", "Número do pacote no dump", cx));
        let subscription = cx.subscribe(&filter, |this, _, _, cx| {
            this.refilter(cx);
            cx.notify();
        });
        let captures = profile.join("captures");
        let collection = cx.new(|cx| {
            CollectionView::new(demo, profile, dialog.clone(), capture_active.clone(), cx)
        });
        let collection_subscription=cx.subscribe(&collection,|this,_,event:&SegmentReady,cx|{
            if let Some((scan,note))=event.0.borrow_mut().take(){
                if this.busy { this.cancel(); }
                let mut data=Inspection::scan(scan);data.collection_note=Some(note);this.data=Some(data);this.selected=None;this.show_bytes=false;
                this.label="Segmento fechado da coleta · prévia recebida mais recentemente".into();
                this.status="Segmento completo analisado. Os limites/perdas da coleta aparecem no painel acima.".into();this.refilter(cx);cx.notify();
            }
        });
        let bundle_subscription = cx.subscribe(&codex, |this, _, event: &InvestigationRestored, cx| {
            if let Some(ready) = event.0.borrow_mut().take() {
                if this.live.is_some() {
                    this.status = "Pare a captura antes de reabrir uma investigação.".into();
                    cx.notify();
                    return;
                }
                this.cancel();
                let mut data = Inspection::scan(ready.scan);
                data.collection_note = ready.provenance.or_else(|| Some("Origem: dump integral reaberto de uma investigação conferida.\n".into()));
                let index = ready.recovered.map(|packet| data.install_recovered(packet).expect("verified recovery"))
                    .or_else(|| data.capture.packets.iter().position(|p| p.number == ready.number));
                this.baseline = ready.baseline;
                this.baseline_source = ready.baseline_source;
                this.data = Some(data);
                this.selected = index;
                this.show_bytes = false;
                this.collection_open = false;
                this.label = "Captura reaberta da investigação · integridade conferida".into();
                this.status = format!("Dump conferido e pacote #{} restaurado. Revise o contexto antes de continuar com Codex.", ready.number);
                this.filter.update(cx, |f, cx| f.set_text("", cx));
                this.refilter(cx);
                cx.notify();
            }
        });
        Self {
            demo,
            data: None,
            label: String::new(),
            status: "Escolha a interface e inicie a captura, ou abra um arquivo PCAP/PCAPNG."
                .into(),
            filter,
            packet_number,
            lookup_number: None,
            matching: vec![],
            selected: None,
            show_bytes: false,
            raw: false,
            baseline: None,
            baseline_source: None,
            progress: Arc::new(Progress::default()),
            busy: false,
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            dialog,
            codex,
            collection,
            collection_open: false,
            capture_active,
            devices: Vec::new(),
            device: None,
            picker: false,
            listing: false,
            bpf: cx.new(|cx| TextInput::new("", "Filtro BPF (ex.: tcp port 443)", cx)),
            live: None,
            live_file: Vec::new(),
            list_scroll: UniformListScrollHandle::new(),
            follow: false,
            long_mode: false,
            duration_index: 1,
            captures,
            last_recording: None,
            follow_at: None,
            follow_seen: 0,
            follow_busy: false,
            _subscription: subscription,
            _collection_subscription: collection_subscription,
            _bundle_subscription: bundle_subscription,
        }
    }
    fn refilter(&mut self, cx: &App) {
        let query: String = self
            .filter
            .read(cx)
            .text()
            .trim()
            .chars()
            .take(256)
            .collect::<String>()
            .to_lowercase();
        self.matching = self
            .data
            .as_ref()
            .map_or_else(Vec::new, |data| data.matches(&query));
        if self.selected.is_some_and(|i| {
            self.data.as_ref().is_some_and(|d| i < d.sample_count) && !self.matching.contains(&i)
        }) {
            self.selected = None;
            self.show_bytes = false;
        }
    }
    pub fn example(&mut self, cx: &mut Context<Self>) {
        if self.busy
            || self.capture_active.load(Ordering::Relaxed)
            || self.dialog.load(Ordering::Relaxed)
        {
            return;
        }
        self.data = Some(Inspection::scan(netlane_capture::example_scan()));
        self.data.as_mut().unwrap().collection_note =
            Some("Origem: exemplo fictício em memória; nenhum tráfego coletado.\n".into());
        self.label = "Exemplo fictício · nenhum tráfego coletado".into();
        self.status =
            "Dados em memória. Porta DNS é uma estimativa, sem análise do protocolo DNS.".into();
        self.selected = None;
        self.show_bytes = false;
        self.filter.update(cx, |f, cx| f.set_text("", cx));
        self.refilter(cx);
        cx.notify();
    }
    fn clear(&mut self, cx: &mut Context<Self>) {
        if self.dialog.load(Ordering::Relaxed) && !self.busy {
            return;
        }
        if self.capture_active.load(Ordering::Relaxed) {
            self.status = "Pare a coleta antes de limpar a visualização.".into();
            cx.notify();
            return;
        }
        self.cancel();
        self.data = None;
        self.live_file = Vec::new();
        self.matching.clear();
        self.label.clear();
        self.selected = None;
        self.show_bytes = false;
        self.status = if self.busy {
            "Encerrando a leitura…"
        } else {
            "Captura removida da memória."
        }
        .into();
        cx.notify();
    }
    fn toggle_picker(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            return;
        }
        self.picker = !self.picker;
        if self.picker && self.devices.is_empty() {
            self.list_devices(cx);
        }
        cx.notify();
    }
    fn list_devices(&mut self, cx: &mut Context<Self>) {
        if self.listing || self.live.is_some() {
            return;
        }
        if self.demo {
            self.devices = vec![Device {
                name: "synthetic-no-network".into(),
                description: "Interface fictícia · sem rede".into(),
            }];
            self.device = Some(0);
            return;
        }
        self.listing = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { netlane_windows::npcap::devices() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.listing = false;
                match result {
                    Ok(devices) => {
                        this.device = (devices.len() == 1).then_some(0);
                        this.status = if devices.is_empty() {
                            "Nenhuma interface Npcap disponível.".into()
                        } else {
                            "Escolha a interface. A captura ainda não foi iniciada.".into()
                        };
                        this.devices = devices;
                    }
                    Err(e) => {
                        this.picker = false;
                        this.status = format!("Interfaces indisponíveis: {e:#}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn start_live(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some()
            || self.busy
            || self.dialog.load(Ordering::Relaxed)
            || self.capture_active.load(Ordering::Relaxed)
        {
            return;
        }
        let Some(device) = self.device.and_then(|i| self.devices.get(i)).cloned() else {
            self.status = "Escolha a interface de captura.".into();
            self.toggle_picker(cx);
            return;
        };
        let filter = self.bpf.read(cx).text().trim().to_owned();
        let minutes = live_capture::DURATIONS[self.duration_index];
        let started = if self.long_mode {
            // A new folder per capture; earlier captures are never touched.
            let directory = self
                .captures
                .join(format!("NetLane-{}", uuid::Uuid::new_v4()));
            let options = netlane_capture::collection::Options {
                interface: device.name.clone(),
                filter: filter.clone(),
                snaplen: live_capture::SNAPLEN,
                duration_seconds: minutes * 60,
                ..Default::default()
            };
            std::fs::create_dir_all(&self.captures)
                .map_err(anyhow::Error::from)
                .and_then(|()| {
                    if self.demo {
                        LiveCapture::record(directory, options, || {
                            Ok(live_capture::Synthetic::new())
                        })
                    } else {
                        let (name, filter) = (device.name.clone(), filter.clone());
                        LiveCapture::record(directory, options, move || {
                            netlane_windows::npcap::Capture::open(
                                &name,
                                &filter,
                                live_capture::SNAPLEN,
                            )
                        })
                    }
                })
        } else if self.demo {
            LiveCapture::start(|| Ok(live_capture::Synthetic::new()))
        } else {
            let (name, filter) = (device.name.clone(), filter.clone());
            LiveCapture::start(move || {
                netlane_windows::npcap::Capture::open(&name, &filter, live_capture::SNAPLEN)
            })
        };
        let live = match started {
            Ok(live) => live,
            Err(e) => {
                self.status = format!("Não foi possível iniciar a captura: {e:#}");
                cx.notify();
                return;
            }
        };
        self.cancel();
        self.live = Some(live);
        self.capture_active.store(true, Ordering::Relaxed);
        self.data = None;
        self.live_file = Vec::new();
        self.matching.clear();
        self.selected = None;
        self.show_bytes = false;
        self.picker = false;
        self.follow = true;
        self.follow_at = None;
        self.follow_seen = 0;
        self.last_recording = self
            .live
            .as_ref()
            .and_then(|l| l.recording.as_ref())
            .map(|r| r.directory.clone());
        self.label = if self.long_mode {
            format!(
                "Captura longa · {} · até {}",
                device.description,
                live_capture::duration_label(minutes)
            )
        } else {
            format!("Captura ao vivo · {}", device.description)
        };
        self.status = "Abrindo interface e aplicando filtro…".into();
        let note = format!(
            "Origem: captura ao vivo {} {}. Interface {:?}; filtro BPF {:?}; até {} bytes por pacote; sem modo promíscuo.\n",
            if self.demo {
                "fictícia"
            } else {
                "Windows/Npcap"
            },
            if self.long_mode {
                "gravada em disco; a tabela mostra os pacotes mais recentes"
            } else {
                "em memória"
            },
            device.description,
            filter,
            live_capture::SNAPLEN
        );
        // Only drain_live clears `live`, and it ends this loop in the same call.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(80))
                    .await;
                let keep = this
                    .update(cx, |this, cx| this.drain_live(&device, &note, cx))
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
    fn stop_live(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = &self.live {
            live.stop();
            self.status = "Encerrando captura…".into();
            cx.notify();
        }
    }
    /// Moves delivered packets into the table. Returns false when the capture ended.
    fn drain_live(&mut self, device: &Device, note: &str, cx: &mut Context<Self>) -> bool {
        let Some(live) = self.live.take() else {
            return false;
        };
        let recording = live.recording.is_some();
        let mut added = false;
        let mut ended = None;
        for _ in 0..64 {
            match live.events.try_recv() {
                Ok(live_capture::Event::Started { link_type }) => {
                    let mut data = Inspection::live(link_type, device.description.clone());
                    data.collection_note = Some(note.to_owned());
                    self.data = Some(data);
                    // Long mode keeps no in-memory image: the PCAP files are on disk.
                    if !recording {
                        self.live_file =
                            live_capture::pcap_header(link_type, live_capture::SNAPLEN);
                    }
                    self.status = if recording {
                        "Gravando nesta interface…"
                    } else {
                        "Capturando nesta interface…"
                    }
                    .into();
                }
                Ok(live_capture::Event::Packets(records)) => {
                    let Some(data) = self.data.as_mut() else {
                        continue;
                    };
                    if recording {
                        let mut dropped = 0;
                        for record in records {
                            dropped += data.push_window(record);
                        }
                        self.selected = self.selected.and_then(|i| i.checked_sub(dropped));
                        added = true;
                        continue;
                    }
                    for record in records {
                        if !data.push_live(record, &mut self.live_file) {
                            live.stop();
                            self.status = format!(
                                "Limite de {} pacotes ou {} MiB atingido; encerrando a captura…",
                                live_capture::MAX_PACKETS,
                                live_capture::MAX_BYTES / 1024 / 1024
                            );
                            break;
                        }
                        added = true;
                    }
                }
                Ok(live_capture::Event::Finished { reason, driver }) => {
                    let driver = driver.map_or_else(
                        || "perdas do driver indisponíveis".into(),
                        |d| {
                            format!(
                                "driver: {} descartados no buffer, {} na interface",
                                d.dropped, d.interface_dropped
                            )
                        },
                    );
                    ended = Some(format!("{reason} {driver}"));
                    break;
                }
                Ok(live_capture::Event::Failed(e)) => {
                    ended = Some(format!("A captura não foi concluída: {e}"));
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    ended = Some("Tarefa de captura encerrada.".into());
                    break;
                }
            }
        }
        if added {
            self.refilter(cx);
            if self.follow {
                self.list_scroll.scroll_to_bottom();
            }
        }
        let dropped = live.queue_dropped.load(Ordering::Relaxed);
        match ended {
            Some(reason) if recording => {
                self.capture_active.store(false, Ordering::Relaxed);
                self.follow = false;
                let shown = if dropped > 0 {
                    format!(" {dropped} pacotes gravados não couberam na tabela.")
                } else {
                    String::new()
                };
                self.status = format!(
                    "{reason}{shown} Use Abrir pasta para ver os arquivos; Abrir PCAP analisa cada um."
                );
            }
            Some(reason) => self.finish_live(reason, dropped, cx),
            None => {
                self.live = Some(live);
                if !recording {
                    self.follow_snapshot(cx);
                }
            }
        }
        cx.notify();
        self.live.is_some()
    }
    /// While Codex follows a diagnostic capture: every 30 s with new packets, hash
    /// and analyze the in-memory image in the background and hand Codex the same
    /// metadata-only context a manual send uses (aggregates + newest samples).
    fn follow_snapshot(&mut self, cx: &mut Context<Self>) {
        const EVERY: std::time::Duration = std::time::Duration::from_secs(30);
        if self.follow_busy
            || !self.codex.read(cx).following()
            || self.follow_at.is_some_and(|at| at.elapsed() < EVERY)
        {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let total = data.capture.total_packets;
        if total <= self.follow_seen {
            return;
        }
        self.follow_busy = true;
        self.follow_at = Some(std::time::Instant::now());
        let previous = self.follow_seen;
        let note = data.collection_note.clone();
        let file = self.live_file.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    netlane_capture::analysis::read_dump(
                        std::io::Cursor::new(file),
                        &AtomicBool::new(false),
                        &Progress::default(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.follow_busy = false;
                let Ok(scan) = result else {
                    return;
                };
                let mut snapshot = Inspection::scan(scan);
                snapshot.collection_note = note;
                let samples = snapshot.capture.packets.len();
                let newest: Vec<usize> = (samples.saturating_sub(8)..samples).collect();
                if newest.is_empty() {
                    return;
                }
                let context = snapshot.evidence(&newest, None);
                let question = format!(
                    "Acompanhamento automático da captura ao vivo: {total} pacotes até agora, {} novos desde o último resumo. O que mudou? Aponte anomalias, hipóteses e o que conferir em seguida. Seja breve.",
                    total - previous
                );
                if this
                    .codex
                    .update(cx, |codex, cx| codex.follow_snapshot(context, question, cx))
                {
                    this.follow_seen = total;
                }
            });
        })
        .detach();
    }
    /// Hashes and analyzes the PCAP image so references, Codex and export work
    /// as for an imported file. The table keeps every captured packet.
    fn finish_live(&mut self, reason: String, dropped: u64, cx: &mut Context<Self>) {
        self.capture_active.store(false, Ordering::Relaxed);
        self.follow = false;
        let packets = self.data.as_ref().map_or(0, |d| d.capture.packets.len());
        let queue = if dropped > 0 {
            format!(" {dropped} pacotes descartados porque a tabela não acompanhou.")
        } else {
            String::new()
        };
        if packets == 0 {
            self.data = None;
            self.live_file = Vec::new();
            self.status = format!("{reason}{queue} Nenhum pacote capturado.");
            return;
        }
        if let Some(note) = self.data.as_mut().and_then(|d| d.collection_note.as_mut()) {
            note.push_str(&format!("{reason}{queue}\n"));
        }
        self.status = format!("{reason}{queue} Analisando {packets} pacotes…");
        let file = self.live_file.clone();
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    netlane_capture::analysis::read_dump(
                        std::io::Cursor::new(file),
                        &AtomicBool::new(false),
                        &Progress::default(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                let Some(data) = this.data.as_mut() else {
                    return;
                };
                match result {
                    Ok(scan) => {
                        data.capture.sha256 = scan.capture.sha256;
                        data.report = Some(scan.report);
                        this.status = format!(
                            "{reason}{queue} {packets} pacotes em memória. Use Salvar captura para guardar o PCAP."
                        );
                    }
                    Err(e) => {
                        this.status = format!("{reason}{queue} Análise não concluída: {e:#}")
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn save_live(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.live.is_some() || self.live_file.len() <= 24 || self.dialog.load(Ordering::Relaxed)
        {
            return;
        }
        let file = self.live_file.clone();
        let generation = self.generation;
        let directory = std::env::var_os("USERPROFILE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ".".into());
        self.dialog.store(true, Ordering::Relaxed);
        let target = cx.prompt_for_new_path(&directory, Some("netlane-captura.pcap"));
        cx.spawn_in(window, async move |this, cx| {
            let path = match target.await {
                Ok(Ok(Some(path))) => path,
                _ => {
                    let _ = this.update(cx, |this, cx| {
                        this.dialog.store(false, Ordering::Relaxed);
                        cx.notify();
                    });
                    return;
                }
            };
            let destination = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    use std::io::Write;
                    // create_new never replaces a file chosen between prompt and write.
                    let mut out = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&destination)?;
                    let result = out.write_all(&file).and_then(|()| out.sync_all());
                    if result.is_err() {
                        drop(out);
                        let _ = std::fs::remove_file(&destination);
                    }
                    result
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dialog.store(false, Ordering::Relaxed);
                this.status = match result {
                    Ok(()) => {
                        if this.generation == generation
                            && let Some(data) = this.data.as_mut()
                        {
                            data.source = Some(path.clone());
                        }
                        format!("Captura salva em {}.", path.display())
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        "O arquivo já existe. Escolha outro nome.".into()
                    }
                    Err(e) => format!("Não foi possível salvar a captura: {e}"),
                };
                cx.notify();
            });
        })
        .detach();
    }
    pub fn prepare_codex(&mut self, cx: &mut Context<Self>) {
        self.prepare_codex_selection(false, cx);
    }
    fn recover_input(&mut self, cx: &mut Context<Self>) {
        let text = self.packet_number.read(cx).text().trim();
        let number = text.strip_prefix('#').unwrap_or(text).parse::<u64>();
        match number {
            Ok(number) => self.recover_number(number, cx),
            Err(_) => {
                self.status = "Informe o número inteiro do pacote, como 501 ou #501.".into();
                cx.notify();
            }
        }
    }
    fn recover_number(&mut self, number: u64, cx: &mut Context<Self>) {
        if self.busy || self.dialog.load(Ordering::Relaxed) {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        if let Some(index) = data.capture.packets.iter().position(|p| p.number == number) {
            self.selected = Some(index);
            self.show_bytes = false;
            self.filter.update(cx, |f, cx| f.set_text("", cx));
            self.refilter(cx);
            self.status = format!("Pacote #{number} selecionado na evidência já carregada.");
            cx.notify();
            return;
        }
        let anchor = match netlane_capture::lookup::PacketAnchor::new(&data.capture, number) {
            Ok(anchor) => anchor,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let Some(path) = data.source.clone() else {
            self.status = "Para recuperar um pacote fora da amostra, abra o dump original.".into();
            cx.notify();
            return;
        };
        self.busy = true;
        self.dialog.store(true, Ordering::Relaxed);
        self.collection
            .update(cx, |c, cx| c.set_importing(true, cx));
        self.generation = self.generation.wrapping_add(1);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.progress = Arc::new(Progress::default());
        self.lookup_number = Some(number);
        self.status = format!("Recuperando #{number} e conferindo o SHA-256 do dump inteiro…");
        self.start_progress(cx);
        let generation = self.generation;
        let cancel = self.cancel.clone();
        let progress = self.progress.clone();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                netlane_capture::lookup::load_packet(&path, &anchor, &cancel, &progress)
            }).await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.dialog.store(false, Ordering::Relaxed);
                this.lookup_number = None;
                this.collection.update(cx, |c, cx| c.set_importing(false, cx));
                if generation != this.generation {
                    if this.status == "Encerrando a leitura…" {
                        this.status = "Leitura cancelada. A evidência atual foi preservada.".into();
                    }
                    cx.notify(); return;
                }
                match result.and_then(|packet| this.data.as_mut().ok_or_else(|| anyhow::anyhow!("Captura removida.")).and_then(|data| data.install_recovered(packet))) {
                    Ok(index) => {
                        this.selected = Some(index);
                        this.show_bytes = false;
                        this.filter.update(cx, |f, cx| f.set_text("", cx));
                        this.refilter(cx);
                        this.status = format!("Pacote #{number} recuperado do original · SHA-256 conferido · prévia de até 2 KiB. Revise antes de preparar para Codex.");
                    }
                    Err(error) => this.status = format!("Pacote não recuperado: {error:#}. A evidência atual foi preservada."),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn prepare_codex_selection(&mut self, replace: bool, cx: &mut Context<Self>) {
        let Some(data) = self.data.as_ref() else {
            return;
        };
        let Some(index) = self
            .selected
            .or_else(|| (!data.capture.packets.is_empty()).then_some(0))
        else {
            return;
        };
        let context = data.evidence(&[index], self.baseline.as_ref());
        let source = data.source();
        let baseline = self.baseline_source.clone();
        self.codex.update(cx, |panel, cx| {
            panel.prepare_capture(context, replace, source, baseline, cx)
        });
    }
    fn export_selection(
        &mut self,
        format: netlane_capture::Format,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.dialog.load(Ordering::Relaxed) {
            return;
        }
        let selection = match self.selection() {
            Ok(s) => s,
            Err(e) => {
                self.status = e.to_string();
                cx.notify();
                return;
            }
        };
        self.dialog.store(true, Ordering::Relaxed);
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Escolha onde salvar uma nova pasta com os pacotes selecionados".into()),
        });
        cx.spawn_in(window,async move|this,cx|{
            let chosen=paths.await;
            let parent=match chosen { Ok(Ok(Some(paths)))=>paths.into_iter().next(), _=>None };
            let Some(parent)=parent else {
                let _=this.update(cx,|this,cx|{this.dialog.store(false,Ordering::Relaxed);cx.notify();});return;
            };
            let directory=parent.join(format!("NetLane-pacotes-{}",uuid::Uuid::new_v4()));
            let destination=directory.clone();
            let _=this.update(cx,|this,cx|{this.status=format!("Exportando {} amostras em segundo plano…",selection.len());cx.notify();});
            let result=cx.background_executor().spawn(async move{selection.save(&destination,format)}).await;
            let _=this.update(cx,|this,cx|{
                this.dialog.store(false,Ordering::Relaxed);
                this.status=match result {
                    Ok(r)=>format!("{} amostras exportadas · {} com bytes reduzidos · {} · recibo com SHA-256 e números de origem.",r.packets.len(),r.clipped_packets,directory.display()),
                    Err(e)=>format!("Exportação não concluída: {e:#}. Arquivos parciais, se houver, preservados em {}.",directory.display()),
                };cx.notify();
            });
        }).detach();
    }
    fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.cancel.store(true, Ordering::Relaxed);
    }
    fn selection(&self) -> anyhow::Result<netlane_capture::export::Selection> {
        let data = self
            .data
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Abra uma captura primeiro."))?;
        let indexes = self
            .selected
            .map_or_else(|| self.matching.clone(), |i| vec![i]);
        netlane_capture::export::Selection::new(&data.capture, &indexes)
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_export(&mut self, directory: &std::path::Path, cx: &mut Context<Self>) {
        let selected = self.selection().unwrap();
        assert_eq!(selected.len(), 1);
        for (format, name) in [
            (netlane_capture::Format::Pcap, "pcap"),
            (netlane_capture::Format::PcapNg, "pcapng"),
        ] {
            let r = selected.save(&directory.join(name), format).unwrap();
            assert_eq!(r.packets.len(), 1);
            assert!(selected.save(&directory.join(name), format).is_err());
        }
        self.selected = None;
        let filtered = self.selection().unwrap();
        assert_eq!(filtered.len(), 6);
        let r = filtered
            .save(&directory.join("filtered"), netlane_capture::Format::PcapNg)
            .unwrap();
        assert_eq!(r.packets.len(), 6);
        self.status="Prova: seleção e amostras da busca exportadas em PCAP/PCAPNG com recibos; originais preservados.".into();
        cx.notify();
    }
    fn start_progress(&mut self, cx: &mut Context<Self>) {
        let progress = self.progress.clone();
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
                let Ok(done) = this.update(cx, |this, cx| {
                    if !this.busy
                        || this.generation != generation
                        || !Arc::ptr_eq(&this.progress, &progress)
                    {
                        return true;
                    }
                    this.status = if let Some(number) = this.lookup_number { format!(
                        "Recuperando #{number} · conferência do dump inteiro · {} pacotes · {:.1} MiB lidos",
                        progress.packets.load(Ordering::Relaxed),
                        progress.bytes.load(Ordering::Relaxed) as f64 / 1048576.
                    ) } else { format!(
                        "Análise em andamento · {} pacotes · {:.1} MiB lidos · {} RST TCP",
                        progress.packets.load(Ordering::Relaxed),
                        progress.bytes.load(Ordering::Relaxed) as f64 / 1048576.,
                        progress.resets.load(Ordering::Relaxed)
                    ) };
                    cx.notify();
                    false
                }) else {
                    break;
                };
                if done {
                    break;
                }
            }
        })
        .detach();
    }
    fn import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.demo {
            self.status =
                "Na demonstração, use o exemplo fictício. Abra normalmente para importar arquivos."
                    .into();
            cx.notify();
            return;
        }
        if self.busy
            || self.capture_active.load(Ordering::Relaxed)
            || self.dialog.load(Ordering::Relaxed)
        {
            return;
        }
        self.dialog.store(true, Ordering::Relaxed);
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Abrir captura PCAP ou PCAPNG".into()),
        });
        cx.spawn_in(window,async move|this,cx|{
            let chosen=paths.await;
            let _=this.update(cx,|this,cx|{this.dialog.store(false,Ordering::Relaxed);cx.notify();});
            let path=match chosen {
                Ok(Ok(Some(paths)))=>match paths.into_iter().next(){Some(path)=>path,None=>return},
                _=>return,
            };
            let label=path.file_name().map(|n|n.to_string_lossy().into_owned()).unwrap_or_else(||"Captura".into());
            let request=this.update(cx,|this,cx|{
                if this.busy {return None;}
                this.busy=true;this.dialog.store(true,Ordering::Relaxed);this.collection.update(cx,|c,cx|c.set_importing(true,cx));this.generation=this.generation.wrapping_add(1);this.cancel=Arc::new(AtomicBool::new(false));
                this.progress=Arc::new(Progress::default());
                this.status="Analisando o dump inteiro em segundo plano…".into();this.start_progress(cx);cx.notify();Some((this.generation,this.cancel.clone(),this.progress.clone()))
            });
            let Ok(Some((generation,cancel,progress)))=request else {return;};
            let result=cx.background_executor().spawn(async move{
                netlane_capture::analysis::load_dump(&path,&cancel,&progress).map(Inspection::scan)
            }).await;
            let _=this.update(cx,|this,cx|{
                this.busy=false;this.dialog.store(false,Ordering::Relaxed);this.collection.update(cx,|c,cx|c.set_importing(false,cx));
                if generation!=this.generation {this.status="Leitura cancelada. A captura atual foi preservada.".into();cx.notify();return;}
                match result {
                    Ok(mut data)=>{
                        data.collection_note=Some("Origem: dump importado. Método/ponto de coleta e perdas de origem não informados pelo importador.\n".into());
                        this.data=Some(data);this.label=label;this.selected=None;this.show_bytes=false;
                        this.status="Dump analisado por completo; a tabela mostra amostras distribuídas. Original preservado.".into();this.refilter(cx);
                    }
                    Err(error)=>this.status=format!("Não foi possível importar: {error:#}"),
                }
                cx.notify();
            });
        }).detach();
    }
    fn button(id: impl Into<gpui::ElementId>, label: &str, cx: &App) -> gpui::Stateful<gpui::Div> {
        let t = cx.theme();
        div()
            .id(id)
            .px(px(10.))
            .py(px(6.))
            .rounded(px(4.))
            .border_1()
            .border_color(t.border)
            .bg(t.elevated)
            .cursor_pointer()
            .hover(|s| s.bg(t.hover))
            .child(label.to_owned())
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_filter(&mut self, query: &str, cx: &mut Context<Self>) {
        self.filter.update(cx, |f, cx| f.set_text(query, cx));
        self.refilter(cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_select(&mut self) {
        self.selected = self.matching.first().copied();
        self.show_bytes = true;
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_focus_packets(&mut self, cx: &mut Context<Self>) {
        self.collection_open = false;
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_matches(&self) -> usize {
        self.matching.len()
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_comparison(&mut self, cx: &mut Context<Self>) {
        self.baseline = Some(netlane_capture::comparison_example(true).report);
        self.data = Some(Inspection::scan(netlane_capture::comparison_example(false)));
        self.data.as_mut().unwrap().collection_note = Some(
            "Origem: dump e referência fictícios em memória; nenhum tráfego coletado.\n".into(),
        );
        self.label = "Dump fictício de 10.000 pacotes · nenhuma coleta real".into();
        self.status = "Análise completa com referência fictícia e amostras distribuídas.".into();
        self.filter.update(cx, |f, cx| f.set_text("", cx));
        self.refilter(cx);
        self.selected = self
            .data
            .as_ref()
            .and_then(|d| d.capture.packets.iter().position(|p| p.number > 9000));
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_scan_valid(&self) -> bool {
        self.data.as_ref().is_some_and(|d| {
            d.report.as_ref().is_some_and(|r| {
                r.packets == 10_000
                    && r.sample_packets <= 512
                    && self
                        .baseline
                        .as_ref()
                        .is_some_and(|b| r.compare(b).iter().any(|f| f.kind == "destino-novo"))
            })
        })
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_recover(&mut self, directory: &std::path::Path, cx: &mut Context<Self>) {
        netlane_capture::qa::generate(
            directory,
            netlane_capture::qa::FixtureOptions {
                records: 2000,
                packet_bytes: 2112,
            },
        )
        .unwrap();
        let scan = netlane_capture::analysis::load_dump(
            &directory.join("deviation.pcap"),
            &AtomicBool::new(false),
            &Progress::default(),
        )
        .unwrap();
        let number = (1001..2000)
            .find(|number| !scan.capture.packets.iter().any(|p| p.number == *number))
            .unwrap();
        self.data = Some(Inspection::scan(scan));
        self.data.as_mut().unwrap().collection_note =
            Some("Origem: fixture PCAP local gerada para QA, sem captura de rede.\n".into());
        self.baseline = None;
        self.selected = None;
        self.collection_open = false;
        self.label = "Dump fictício · recuperação por número fora da amostra".into();
        self.packet_number
            .update(cx, |f, cx| f.set_text(number.to_string(), cx));
        self.refilter(cx);
        self.recover_input(cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_recovered(&self) -> bool {
        !self.busy
            && self.data.as_ref().is_some_and(|d| {
                self.selected == Some(d.sample_count)
                    && d.capture.packets.len() == d.sample_count + 1
                    && d.capture.packets[d.sample_count].bytes.len() == 2048
                    && d.capture.packets[d.sample_count].captured_len == 2112
                    && d.matches("").len() == d.sample_count
                    && self.status.contains("SHA-256 conferido")
            })
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_bundle_prepare(&mut self, directory: &std::path::Path, cx: &mut Context<Self>) {
        assert!(self.proof_recovered());
        let baseline = netlane_capture::analysis::load_dump(
            &directory.join("baseline.pcap"),
            &AtomicBool::new(false),
            &Progress::default(),
        )
        .unwrap();
        self.baseline_source = Some(netlane_ai::bundle::Source::new(
            baseline.source.unwrap(),
            &baseline.capture,
        ));
        self.baseline = Some(baseline.report);
        self.prepare_codex(cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_bundle_restored(&self, path: &std::path::Path) -> bool {
        !self.busy
            && self.label.contains("Captura reaberta")
            && self.baseline.as_ref().is_some_and(|b| b.packets == 1000)
            && self
                .baseline_source
                .as_ref()
                .is_some_and(|s| s.path.starts_with(path))
            && self.data.as_ref().is_some_and(|d| {
                d.capture.total_packets == 2000
                    && d.source.as_ref().is_some_and(|s| s.starts_with(path))
                    && self.selected == Some(d.sample_count)
                    && d.capture.packets.len() == d.sample_count + 1
                    && d.capture.packets[d.sample_count].bytes.len() == 2048
                    && d.capture.packets[d.sample_count].captured_len == 2112
                    && d.matches("").len() == d.sample_count
            })
    }
    #[cfg(feature = "gui-test")]
    pub fn qa_collection_start(
        &mut self,
        parent: std::path::PathBuf,
        options: netlane_capture::collection::Options,
        cx: &mut Context<Self>,
    ) {
        self.collection_open = true;
        self.collection
            .update(cx, |c, cx| c.qa_start(parent, options, cx));
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn qa_collection_stop(&mut self, cx: &mut Context<Self>) {
        self.collection.update(cx, |c, cx| c.qa_stop(cx));
    }
    #[cfg(feature = "gui-test")]
    pub fn qa_collection_state(&self, cx: &App) -> serde_json::Value {
        let mut value = self.collection.read(cx).qa_state();
        value["displayed_packets"] =
            serde_json::json!(self.data.as_ref().map(|d| d.capture.total_packets));
        value["displayed_sha256"] =
            serde_json::json!(self.data.as_ref().map(|d| &d.capture.sha256));
        value
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_collection(&mut self, cx: &mut Context<Self>) {
        self.collection_open = true;
        self.collection.update(cx, |c, cx| c.proof_start(cx));
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_collection_valid(&self, cx: &App) -> bool {
        self.collection.read(cx).proof_valid()
            && self.data.as_ref().is_some_and(|d| {
                d.report.is_some()
                    && d.collection_note
                        .as_ref()
                        .is_some_and(|n| n.contains("coleta fictícia") && !n.contains("C:\\"))
            })
    }
}

impl Render for PacketView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let compact = AppSettings::get(cx).compact;
        let capturing = self.live.is_some();
        let device = self
            .device
            .and_then(|i| self.devices.get(i))
            .map_or("Escolher interface", |d| d.description.as_str());
        // One compact row: capture controls, then file and analysis tools as icons.
        let chip = |id: &'static str, label: String, cx: &App| {
            div()
                .id(id)
                .h(px(28.))
                .px(px(8.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(4.))
                .rounded(px(4.))
                .border_1()
                .border_color(t.border)
                .bg(t.elevated)
                .text_size(px(12.5))
                .cursor_pointer()
                .hover(|d| d.bg(cx.theme().hover))
                .when(capturing, |d| d.opacity(0.6))
                .child(label)
        };
        let divider = || div().w(px(1.)).h(px(18.)).mx(px(4.)).bg(t.border);
        let hashed = !capturing
            && self
                .data
                .as_ref()
                .is_some_and(|d| !d.capture.sha256.is_empty());
        let toolbar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.))
            .child(
                chip("capture-interface", format!("{device} ▾"), cx)
                    .max_w(px(280.))
                    .truncate()
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_picker(cx))),
            )
            .child(
                div()
                    .w(px(220.))
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.editor)
                    .overflow_hidden()
                    .child(self.bpf.clone()),
            )
            .child(
                chip(
                    "capture-mode",
                    if self.long_mode {
                        "Longa (disco) ▾".into()
                    } else {
                        "Diagnóstico ▾".into()
                    },
                    cx,
                )
                .tooltip(|_, cx| {
                    cx.new(|_| {
                        Tip("Diagnóstico: em memória, análise ao parar. Longa: grava em disco pelo período escolhido.".into())
                    })
                    .into()
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.live.is_none() {
                        this.long_mode = !this.long_mode;
                        cx.notify();
                    }
                })),
            )
            .when(self.long_mode, |row| {
                row.child(
                    chip(
                        "capture-duration",
                        format!(
                            "{} ▾",
                            live_capture::duration_label(
                                live_capture::DURATIONS[self.duration_index]
                            )
                        ),
                        cx,
                    )
                    .tooltip(|_, cx| cx.new(|_| Tip("Por quanto tempo gravar".into())).into())
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.live.is_none() {
                            this.duration_index =
                                (this.duration_index + 1) % live_capture::DURATIONS.len();
                            cx.notify();
                        }
                    })),
                )
            })
            .child(if capturing {
                div()
                    .id("capture-stop")
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(t.warning)
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .hover(|d| d.bg(t.hover))
                    .child(icon("stop", 16., t.warning))
                    .child("Parar")
                    .on_click(cx.listener(|this, _, _, cx| this.stop_live(cx)))
            } else {
                div()
                    .id("capture-start")
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(4.))
                    .bg(t.accent)
                    .text_color(t.text_on_accent)
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .child(icon("play_arrow", 16., t.text_on_accent))
                    .child("Capturar")
                    .on_click(cx.listener(|this, _, _, cx| this.start_live(cx)))
            })
            .child(divider())
            .when(!capturing && self.live_file.len() > 24, |row| {
                row.child(
                    icon_button("capture-save", "save", "Salvar captura (PCAP)", false, cx)
                        .on_click(cx.listener(|this, _, w, cx| this.save_live(w, cx))),
                )
            })
            .when_some(self.last_recording.clone(), |row, directory| {
                row.child(
                    icon_button(
                        "capture-folder",
                        "folder_open",
                        "Abrir pasta da captura longa",
                        false,
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if directory.is_dir() {
                            cx.open_with_system(&directory);
                        } else {
                            this.status = "A pasta da captura ainda não existe.".into();
                            cx.notify();
                        }
                    })),
                )
            })
            .child(
                icon_button("import-capture", "add", "Abrir arquivo PCAP/PCAPNG", false, cx)
                    .on_click(cx.listener(|this, _, w, cx| this.import(w, cx))),
            )
            .child(
                icon_button("example-capture", "info", "Exemplo fictício", false, cx)
                    .on_click(cx.listener(|this, _, _, cx| this.example(cx))),
            )
            .child(
                icon_button("clear-capture", "delete", "Limpar", false, cx)
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            )
            .child(
                icon_button(
                    "packets-raw",
                    "code",
                    if self.raw {
                        "RAW: exibindo o payload imprimível"
                    } else {
                        "RAW: mostrar o payload imprimível"
                    },
                    self.raw,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.raw = !this.raw;
                    cx.notify();
                })),
            )
            .when(self.busy, |row| {
                row.child(
                    icon_button("cancel-capture-import", "stop", "Cancelar leitura", false, cx)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel();
                            this.status = "Encerrando a leitura…".into();
                            cx.notify();
                        })),
                )
            })
            // Analysis needs a hashed capture: imported files and stopped diagnostic captures.
            .when(hashed, |row| {
                row.child(divider())
                    .child(
                        icon_button(
                            "prepare-dump-codex",
                            "send",
                            "Preparar análise para Codex",
                            false,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.prepare_codex(cx))),
                    )
                    .child(
                        icon_button(
                            "packets-baseline",
                            "check",
                            "Usar como referência para comparar",
                            self.baseline.is_some(),
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.baseline =
                                this.data.as_ref().and_then(|data| data.report.clone());
                            this.baseline_source =
                                this.data.as_ref().and_then(Inspection::source);
                            cx.notify();
                        })),
                    )
                    .when(self.baseline.is_some(), |row| {
                        row.child(
                            icon_button("clear-baseline", "close", "Remover referência", false, cx)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.baseline = None;
                                    this.baseline_source = None;
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        icon_button(
                            "export-pcapng",
                            "arrow_downward",
                            "Exportar seleção em PCAPNG",
                            false,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.export_selection(netlane_capture::Format::PcapNg, w, cx)
                        })),
                    )
                    .child(
                        icon_button(
                            "export-pcap",
                            "save",
                            "Exportar seleção em PCAP",
                            false,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.export_selection(netlane_capture::Format::Pcap, w, cx)
                        })),
                    )
            });
        let picker = self.picker.then(|| {
            let mut list = div()
                .id("capture-interfaces")
                .max_h(px(180.))
                .overflow_y_scroll()
                .rounded(px(4.))
                .border_1()
                .border_color(t.border)
                .bg(t.panel);
            if self.listing {
                list = list.child(
                    div()
                        .px(px(10.))
                        .py(px(6.))
                        .text_color(t.text_muted)
                        .child("Consultando interfaces…"),
                );
            }
            for (index, d) in self.devices.iter().enumerate() {
                list = list.child(
                    div()
                        .id(("capture-if", index))
                        .px(px(10.))
                        .py(px(5.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .cursor_pointer()
                        .when(self.device == Some(index), |row| row.bg(t.selected))
                        .hover(|row| row.bg(t.hover))
                        .child(d.description.clone())
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(11.))
                                .text_color(t.text_muted)
                                .child(d.name.clone()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.device = Some(index);
                            this.picker = false;
                            cx.notify();
                        })),
                );
            }
            list.with_scrollbar(cx)
        });
        let status = if let Some(recording) = self.live.as_ref().and_then(|l| l.recording.as_ref())
        {
            let p = &recording.progress;
            format!(
                "● {} · {} pacotes · {:.1} MiB gravados em {} arquivos · até {} · tabela com os {} mais recentes",
                self.status,
                p.seen.load(Ordering::Relaxed),
                p.bytes.load(Ordering::Relaxed) as f64 / 1_048_576.,
                p.segments.load(Ordering::Relaxed),
                live_capture::duration_label(recording.minutes),
                live_capture::MAX_PACKETS
            )
        } else if capturing {
            format!(
                "● {} · {} pacotes · {:.1} MiB",
                self.status,
                self.data.as_ref().map_or(0, |d| d.capture.total_packets),
                self.live_file.len() as f64 / 1_048_576.
            )
        } else {
            self.status.clone()
        };
        let mut root = div()
            .flex()
            .flex_col()
            .gap(px(if compact { 8. } else { 14. }))
            .min_w_0()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(if compact { 20. } else { 24. }))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.text_strong)
                            .child("Pacotes"),
                    )
                    .child(div().text_size(px(12.)).text_color(t.text_muted).child(
                        "Capture ao vivo ou abra um arquivo. A análise fica neste computador.",
                    )),
            )
            .child(toolbar)
            .children(picker)
            .child(
                div()
                    .text_color(if capturing { t.ok } else { t.text_muted })
                    .child(status),
            );
        // Segmented long collection stays available to the QA scripts only.
        if self.collection_open {
            root = root.child(self.collection.clone());
        }
        let Some(data) = &self.data else {
            return root.child(div().p(px(20.)).rounded(px(6.)).bg(t.panel).text_color(t.text_muted).child(
                "Nenhum pacote ainda. Escolha a interface e clique em Iniciar captura, ou abra um arquivo PCAP/PCAPNG.",
            ));
        };
        let capture = &data.capture;
        root=root.child(div().px(px(12.)).py(px(8.)).bg(t.panel).rounded(px(6.)).flex().flex_col().gap(px(4.))
            .child(div().flex().flex_wrap().items_baseline().gap(px(10.))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(self.label.clone()))
                .child(div().text_size(px(12.)).text_color(t.text_muted).child(format!("{} · {} pacotes · {} interfaces · {} bytes",capture.format.label(),capture.total_packets,capture.interfaces.len(),capture.file_bytes))))
            .when(!capture.sha256.is_empty(),|d|d.child(div().font_family(MONO_FONT).text_size(px(11.)).text_color(t.text_muted).child(format!("SHA-256 {}",capture.sha256))))
            .when(capture.limited(),|d|d.child(div().text_color(t.warning).child(format!("Amostragem: {} pacotes na tabela; {} adicionais entraram nos totais. Até 2048 bytes por amostra. O hash identifica o dump inteiro.",data.sample_count,capture.total_packets-data.sample_count as u64))))
            .when(capture.truncated_packets>0,|d|d.child(format!("{} pacotes têm menos bytes capturados que o tamanho original.",capture.truncated_packets)))
            .when(capture.auxiliary_blocks>0,|d|d.child(format!("{} blocos auxiliares/não reconhecidos não foram interpretados.",capture.auxiliary_blocks))));
        if let Some(report) = &data.report {
            root=root.child(div().text_size(px(12.)).text_color(t.text_muted).child(format!(
                "Analisados: {} · TCP RST: {} · janelas: {} × {} s · destinos excedentes: {} pacotes",report.packets,report.tcp_resets,report.windows.len(),report.window_seconds,report.destination_overflow_packets)));
            if let Some(baseline) = &self.baseline {
                let findings = report.compare(baseline);
                root = root.child(div().text_size(px(12.)).child(format!(
                    "Referência {}… · {} pacotes · {} diferenças pelos limiares",
                    &baseline.sha256[..12],
                    baseline.packets,
                    findings.len()
                )));
                for finding in findings.iter().take(6) {
                    let mut row = div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .text_color(t.warning)
                        .child(format!(
                            "{} · pacotes {:?}",
                            finding.description, finding.packet_references
                        ));
                    let mut links = div().flex().flex_wrap().gap(px(6.));
                    for number in finding
                        .packet_references
                        .iter()
                        .take(2)
                        .copied()
                        .collect::<std::collections::BTreeSet<_>>()
                    {
                        links = links.child(
                            Self::button(
                                format!("finding-{}-{number}", finding.kind),
                                &format!("Abrir #{number}"),
                                cx,
                            )
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.recover_number(number, cx)),
                            ),
                        );
                    }
                    row = row.child(links);
                    root = root.child(row);
                }
            }
        }
        let input = |child: Entity<TextInput>| {
            div()
                .h(px(30.))
                .px(px(8.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(t.border)
                .bg(t.editor)
                .overflow_hidden()
                .child(child)
        };
        // Search, out-of-sample recovery and match count share one row.
        root = root.child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .child(input(self.filter.clone()).flex_1().min_w(px(260.)))
                .child(input(self.packet_number.clone()).w(px(190.)))
                .child(
                    Self::button("recover-packet-number", "Abrir por número", cx)
                        .on_click(cx.listener(|this, _, _, cx| this.recover_input(cx))),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(t.text_muted)
                        .child(format!("{} na busca", self.matching.len())),
                ),
        );
        if capture.packets.len() > data.sample_count {
            root = root.child(div().text_size(px(11.)).text_color(t.text_muted)
                .child("1 pacote recuperado fora da amostra, exibido nos detalhes. A busca e a exportação sem seleção continuam usando as amostras distribuídas."));
        }
        let cell = |text: String, width: f32, color: gpui::Hsla| {
            div()
                .w(px(width))
                .flex_none()
                .overflow_hidden()
                .text_color(color)
                .child(text)
        };
        let table = div()
            .id("packet-table-scroll")
            .h(px(if self.selected.is_some() { 300. } else { 420. }))
            .min_w_0()
            .overflow_x_scroll()
            .border_1()
            .border_color(t.border)
            .child(
                div()
                    .min_w(px(1230.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .font_family(MONO_FONT)
                    .text_size(px(11.5))
                    .child(
                        div()
                            .h(px(28.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(8.))
                            .bg(t.panel)
                            .child(cell("Nº".into(), 60., t.text_muted))
                            .child(cell("Hora UTC".into(), 108., t.text_muted))
                            .child(cell("Prot.".into(), 56., t.text_muted))
                            .child(cell("Origem:porta".into(), 235., t.text_muted))
                            .child(cell("Destino:porta".into(), 235., t.text_muted))
                            .child(cell("Serviço¹".into(), 70., t.text_muted))
                            .child(cell("Bytes".into(), 70., t.text_muted))
                            .child(cell("Dados / flags".into(), 310., t.text_muted)),
                    )
                    .child(
                        uniform_list(
                            "packet-virtual-list",
                            self.matching.len(),
                            cx.processor(|panel, range: std::ops::Range<usize>, _, cx| {
                                let t = cx.theme();
                                let data = panel.data.as_ref().unwrap();
                                range
                                    .map(|row| {
                                        let index = panel.matching[row];
                                        let p = &data.capture.packets[index];
                                        let s = &data.summaries[index];
                                        let cell = |text: String, width: f32, color: gpui::Hsla| {
                                            div()
                                                .w(px(width))
                                                .flex_none()
                                                .overflow_hidden()
                                                .text_color(color)
                                                .child(text)
                                        };
                                        div()
                                            .id(("packet-row", index))
                                            .h(px(24.))
                                            .flex()
                                            .items_center()
                                            .gap(px(10.))
                                            .px(px(8.))
                                            .bg(if panel.selected == Some(index) {
                                                t.selected
                                            } else if row % 2 == 0 {
                                                t.editor
                                            } else {
                                                t.panel
                                            })
                                            .cursor_pointer()
                                            .hover(|d| d.bg(t.hover))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                // Keep the chosen row in view while packets arrive.
                                                this.follow = false;
                                                this.selected = Some(index);
                                                this.show_bytes = false;
                                                cx.notify();
                                            }))
                                            .child(cell(p.number.to_string(), 60., t.text_muted))
                                            .child(cell(
                                                p.timestamp.map_or_else(
                                                    || "Sem hora".into(),
                                                    |ts| ts.clock_label(),
                                                ),
                                                108.,
                                                t.ok,
                                            ))
                                            .child(cell(
                                                s.protocol.into(),
                                                56.,
                                                if s.protocol == "TCP" {
                                                    t.warning
                                                } else {
                                                    t.accent
                                                },
                                            ))
                                            .child(cell(s.source.clone(), 235., t.accent))
                                            .child(cell(s.destination.clone(), 235., t.text))
                                            .child(cell(s.service.into(), 70., t.text_muted))
                                            .child(cell(
                                                p.original_len.to_string(),
                                                70.,
                                                t.text_muted,
                                            ))
                                            .child(cell(
                                                if panel.raw {
                                                    let raw = s.raw_preview(&p.bytes);
                                                    if raw.is_empty() {
                                                        s.info.clone()
                                                    } else {
                                                        raw
                                                    }
                                                } else {
                                                    s.info.clone()
                                                },
                                                310.,
                                                t.text,
                                            ))
                                    })
                                    .collect()
                            }),
                        )
                        .track_scroll(&self.list_scroll)
                        .flex_1()
                        .min_h_0(),
                    ),
            );
        root = root.child(table);
        if self.matching.is_empty() {
            root = root.child(
                div()
                    .text_color(t.text_muted)
                    .child("Nenhum pacote corresponde à busca."),
            );
        }
        if let Some(index) = self.selected {
            let packet = &capture.packets[index];
            let summary = &data.summaries[index];
            let iface = &capture.interfaces[packet.interface];
            let mut details = div()
                .p(px(16.))
                .bg(t.panel)
                .rounded(px(6.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("Pacote #{} · {}", packet.number, summary.protocol)),
                )
                .child(format!("{} → {}", summary.source, summary.destination))
                .child(summary.info.clone())
                .child(format!(
                    "{} · seção {} / interface {} · enlace {}",
                    iface.name,
                    iface.section + 1,
                    iface.section_id + 1,
                    iface.link_type
                ))
                .child(format!(
                    "{} bytes capturados / {} originais · {} retidos na amostra · posição {} no arquivo",
                    packet.captured_len, packet.original_len, packet.bytes.len(), packet.file_offset
                ))
                .child(packet.timestamp.map_or_else(
                    || "Este pacote não contém horário de captura.".into(),
                    |ts| {
                        format!(
                            "Horário Unix (ns): {} · resolução original 1/{}^{} s",
                            ts.unix_nanos(),
                            if ts.resolution & 0x80 == 0 { 10 } else { 2 },
                            ts.resolution & 0x7f
                        )
                    },
                ))
                .child(
                    Self::button(
                        "packet-show-bytes",
                        if self.show_bytes {
                            "Ocultar bytes"
                        } else {
                            "Ver bytes deste pacote"
                        },
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_bytes = !this.show_bytes;
                        cx.notify();
                    })),
                )
                .child(
                    Self::button("packet-prepare-codex", "Preparar para Codex", cx)
                        .on_click(cx.listener(|this, _, _, cx| this.prepare_codex(cx))),
                )
                .child(Self::button("packet-replace-codex", "Somente este pacote", cx)
                    .on_click(cx.listener(|this, _, _, cx| this.prepare_codex_selection(true, cx))));
            if self.show_bytes {
                details = details.child(
                    div()
                        .font_family(MONO_FONT)
                        .text_size(px(11.))
                        .child(packet.hex_preview()),
                );
            }
            root = root.child(details);
        }
        root.child(div().text_size(px(11.)).text_color(t.text_muted).child(format!("¹ Serviço estimado pela porta. Captura ao vivo: até {} pacotes / {} MiB em memória, {} bytes por pacote, sem modo promíscuo. RAW exibe até 96 bytes imprimíveis do payload retido; sem remontagem ou decifração TLS. Captura longa: grava tudo em arquivos PCAP de até 64 MiB ou 60 s, numa pasta nova em {}, até o fim do período ou 4 GiB. Exportar seleção: pacote selecionado, ou até 512 amostras da busca, em pasta nova com recibo.", live_capture::MAX_PACKETS, live_capture::MAX_BYTES / 1024 / 1024, live_capture::SNAPLEN, self.captures.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_packets_build_the_same_pcap_the_analyzer_reads_and_stop_at_the_bound() {
        let example = netlane_capture::example();
        let mut data = Inspection::live(1, "Interface de teste".into());
        let mut file = live_capture::pcap_header(1, live_capture::SNAPLEN);
        for (i, packet) in example.packets.iter().enumerate() {
            let record = Record {
                seconds: 1_791_590_400,
                micros: i as u32 * 1000,
                original_len: packet.original_len + 10,
                bytes: packet.bytes.clone(),
            };
            assert!(data.push_live(record, &mut file));
        }
        assert_eq!(data.capture.total_packets, 6);
        assert_eq!(data.sample_count, 6);
        assert_eq!(data.capture.truncated_packets, 6);
        assert_eq!(data.matches("udp").len(), 6);
        let scan = netlane_capture::analysis::read_dump(
            std::io::Cursor::new(file.clone()),
            &AtomicBool::new(false),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(scan.report.packets, 6);
        for (live, read) in data.capture.packets.iter().zip(&scan.capture.packets) {
            assert_eq!(
                (live.number, live.file_offset, live.timestamp, &live.bytes),
                (read.number, read.file_offset, read.timestamp, &read.bytes)
            );
            assert_eq!(
                (live.captured_len, live.original_len),
                (read.captured_len, read.original_len)
            );
        }
        // At the packet bound nothing is appended: table and PCAP image stay consistent.
        while data.capture.packets.len() < live_capture::MAX_PACKETS {
            let bytes = example.packets[0].bytes.clone();
            assert!(data.push_live(
                Record {
                    seconds: 1_791_590_401,
                    micros: 0,
                    original_len: bytes.len() as u32,
                    bytes,
                },
                &mut file,
            ));
        }
        let before = file.len();
        let extra = Record {
            seconds: 1_791_590_402,
            micros: 0,
            original_len: 42,
            bytes: vec![0; 42],
        };
        assert!(!data.push_live(extra, &mut file));
        assert_eq!(file.len(), before);
        assert_eq!(data.capture.total_packets, live_capture::MAX_PACKETS as u64);
        assert_eq!(data.capture.file_bytes, before as u64);
    }
    #[test]
    fn follow_snapshot_context_from_a_live_capture_is_valid_and_metadata_only() {
        let example = netlane_capture::example();
        let mut data = Inspection::live(1, "Interface de teste".into());
        data.collection_note = Some("Origem: captura ao vivo de teste.\n".into());
        let mut file = live_capture::pcap_header(1, live_capture::SNAPLEN);
        // Larger than the analyzer's 512 distributed samples.
        for i in 0..1500u32 {
            let mut bytes = example.packets[(i % 6) as usize].bytes.clone();
            bytes.extend(b"payload-secreto");
            let record = Record {
                seconds: 1_791_590_400 + i / 10,
                micros: i % 10 * 1000,
                original_len: bytes.len() as u32,
                bytes,
            };
            assert!(data.push_live(record, &mut file));
        }
        let scan = netlane_capture::analysis::read_dump(
            std::io::Cursor::new(file),
            &AtomicBool::new(false),
            &Progress::default(),
        )
        .unwrap();
        let mut snapshot = Inspection::scan(scan);
        snapshot.collection_note = data.collection_note.clone();
        let samples = snapshot.capture.packets.len();
        let newest: Vec<usize> = (samples.saturating_sub(8)..samples).collect();
        let context = snapshot.evidence(&newest, None);
        context.validate().unwrap();
        assert_eq!(context.total_packets, 1500);
        assert_eq!(context.packets.len(), 8);
        assert_eq!(context.packets.last().unwrap().number, 1500);
        let prompt = context
            .prompt("", "Acompanhamento automático: o que mudou?")
            .unwrap();
        assert!(!prompt.contains("payload-secreto"));
    }
    #[test]
    fn long_mode_window_keeps_the_newest_packets_and_counts_the_rest() {
        let bytes = netlane_capture::example().packets[0].bytes.clone();
        let mut data = Inspection::live(1, "Interface de teste".into());
        let total = live_capture::MAX_PACKETS + 25;
        let mut dropped = 0;
        for i in 0..total {
            dropped += data.push_window(Record {
                seconds: 1_791_590_400 + i as u32,
                micros: 0,
                original_len: bytes.len() as u32,
                bytes: bytes.clone(),
            });
        }
        assert_eq!(dropped, 25);
        assert_eq!(data.capture.packets.len(), live_capture::MAX_PACKETS);
        assert_eq!(data.capture.total_packets, total as u64);
        assert_eq!(data.capture.omitted_packets, 25);
        assert_eq!(data.capture.packets[0].number, 26);
        assert_eq!(data.capture.packets.last().unwrap().number, total as u64);
        assert_eq!(data.summaries.len(), data.capture.packets.len());
        assert_eq!(data.matches("").len(), live_capture::MAX_PACKETS);
        assert_eq!(
            data.capture.retained_bytes,
            live_capture::MAX_PACKETS * bytes.len()
        );
        // The byte bound also trims, but always keeps the newest packet.
        let mut big = Inspection::live(1, "Interface de teste".into());
        let large = vec![0u8; 16 * 1024 * 1024];
        for _ in 0..3 {
            big.push_window(Record {
                seconds: 1_791_590_400,
                micros: 0,
                original_len: large.len() as u32,
                bytes: large.clone(),
            });
        }
        assert_eq!(big.capture.packets.len(), 2);
        assert!(big.capture.retained_bytes <= live_capture::MAX_BYTES);
        assert_eq!(big.capture.packets[0].number, 2);
    }
    #[test]
    fn search_indexes_metadata_and_excludes_packet_payloads() {
        let mut capture = netlane_capture::example();
        let packet = &mut capture.packets[0];
        let payload = b"private-payload-marker";
        packet.bytes.extend(payload);
        packet.original_len = packet.bytes.len() as u32;
        packet.bytes[16..18].copy_from_slice(&(28 + payload.len() as u16).to_be_bytes());
        packet.bytes[38..40].copy_from_slice(&(8 + payload.len() as u16).to_be_bytes());
        let data = Inspection::new(capture);
        assert_eq!(data.matches("udp").len(), 6);
        assert_eq!(data.matches("50002"), vec![2]);
        assert!(data.matches("not-present").is_empty());
        assert_eq!(data.matches("").len(), 6);
        assert!(data.matches("private-payload-marker").is_empty());
    }
    #[test]
    fn recovered_preview_is_bounded_and_keeps_distributed_search_and_export_intact() {
        let root = tempfile::tempdir().unwrap();
        let fixtures = root.path().join("fixtures");
        netlane_capture::qa::generate(
            &fixtures,
            netlane_capture::qa::FixtureOptions {
                records: 2000,
                packet_bytes: 2112,
            },
        )
        .unwrap();
        let path = fixtures.join("deviation.pcap");
        let stop = AtomicBool::new(false);
        let mut data = Inspection::scan(
            netlane_capture::analysis::load_dump(&path, &stop, &Progress::default()).unwrap(),
        );
        let original: Vec<_> = data.capture.packets.iter().map(|p| p.number).collect();
        let missing: Vec<_> = (1..=2000)
            .filter(|n| !original.contains(n))
            .take(2)
            .collect();
        for &number in &missing {
            let anchor = netlane_capture::lookup::PacketAnchor::new(&data.capture, number).unwrap();
            let recovered =
                netlane_capture::lookup::load_packet(&path, &anchor, &stop, &Progress::default())
                    .unwrap();
            let index = data.install_recovered(recovered).unwrap();
            assert_eq!(index, data.sample_count);
            assert_eq!(data.capture.packets.len(), data.sample_count + 1);
            assert!(data.capture.retained_bytes <= (data.sample_count + 1) * 2048);
            assert_eq!(data.matches("").len(), data.sample_count);
            assert_eq!(
                data.capture.packets[..data.sample_count]
                    .iter()
                    .map(|p| p.number)
                    .collect::<Vec<_>>(),
                original
            );
            let folder = root.path().join(format!("export-{number}"));
            let receipt = netlane_capture::export::Selection::new(&data.capture, &[index])
                .unwrap()
                .save(&folder, netlane_capture::Format::PcapNg)
                .unwrap();
            let exported = netlane_capture::load(&folder.join(receipt.file), &stop).unwrap();
            assert_eq!(exported.packets[0].bytes, data.capture.packets[index].bytes);
            assert_eq!(exported.packets[0].original_len, 2112);
            assert!(
                netlane_capture::export::Selection::new(&data.capture, &data.matches("")).is_ok()
            );
        }
        let anchor = netlane_capture::lookup::PacketAnchor::new(&data.capture, missing[0]).unwrap();
        let mut foreign =
            netlane_capture::lookup::load_packet(&path, &anchor, &stop, &Progress::default())
                .unwrap();
        foreign.sha256 = "0".repeat(64);
        assert!(data.install_recovered(foreign).is_err());
        assert_eq!(data.capture.packets.last().unwrap().number, missing[1]);
    }
}
