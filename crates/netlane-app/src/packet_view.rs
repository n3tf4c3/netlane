//! Local import. Only selected metadata can enter an explicitly reviewed context.
use crate::{
    codex_view::{CodexView, bundle::InvestigationRestored},
    collection_view::{CollectionView, SegmentReady},
    text_input::TextInput,
    theme::{ActiveTheme, MONO_FONT},
};
use gpui::{
    App, Context, Entity, FontWeight, Render, Subscription, Window, div, prelude::*, px,
    uniform_list,
};
use netlane_capture::analysis::{Progress, Report, Scan};
use netlane_capture::{CaptureFile, Summary, lookup::RecoveredPacket};
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
            status: "Abra uma captura PCAP/PCAPNG ou explore o exemplo fictício.".into(),
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
        let packet = &data.capture.packets[index];
        let summary = &data.summaries[index];
        let interface = &data.capture.interfaces[packet.interface];
        let context = netlane_ai::Context {
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
                        report.summary_text(self.baseline.as_ref()),
                        report.first_unix_nanos.as_deref().unwrap_or("ausente"),
                        report.last_unix_nanos.as_deref().unwrap_or("ausente"),
                        data.capture.packets.len().saturating_sub(data.sample_count),
                        data.collection_note.as_deref().unwrap_or("")
                    ),
                    analyzed_packets: report.packets,
                    sample_packets: report.sample_packets,
                    baseline_sha256: self.baseline.as_ref().map(|r| r.sha256.clone()),
                    packet_references: self.baseline.as_ref().map_or_else(Vec::new, |b| {
                        report
                            .compare(b)
                            .into_iter()
                            .flat_map(|f| f.packet_references)
                            .take(24)
                            .collect()
                    }),
                }),
            packets: vec![netlane_ai::PacketEvidence {
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
            }],
        };
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
        let mut root = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .min_w_0()
            .child(
                div()
                    .text_size(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text_strong)
                    .child("Pacotes"),
            )
            .child(
                div()
                    .text_color(t.text_muted)
                    .child("Inspecione uma captura de rede. A análise permanece neste computador."),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(
                        Self::button("import-capture", "Abrir PCAP / PCAPNG", cx)
                            .on_click(cx.listener(|this, _, w, cx| this.import(w, cx))),
                    )
                    .child(
                        Self::button("example-capture", "Exemplo fictício", cx)
                            .on_click(cx.listener(|this, _, _, cx| this.example(cx))),
                    )
                    .child(
                        Self::button("clear-capture", "Limpar", cx)
                            .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
                    )
                    .child(
                        Self::button(
                            "collection-toggle",
                            if self.collection_open {
                                "Ocultar coleta longa"
                            } else {
                                "Captura longa"
                            },
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.collection_open = !this.collection_open;
                            cx.notify();
                        })),
                    )
                    .child(
                        Self::button(
                            "packets-raw",
                            if self.raw {
                                "RAW: exibindo"
                            } else {
                                "RAW: oculto"
                            },
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.raw = !this.raw;
                            cx.notify();
                        })),
                    )
                    .when(self.data.is_some(), |row| {
                        row.child(
                            Self::button("packets-baseline", "Usar como referência", cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.baseline =
                                        this.data.as_ref().and_then(|data| data.report.clone());
                                    this.baseline_source =
                                        this.data.as_ref().and_then(Inspection::source);
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(
                            Self::button("prepare-dump-codex", "Preparar análise para Codex", cx)
                                .on_click(cx.listener(|this, _, _, cx| this.prepare_codex(cx))),
                        )
                        .child(
                            Self::button("export-pcapng", "Exportar PCAPNG", cx).on_click(
                                cx.listener(|this, _, w, cx| {
                                    this.export_selection(netlane_capture::Format::PcapNg, w, cx)
                                }),
                            ),
                        )
                        .child(
                            Self::button("export-pcap", "Exportar PCAP", cx).on_click(cx.listener(
                                |this, _, w, cx| {
                                    this.export_selection(netlane_capture::Format::Pcap, w, cx)
                                },
                            )),
                        )
                    })
                    .when(self.baseline.is_some(), |row| {
                        row.child(
                            Self::button("clear-baseline", "Remover referência", cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.baseline = None;
                                    this.baseline_source = None;
                                    cx.notify();
                                }),
                            ),
                        )
                    })
                    .when(self.busy, |row| {
                        row.child(
                            Self::button("cancel-capture-import", "Cancelar leitura", cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.cancel();
                                    this.status = "Encerrando a leitura…".into();
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
            )
            .child(div().text_color(t.text_muted).child(self.status.clone()));
        if self.collection_open || self.capture_active.load(Ordering::Relaxed) {
            root = root.child(self.collection.clone());
        }
        let Some(data) = &self.data else {
            return root.child(div().p(px(24.)).rounded(px(6.)).bg(t.panel).child(
                "Escolha uma captura para começar ou explore os pacotes do exemplo fictício.",
            ));
        };
        let capture = &data.capture;
        root=root.child(div().text_size(px(11.)).text_color(t.text_muted).child(
            "Exportação: pacote selecionado, ou amostras da busca se não houver seleção. Inclui os bytes retidos (até 2 KiB por amostra), em uma pasta nova com recibo; não copia o dump inteiro."));
        root=root.child(div().p(px(14.)).bg(t.panel).rounded(px(6.)).flex().flex_col().gap(px(5.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(self.label.clone()))
            .child(format!("{} · {} pacotes · {} interfaces · {} bytes",capture.format.label(),capture.total_packets,capture.interfaces.len(),capture.file_bytes))
            .child(div().font_family(MONO_FONT).text_size(px(11.)).child(format!("SHA-256 {}",capture.sha256)))
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
        root = root.child(self.filter.clone());
        root = root.child(div().flex().flex_wrap().items_center().gap(px(8.))
            .child(div().w(px(260.)).child(self.packet_number.clone()))
            .child(Self::button("recover-packet-number", "Abrir por número", cx)
                .on_click(cx.listener(|this, _, _, cx| this.recover_input(cx))))
            .child(div().text_size(px(11.)).text_color(t.text_muted)
                .child("Fora da amostra: confere o dump original em segundo plano; leitura cancelável.")));
        if capture.packets.len() > data.sample_count {
            root = root.child(div().text_size(px(11.)).text_color(t.text_muted)
                .child("1 pacote recuperado fora da amostra, exibido nos detalhes. A busca e a exportação sem seleção continuam usando as amostras distribuídas."));
        }
        root = root.child(div().text_color(t.text_muted).child(format!(
            "{} amostras correspondem à busca · rolagem virtualizada · serviço estimado pela porta",
            self.matching.len()
        )));
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
            .h(px(220.))
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
        root.child(div().text_size(px(11.)).text_color(t.text_muted).child("¹ Serviço estimado pela porta. RAW exibe até 96 bytes imprimíveis do payload retido. Sem remontagem ou decifração TLS; comparar capturas com pontos/filtros diferentes exige revisão."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
