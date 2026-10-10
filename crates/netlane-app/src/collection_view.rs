//! Explicit collection controls; demo always uses a generated source.
use crate::{text_input::TextInput, theme::ActiveTheme};
use gpui::{App, Context, Entity, EventEmitter, FontWeight, Render, Window, div, prelude::*, px};
use netlane_capture::{
    analysis::Scan,
    collection::{self, Options, Progress, Receipt},
};
use netlane_windows::npcap::Device;
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
pub struct SegmentReady(pub RefCell<Option<(Scan, String)>>);
pub struct CollectionView {
    demo: bool,
    profile: PathBuf,
    dialog: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
    devices: Vec<Device>,
    selected: Option<usize>,
    parent: Option<PathBuf>,
    filter: Entity<TextInput>,
    status: String,
    listing: bool,
    stop: Arc<AtomicBool>,
    progress: Arc<Progress>,
    events: Option<Receiver<Scan>>,
    receipt: Option<Receipt>,
    options: Option<Options>,
    disk_index: usize,
    duration_index: usize,
    importing: bool,
}
impl EventEmitter<SegmentReady> for CollectionView {}
impl Drop for CollectionView {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl CollectionView {
    pub fn new(
        demo: bool,
        profile: PathBuf,
        dialog: Arc<AtomicBool>,
        active: Arc<AtomicBool>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {demo,profile,dialog,active,devices:vec![],selected:None,parent:None,filter:cx.new(|cx|TextInput::new("","Filtro BPF opcional: tcp or udp",cx)),
            status:"Escolha interface, destino e limites antes de iniciar. Npcap instalado é necessário para coleta real.".into(),listing:false,
            stop:Arc::new(AtomicBool::new(false)),progress:Arc::new(Progress::default()),events:None,receipt:None,options:None,disk_index:1,duration_index:2,importing:false}
    }
    pub fn running(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }
    pub fn set_importing(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.importing = busy;
        cx.notify();
    }
    fn list(&mut self, cx: &mut Context<Self>) {
        if self.running() || self.listing {
            return;
        }
        if self.demo {
            self.devices = vec![Device {
                name: "synthetic-no-network".into(),
                description: "Interface fictícia · sem rede".into(),
            }];
            self.selected = Some(0);
            cx.notify();
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
                        this.devices = devices;
                        this.selected = None;
                        this.status =
                            "Selecione uma interface da lista; listar não iniciou captura.".into();
                    }
                    Err(e) => this.status = e.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running() || self.dialog.load(Ordering::Relaxed) {
            return;
        }
        if self.demo {
            self.parent = Some(self.profile.join("synthetic-captures"));
            cx.notify();
            return;
        }
        self.dialog.store(true, Ordering::Relaxed);
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Escolha onde guardar uma nova pasta de coleta".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let paths = paths.await;
            let _ = this.update(cx, |this, cx| {
                this.dialog.store(false, Ordering::Relaxed);
                if let Ok(Ok(Some(paths))) = paths {
                    this.parent = paths.into_iter().next();
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn start(&mut self, cx: &mut Context<Self>) {
        if self.importing {
            self.status =
                "Aguarde a importação do dump terminar antes de iniciar uma coleta.".into();
            cx.notify();
            return;
        }
        if self.running() || self.listing || self.dialog.load(Ordering::Relaxed) {
            return;
        }
        let Some(device) = self.selected.and_then(|i| self.devices.get(i)).cloned() else {
            self.status = "Selecione uma interface de captura.".into();
            cx.notify();
            return;
        };
        let Some(parent) = self.parent.clone() else {
            self.status = "Escolha o destino dos arquivos PCAP.".into();
            cx.notify();
            return;
        };
        let mut options = Options {
            interface: device.name,
            filter: self.filter.read(cx).text().to_string(),
            total_bytes: [1, 4, 16, 64][self.disk_index] * 1024 * 1024 * 1024,
            duration_seconds: [1, 8, 24, 72][self.duration_index] * 3600,
            ..Options::default()
        };
        if self.demo {
            options.snaplen = 128;
            options.segment_bytes = 4096;
        }
        if let Err(e) = options.validate() {
            self.status = e.to_string();
            cx.notify();
            return;
        }
        let directory = parent.join(format!("NetLane-{}", uuid::Uuid::new_v4()));
        self.progress = Arc::new(Progress::default());
        self.stop = Arc::new(AtomicBool::new(false));
        self.receipt = None;
        self.options = Some(options.clone());
        self.active.store(true, Ordering::Relaxed);
        self.status = format!(
            "Coleta {} · arquivos em {}",
            if self.demo { "fictícia" } else { "iniciando" },
            directory.display()
        );
        let (tx, rx) = mpsc::sync_channel(2);
        self.events = Some(rx);
        let stop = self.stop.clone();
        let progress = self.progress.clone();
        let demo = self.demo;
        self.poll(cx);
        let display_directory = directory.clone();
        cx.spawn(async move|this,cx|{
            let result=cx.background_executor().spawn(async move{
                if demo {
                    std::fs::create_dir_all(&parent)?;
                    collection::run(directory,options,||Ok(Synthetic {remaining:300,index:0}),stop,progress,tx)
                }else{
                    let name=options.interface.clone();let filter=options.filter.clone();let snaplen=options.snaplen;
                    collection::run(directory,options,move||netlane_windows::npcap::Capture::open(&name,&filter,snaplen),stop,progress,tx)
                }
            }).await;
            let _=this.update(cx,|this,cx|{
                this.active.store(false,Ordering::Relaxed);
                match result {
                    Ok(receipt)=>{this.status=format!("{} · {} pacotes gravados · {} segmentos PCAP · fila: {} descartados · não gravados ao encerrar: {} · {} segmentos sem análise automática. Arquivos em {}",
                        receipt.reason,receipt.written,receipt.segments.len(),receipt.queue_dropped,receipt.unwritten,receipt.analysis_skipped,display_directory.display());this.receipt=Some(receipt);},
                    Err(e)=>this.status=format!("A coleta não foi concluída: {e:#}. Arquivos parciais existentes foram preservados."),
                }this.drain(cx);this.events=None;cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn drain(&mut self, cx: &mut Context<Self>) {
        if let Some(events) = &self.events {
            let mut latest = None;
            while let Ok(scan) = events.try_recv() {
                latest = Some(scan);
            }
            if let Some(scan) = latest {
                let options = self.options.as_ref().unwrap();
                let note = format!(
                    "Origem deste arquivo: segmento de coleta {}. Interface {:?}; filtro BPF {:?}; snaplen {}. Totais da coleta no momento da entrega: {} lidos, {} gravados, {} descartados na fila; {} segmentos sem análise automática, {} falhas de análise. Estatísticas do driver no fechamento: {:?}. Esses totais são da coleta; os agregados do dump e as referências pertencem somente ao segmento com o SHA informado. Zero perdas do driver não comprova ausência de perdas.",
                    if self.demo {
                        "fictícia"
                    } else {
                        "Windows/Npcap"
                    },
                    options.interface,
                    options.filter,
                    options.snaplen,
                    self.progress.seen.load(Ordering::Relaxed),
                    self.progress.written.load(Ordering::Relaxed),
                    self.progress.queue_dropped.load(Ordering::Relaxed),
                    self.progress.analysis_skipped.load(Ordering::Relaxed),
                    self.progress.analysis_failed.load(Ordering::Relaxed),
                    self.receipt.as_ref().and_then(|r| r.driver_stats)
                );
                cx.emit(SegmentReady(RefCell::new(Some((scan, note)))));
            }
        }
    }
    fn poll(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let Ok(done) = this.update(cx, |this, cx| {
                    this.drain(cx);
                    cx.notify();
                    !this.running()
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
    fn stop(&mut self, cx: &mut Context<Self>) {
        self.stop.store(true, Ordering::Relaxed);
        self.status = "Parada solicitada. Fechando arquivo, fonte e análise pendente…".into();
        cx.notify();
    }
    fn button(id: &'static str, label: &str, cx: &App) -> gpui::Stateful<gpui::Div> {
        let t = cx.theme();
        div()
            .id(id)
            .px(px(12.))
            .py(px(8.))
            .bg(t.hover)
            .border_1()
            .border_color(t.border)
            .rounded(px(4.))
            .cursor_pointer()
            .hover(|d| d.bg(t.selected))
            .child(label.to_string())
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_start(&mut self, cx: &mut Context<Self>) {
        assert!(self.demo);
        self.list(cx);
        self.parent = Some(self.profile.join("synthetic-captures"));
        self.start(cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_valid(&self) -> bool {
        self.receipt.as_ref().is_some_and(|r| {
            r.source_closed
                && r.written == 300
                && r.segments.len() > 1
                && r.analysis_failed == 0
                && !self.running()
        })
    }
}
struct Synthetic {
    remaining: u32,
    index: u32,
}
impl collection::Source for Synthetic {
    fn link_type(&self) -> u16 {
        1
    }
    fn poll(&mut self) -> anyhow::Result<collection::Poll> {
        if self.remaining == 0 {
            return Ok(collection::Poll::End);
        }
        std::thread::sleep(Duration::from_millis(1));
        self.remaining -= 1;
        self.index += 1;
        let mut bytes = netlane_capture::example().packets.remove(0).bytes;
        bytes.resize(128, 0);
        Ok(collection::Poll::Packet(collection::Record {
            seconds: 1_700_000_000 + self.index,
            micros: 0,
            original_len: 128,
            bytes,
        }))
    }
    fn stats(&mut self) -> Option<collection::DriverStats> {
        Some(collection::DriverStats {
            received: self.index,
            ..Default::default()
        })
    }
}
impl Render for CollectionView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let running = self.running();
        let p = &self.progress;
        let mut root = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(14.))
            .bg(t.panel)
            .rounded(px(6.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Coleta longa · segmentos PCAP e análise contínua"),
            )
            .child(self.status.clone());
        if running {
            root=root.child(format!("{} pacotes lidos · {} gravados · {:.1} MiB · {} segmentos · {} descartados na fila · {} análises puladas · {} falhas de análise",
            p.seen.load(Ordering::Relaxed),p.written.load(Ordering::Relaxed),p.bytes.load(Ordering::Relaxed) as f64/1048576.,p.segments.load(Ordering::Relaxed),p.queue_dropped.load(Ordering::Relaxed),p.analysis_skipped.load(Ordering::Relaxed),p.analysis_failed.load(Ordering::Relaxed)))
            .child(Self::button("collection-stop","Parar e fechar coleta",cx).on_click(cx.listener(|this,_,_,cx|this.stop(cx))));
        } else {
            root = root.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(
                        Self::button(
                            "collection-devices",
                            if self.listing {
                                "Listando…"
                            } else {
                                "Listar interfaces Npcap"
                            },
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.list(cx))),
                    )
                    .child(
                        Self::button("collection-directory", "Escolher destino", cx)
                            .on_click(cx.listener(|this, _, w, cx| this.choose(w, cx))),
                    )
                    .child(
                        Self::button(
                            "collection-disk",
                            &format!("Disco: até {} GiB", [1, 4, 16, 64][self.disk_index]),
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.disk_index = (this.disk_index + 1) % 4;
                            cx.notify();
                        })),
                    )
                    .child(
                        Self::button(
                            "collection-duration",
                            &format!("Duração: até {} h", [1, 8, 24, 72][self.duration_index]),
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.duration_index = (this.duration_index + 1) % 4;
                            cx.notify();
                        })),
                    )
                    .child(
                        Self::button(
                            "collection-start",
                            if self.demo {
                                "Simular coleta"
                            } else {
                                "Iniciar coleta nesta interface"
                            },
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                    ),
            );
            for (index, device) in self.devices.iter().enumerate() {
                root = root.child(
                    div()
                        .id(("collection-device", index))
                        .px(px(8.))
                        .py(px(5.))
                        .bg(if self.selected == Some(index) {
                            t.selected
                        } else {
                            t.editor
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = Some(index);
                            cx.notify();
                        }))
                        .child(format!(
                            "{} {} · {}",
                            if self.selected == Some(index) {
                                "✓"
                            } else {
                                "○"
                            },
                            device.description,
                            device.name
                        )),
                );
            }
            root = root
                .child(self.filter.clone())
                .child(self.parent.as_ref().map_or_else(
                    || "Destino ainda não escolhido.".into(),
                    |p| format!("Destino: {} · será criada uma nova pasta.", p.display()),
                ));
        }
        if let Some(receipt) = &self.receipt {
            let losses=receipt.driver_stats.map_or_else(||"Perdas do driver indisponíveis.".into(),|stats|format!("Driver: {} descartados no buffer · {} na interface. Zero pode significar estatística indisponível.",stats.dropped,stats.interface_dropped));
            root = root.child(format!(
                "{} · {} · {} falhas de análise · {} prévias não entregues à tabela.",
                if receipt.source_closed {
                    "Leitura encerrada"
                } else {
                    "Encerramento não confirmado"
                },
                losses,
                receipt.analysis_failed,
                receipt.display_skipped
            ));
        }
        let rotation = if self.demo {
            "Demonstração: segmentos de 4 KiB · até 128 bytes por pacote · fonte fictícia de 300 pacotes."
        } else {
            "Rotação a cada 60 s ou 64 MiB · até 2048 bytes por pacote · máximo 4096 segmentos."
        };
        root.child(div().text_size(px(12.)).text_color(t.text_muted).child(format!("{rotation} Ao atingir disco/duração, a coleta para; arquivos anteriores são preservados. A tabela mostra a prévia recebida mais recentemente. IA recebe contexto somente pelo envio revisado.")))
    }
}
