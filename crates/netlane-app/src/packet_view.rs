//! Local import. Only selected metadata can enter an explicitly reviewed context.
use crate::{
    codex_view::CodexView,
    collection_view::{CollectionView, SegmentReady},
    text_input::TextInput,
    theme::{ActiveTheme, MONO_FONT},
};
use gpui::{
    App, Context, Entity, FontWeight, Render, Subscription, Window, div, prelude::*, px,
    uniform_list,
};
use netlane_capture::analysis::{Progress, Report, Scan};
use netlane_capture::{CaptureFile, Summary};
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
            capture,
            summaries,
            search,
            report: None,
            collection_note: None,
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
            capture: scan.capture,
            summaries: scan.summaries,
            search,
            report: Some(scan.report),
            collection_note: None,
        }
    }
    fn matches(&self, query: &str) -> Vec<usize> {
        self.search
            .iter()
            .enumerate()
            .filter(|(_, s)| s.contains(query))
            .map(|(i, _)| i)
            .collect()
    }
}

pub struct PacketView {
    demo: bool,
    data: Option<Inspection>,
    label: String,
    status: String,
    filter: Entity<TextInput>,
    matching: Vec<usize>,
    selected: Option<usize>,
    show_bytes: bool,
    raw: bool,
    baseline: Option<Report>,
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
        let subscription = cx.subscribe(&filter, |this, _, _, cx| {
            this.refilter(cx);
            cx.notify();
        });
        let collection = cx.new(|cx| {
            CollectionView::new(demo, profile, dialog.clone(), capture_active.clone(), cx)
        });
        let collection_subscription=cx.subscribe(&collection,|this,_,event:&SegmentReady,cx|{
            if let Some((scan,note))=event.0.borrow_mut().take(){
                let mut data=Inspection::scan(scan);data.collection_note=Some(note);this.data=Some(data);this.selected=None;this.show_bytes=false;
                this.label="Segmento fechado da coleta · prévia recebida mais recentemente".into();
                this.status="Segmento completo analisado. Os limites/perdas da coleta aparecem no painel acima.".into();this.refilter(cx);cx.notify();
            }
        });
        Self {
            demo,
            data: None,
            label: String::new(),
            status: "Abra uma captura PCAP/PCAPNG ou explore o exemplo fictício.".into(),
            filter,
            matching: vec![],
            selected: None,
            show_bytes: false,
            raw: false,
            baseline: None,
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
        if self.selected.is_some_and(|i| !self.matching.contains(&i)) {
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
                        "{}{}",
                        report.summary_text(self.baseline.as_ref()),
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
        self.codex
            .update(cx, |panel, cx| panel.prepare(context, cx));
    }
    fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.cancel.store(true, Ordering::Relaxed);
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
                    this.status = format!(
                        "Análise em andamento · {} pacotes · {:.1} MiB lidos · {} RST TCP",
                        progress.packets.load(Ordering::Relaxed),
                        progress.bytes.load(Ordering::Relaxed) as f64 / 1048576.,
                        progress.resets.load(Ordering::Relaxed)
                    );
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
                this.busy=true;this.collection.update(cx,|c,cx|c.set_importing(true,cx));this.generation=this.generation.wrapping_add(1);this.cancel=Arc::new(AtomicBool::new(false));
                this.progress=Arc::new(Progress::default());
                this.status="Analisando o dump inteiro em segundo plano…".into();this.start_progress(cx);cx.notify();Some((this.generation,this.cancel.clone(),this.progress.clone()))
            });
            let Ok(Some((generation,cancel,progress)))=request else {return;};
            let result=cx.background_executor().spawn(async move{
                netlane_capture::analysis::load_dump(&path,&cancel,&progress).map(Inspection::scan)
            }).await;
            let _=this.update(cx,|this,cx|{
                this.busy=false;this.collection.update(cx,|c,cx|c.set_importing(false,cx));
                if generation!=this.generation {this.status="Leitura cancelada. A captura atual foi preservada.".into();cx.notify();return;}
                match result {
                    Ok(data)=>{
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
    pub fn proof_matches(&self) -> usize {
        self.matching.len()
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_comparison(&mut self, cx: &mut Context<Self>) {
        self.baseline = Some(netlane_capture::comparison_example(true).report);
        self.data = Some(Inspection::scan(netlane_capture::comparison_example(false)));
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
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(
                            Self::button("prepare-dump-codex", "Preparar análise para Codex", cx)
                                .on_click(cx.listener(|this, _, _, cx| this.prepare_codex(cx))),
                        )
                    })
                    .when(self.baseline.is_some(), |row| {
                        row.child(
                            Self::button("clear-baseline", "Remover referência", cx).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.baseline = None;
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
        root=root.child(div().p(px(14.)).bg(t.panel).rounded(px(6.)).flex().flex_col().gap(px(5.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(self.label.clone()))
            .child(format!("{} · {} pacotes · {} interfaces · {} bytes",capture.format.label(),capture.total_packets,capture.interfaces.len(),capture.file_bytes))
            .child(div().font_family(MONO_FONT).text_size(px(11.)).child(format!("SHA-256 {}",capture.sha256)))
            .when(capture.limited(),|d|d.child(div().text_color(t.warning).child(format!("Amostragem: {} pacotes na tabela; {} adicionais entraram nos totais. Até 2048 bytes por amostra. O hash identifica o dump inteiro.",capture.packets.len(),capture.omitted_packets))))
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
                    root = root.child(div().text_color(t.warning).child(format!(
                        "{} · pacotes {:?}",
                        finding.description, finding.packet_references
                    )));
                }
            }
        }
        root = root.child(self.filter.clone());
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
                );
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
}
