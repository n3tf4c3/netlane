use crate::{
    codex_view::{CodexView, ContextPrepared},
    demo,
    desktop::Desktop,
    executable_icons::ExecutableIcons,
    lifecycle::{ExitDecision, exit_decision},
    packet_view::PacketView,
    settings::{AppSettings, Settings},
    text_input::{TextInput, TextInputEvent},
    theme::{ActiveTheme, MONO_FONT, Themes, UI_FONT},
    theme_picker::{ThemePicker, ThemePickerEvent},
    ui::icon,
};
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, FontWeight, KeyBinding, Subscription, Window,
    actions, div, prelude::*, px,
};
use netlane_core::{
    NetworkSnapshot, RouteMode, RoutingPolicy,
    persistence::{PolicyFile, atomic_write, read_bounded, without_bom},
    traffic::TrafficTracker,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
#[path = "default_connection_view.rs"]
mod default_connection_view;
#[path = "quality_view.rs"]
mod quality_view;

actions!(workspace, [SelectTheme, SaveRules]);
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-t", SelectTheme, Some("NetLane")),
        KeyBinding::new("ctrl-s", SaveRules, Some("NetLane")),
    ]);
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Packets,
    Codex,
    Connections,
    Rules,
    Diagnostics,
    Settings,
}
impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Visão geral",
            Self::Packets => "Pacotes",
            Self::Codex => "Codex",
            Self::Connections => "Conexões",
            Self::Rules => "Regras",
            Self::Diagnostics => "Diagnóstico",
            Self::Settings => "Configurações",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Overview => "lan",
            Self::Packets => "search",
            Self::Codex => "info",
            Self::Connections => "desktop_windows",
            Self::Rules => "tune",
            Self::Diagnostics => "info",
            Self::Settings => "settings",
        }
    }
}
pub struct Workspace {
    pub page: Page,
    pub snapshot: NetworkSnapshot,
    pub policies: Vec<RoutingPolicy>,
    revision: Option<String>,
    policy_file: PolicyFile,
    profile: PathBuf,
    pub dirty: bool,
    error: Option<String>,
    notice: Option<String>,
    demo: bool,
    filter: Entity<TextInput>,
    executable: Entity<TextInput>,
    selected: Vec<String>,
    traffic: TrafficTracker,
    icons: ExecutableIcons,
    quality: netlane_core::quality::Monitor,
    quality_preferences: netlane_core::quality::Settings,
    quality_input: Entity<TextInput>,
    quality_status: String,
    quality_dns_busy: bool,
    default_snapshot: Option<netlane_core::default_connection::Snapshot>,
    default_journal: Option<netlane_core::default_connection::Journal>,
    default_revision: Option<String>,
    default_selected: Option<uuid::Uuid>,
    default_status: String,
    default_valid: bool,
    default_reading: bool,
    default_busy: bool,
    default_confirmation: Option<default_connection_view::Confirmation>,
    #[cfg(feature = "gui-test")]
    proof_quality_details: bool,
    started: Instant,
    controller: Option<netlane_agent::Controller>,
    allow_flags: bool,
    theme_picker: Option<Entity<ThemePicker>>,
    theme_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
    focus: FocusHandle,
    connection_page: usize,
    closing: bool,
    waiting_for_stop: bool,
    desktop: Option<Desktop>,
    startup_registration: Option<netlane_windows::startup::StartupRegistration>,
    startup_enabled: bool,
    file_dialog_open: bool,
    packets: Entity<PacketView>,
    codex: Entity<CodexView>,
    packet_dialog: std::sync::Arc<std::sync::atomic::AtomicBool>,
    capture_active: std::sync::Arc<std::sync::atomic::AtomicBool>,
    hide_pending: bool,
    #[cfg(feature = "gui-test")]
    proof_session: Option<netlane_agent::SessionState>,
}
impl Workspace {
    #[cfg(feature = "gui-test")]
    pub fn proof_prepare_icons(&mut self, cx: &mut Context<Self>) {
        assert!(self.demo && self.controller.is_none());
        self.icons = ExecutableIcons::new(true);
        let executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        self.policies[0].executable_path = Some(executable.clone());
        self.policies[0].application_id = "NetLane.exe (fixture)".into();
        self.snapshot.connections[0].executable = Some(executable);
        self.snapshot.connections[0].name = "NetLane.exe".into();
        let missing = self
            .profile
            .join("missing-icon-fixture.exe")
            .to_string_lossy()
            .into_owned();
        assert!(!std::path::Path::new(&missing).exists());
        self.policies[1].executable_path = Some(missing.clone());
        self.snapshot.connections[1].executable = Some(missing);
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_icons_ready(&self) -> bool {
        let (count, busy) = self.icons.stats();
        assert!(count <= 128 && busy <= 2);
        assert!(
            self.icons
                .image(self.policies[1].executable_path.as_deref())
                .is_none()
        );
        self.icons
            .image(self.policies[0].executable_path.as_deref())
            .is_some_and(|image| {
                assert_eq!(image.as_bytes(0).unwrap().len(), 32 * 32 * 4);
                assert_eq!(
                    self.icons
                        .image(self.snapshot.connections[0].executable.as_deref())
                        .unwrap()
                        .id,
                    image.id
                );
                true
            })
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_icon_remote_fallback(&mut self, cx: &mut Context<Self>) {
        assert!(self.demo);
        self.policies[0].executable_path = Some(r"\\unreachable-fixture\app.exe".into());
        self.snapshot.connections[0].executable = self.policies[0].executable_path.clone();
        assert!(
            self.icons
                .image(self.policies[0].executable_path.as_deref())
                .is_none()
        );
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_restore_icon_fixture(&mut self, cx: &mut Context<Self>) {
        let executable = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        self.policies[0].executable_path = Some(executable.clone());
        self.snapshot.connections[0].executable = Some(executable);
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_desktop(&self) -> &Desktop {
        self.desktop.as_ref().unwrap()
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_desktop_failure(&mut self) {
        self.desktop.as_mut().unwrap().simulate_failure();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_set_session(&mut self, state: netlane_agent::SessionState) {
        assert!(self.demo);
        self.proof_session = Some(state);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_exit_pending(&self) -> bool {
        self.waiting_for_stop
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_exit_confirmation(&self) -> bool {
        self.closing
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_cancel_exit(&mut self, cx: &mut Context<Self>) {
        self.closing = false;
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_discard_failure(&mut self, cx: &mut Context<Self>) {
        assert!(self.demo && self.dirty && !self.policy_file.path.exists());
        let previous = serde_json::to_value(&self.policies).unwrap();
        std::fs::write(&self.policy_file.path, "{invalid fixture").unwrap();
        self.demo = false;
        let discarded = self.discard(cx);
        self.demo = true;
        std::fs::remove_file(&self.policy_file.path).unwrap();
        assert!(!discarded && self.dirty);
        assert_eq!(serde_json::to_value(&self.policies).unwrap(), previous);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_open_theme_picker(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.open_theme_picker(w, cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_theme_filter(&mut self, text: &str, cx: &mut Context<Self>) {
        self.theme_picker
            .as_ref()
            .unwrap()
            .update(cx, |picker, cx| picker.proof_filter(text, cx));
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_theme_cancel(&mut self, cx: &mut Context<Self>) {
        let picker = self.theme_picker.as_ref().unwrap().clone();
        picker.update(cx, |picker, cx| picker.proof_cancel(cx));
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_theme_confirm(&mut self, cx: &mut Context<Self>) {
        let picker = self.theme_picker.as_ref().unwrap().clone();
        picker.update(cx, |picker, cx| picker.proof_confirm(cx));
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_edit_rule(&mut self, cx: &mut Context<Self>) {
        self.policies[0].enabled = false;
        self.dirty = true;
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_save(&mut self, cx: &mut Context<Self>) {
        self.save(cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_appearance(&mut self, font: u8, compact: bool, cx: &mut Context<Self>) {
        self.change_settings(cx, |s| {
            s.font_size = font;
            s.compact = compact;
        });
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_packets(&mut self, query: &str, select: bool, cx: &mut Context<Self>) {
        self.page = Page::Packets;
        self.packets.update(cx, |panel, cx| {
            panel.example(cx);
            panel.proof_filter(query, cx);
            if select {
                panel.proof_select();
            }
        });
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_packet_matches(&self, cx: &App) -> usize {
        self.packets.read(cx).proof_matches()
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_codex(&mut self, cx: &mut Context<Self>) {
        self.packets.update(cx, |panel, cx| panel.prepare_codex(cx));
        self.codex.update(cx, |panel, cx| panel.proof_review(cx));
        self.page = Page::Codex;
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_codex_context(&self, cx: &App) -> bool {
        self.codex.read(cx).proof_context()
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_long_dump(&mut self, cx: &mut Context<Self>) {
        self.packets
            .update(cx, |panel, cx| panel.proof_comparison(cx));
        self.page = Page::Packets;
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_long_valid(&self, cx: &App) -> bool {
        self.packets.read(cx).proof_scan_valid()
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_analyze_dump(&mut self, cx: &mut Context<Self>) {
        self.packets.update(cx, |panel, cx| panel.prepare_codex(cx));
        self.codex.update(cx, |panel, cx| panel.proof_analysis(cx));
        self.page = Page::Codex;
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_analyzed_valid(&self, cx: &App) -> bool {
        self.codex.read(cx).proof_answer()
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_collection_start(&mut self, cx: &mut Context<Self>) {
        self.page = Page::Packets;
        self.packets.update(cx, |p, cx| p.proof_collection(cx));
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_collection_valid(&self, cx: &App) -> bool {
        self.packets.read(cx).proof_collection_valid(cx)
            && !self
                .capture_active
                .load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_saved_theme(&self) -> Option<String> {
        AppSettings::load(self.profile.join("appearance.json"))
            .unwrap()
            .value
            .theme
    }
    pub fn new(
        profile: PathBuf,
        is_demo: bool,
        default_profile: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let policy_file = PolicyFile::new(profile.join("netlane-rules.json"));
        let (policies, revision, mut error) = if is_demo {
            (demo::policies(), None, None)
        } else {
            match policy_file.load() {
                Ok(s) => (s.policies, s.revision, None),
                Err(e) => (vec![], None, Some(format!("Falha ao ler regras: {e:#}"))),
            }
        };
        let snapshot = if is_demo {
            demo::snapshot()
        } else {
            NetworkSnapshot::default()
        };
        let selected = if is_demo {
            snapshot.adapters.iter().map(|a| a.id.clone()).collect()
        } else {
            match read_bounded(&profile.join("interface-selection.json"), 65536) {
                Ok(Some(bytes)) => match serde_json::from_slice::<Vec<String>>(without_bom(&bytes))
                {
                    Ok(ids) => ids,
                    Err(e) => {
                        error = Some(format!("Não foi possível ler a seleção de interfaces: {e}"));
                        vec![]
                    }
                },
                Ok(None) => vec![],
                Err(e) => {
                    error = Some(format!(
                        "Não foi possível ler a seleção de interfaces: {e:#}"
                    ));
                    vec![]
                }
            }
        };
        let filter = cx.new(|cx| {
            TextInput::new("", "Pesquisar aplicativos, PID ou endereço…", cx).tab_stop(0)
        });
        let executable = cx.new(|cx| TextInput::new("", "Caminho completo do aplicativo .exe", cx));
        let (quality_preferences, quality_status) = if is_demo {
            (
                netlane_core::quality::Settings::default(),
                "Desativado · Iniciar usa amostras fictícias.".to_owned(),
            )
        } else {
            match netlane_core::quality::Settings::load(&profile.join("quality-settings.json")) {
                Ok(value) => (
                    value,
                    "Desativado · clique em Iniciar para medir.".to_owned(),
                ),
                Err(e) => (
                    netlane_core::quality::Settings::default(),
                    format!("Não foi possível ler o destino salvo: {e}. Escolha um destino."),
                ),
            }
        };
        let quality_input =
            cx.new(|cx| TextInput::new(&quality_preferences.target, "IPv4 ou site HTTPS", cx));
        let subscription = cx.subscribe(&filter, |ws, _, _, cx| {
            ws.connection_page = 0;
            cx.notify();
        });
        let add_subscription = cx.subscribe_in(&executable, window, |ws, _, event, _, cx| {
            if matches!(event, TextInputEvent::Confirmed) {
                ws.add_rule(cx);
            }
        });
        let controller = if is_demo {
            None
        } else {
            std::env::current_exe()
                .ok()
                .map(|p| netlane_agent::Controller::new(p.with_file_name("netlane-agent.exe")))
        };
        let packet_dialog = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let capture_active = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let codex = cx.new(|cx| CodexView::new(is_demo, profile.clone(), cx));
        let codex_subscription = cx.subscribe(&codex, |ws, _, _: &ContextPrepared, cx| {
            ws.page = Page::Codex;
            cx.notify();
        });
        let packets = cx.new(|cx| {
            PacketView::new(
                is_demo,
                packet_dialog.clone(),
                codex.clone(),
                profile.clone(),
                capture_active.clone(),
                cx,
            )
        });
        let mut ws = Self {
            page: Page::Overview,
            snapshot,
            policies,
            revision,
            policy_file,
            profile,
            dirty: false,
            error,
            notice: None,
            demo: is_demo,
            filter,
            executable,
            selected,
            traffic: TrafficTracker::default(),
            icons: ExecutableIcons::new(!is_demo),
            quality: netlane_core::quality::Monitor::default(),
            quality_preferences,
            quality_input,
            quality_status,
            quality_dns_busy: false,
            default_snapshot: None,
            default_journal: None,
            default_revision: None,
            default_selected: None,
            default_status: "Aguardando leitura da prioridade IPv4…".into(),
            default_valid: false,
            default_reading: false,
            default_busy: false,
            default_confirmation: None,
            #[cfg(feature = "gui-test")]
            proof_quality_details: false,
            started: Instant::now(),
            controller,
            allow_flags: false,
            theme_picker: None,
            theme_subscription: None,
            _subscriptions: vec![subscription, add_subscription, codex_subscription],
            focus: cx.focus_handle(),
            connection_page: 0,
            closing: false,
            waiting_for_stop: false,
            desktop: None,
            startup_registration: (!is_demo && default_profile)
                .then(netlane_windows::startup::StartupRegistration::current),
            startup_enabled: false,
            file_dialog_open: false,
            packets,
            codex,
            packet_dialog,
            capture_active,
            hide_pending: false,
            #[cfg(feature = "gui-test")]
            proof_session: None,
        };
        ws.refresh_startup();
        ws.initialize_default(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |ws, cx| ws.refresh_quality(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        if is_demo {
            for sample in 0..30 {
                let mut snapshot = demo::snapshot();
                for (i, a) in snapshot.adapters.iter_mut().enumerate() {
                    a.received = Some(
                        a.received.unwrap_or(0)
                            + sample * (if i == 0 { 2_200_000 } else { 4_600_000 }),
                    );
                    a.sent = Some(a.sent.unwrap_or(0) + sample * 480_000);
                }
                ws.traffic
                    .sample(Duration::from_secs(sample * 2), &snapshot.adapters);
            }
        } else {
            cx.spawn_in(window, async move |this, cx| {
                loop {
                    let snapshot = cx
                        .background_executor()
                        .spawn(async { netlane_windows::monitor::snapshot() })
                        .await;
                    if this
                        .update(cx, |ws, cx| {
                            match snapshot {
                                Ok(snapshot) => {
                                    if ws.selected.is_empty()
                                        && !ws.profile.join("interface-selection.json").exists()
                                    {
                                        ws.selected = snapshot
                                            .adapters
                                            .iter()
                                            .filter(|a| a.connected && a.has_gateway)
                                            .map(|a| a.id.clone())
                                            .collect();
                                    }
                                    ws.traffic.sample(ws.started.elapsed(), &snapshot.adapters);
                                    ws.snapshot = snapshot;
                                    ws.sync_quality();
                                }
                                Err(e) => {
                                    ws.snapshot = NetworkSnapshot {
                                        error: Some(format!("Coleta indisponível: {e:#}")),
                                        ..Default::default()
                                    };
                                    ws.traffic.reset();
                                    ws.sync_quality();
                                }
                            }
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                }
            })
            .detach();
        }
        ws
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.demo {
            self.dirty = false;
            self.notice = Some("Prévia atualizada nesta demonstração.".to_owned());
            cx.notify();
            return;
        }
        match self
            .policy_file
            .save(&self.policies, self.revision.as_deref())
        {
            Ok(revision) => {
                self.revision = Some(revision);
                self.dirty = false;
                self.error = None;
                self.notice = Some(
                    "Regras salvas. A sessão ativa lê as mudanças no próximo ciclo.".to_owned(),
                );
            }
            Err(e) => self.error = Some(format!("Não foi possível salvar: {e:#}")),
        }
        cx.notify();
    }
    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.dirty {
            self.error = Some("Salve ou descarte as alterações antes de recarregar.".to_owned());
        } else if self.demo {
            self.policies = demo::policies();
        } else {
            match self.policy_file.load() {
                Ok(s) => {
                    self.policies = s.policies;
                    self.revision = s.revision;
                    self.error = None;
                }
                Err(e) => self.error = Some(format!("Não foi possível recarregar: {e:#}")),
            }
        }
        cx.notify();
    }
    fn discard(&mut self, cx: &mut Context<Self>) -> bool {
        let discarded = if self.demo {
            self.policies = demo::policies();
            self.dirty = false;
            true
        } else {
            match self.policy_file.load() {
                Ok(saved) => {
                    self.policies = saved.policies;
                    self.revision = saved.revision;
                    self.dirty = false;
                    self.error = None;
                    true
                }
                Err(e) => {
                    self.error = Some(format!(
                        "Não foi possível recarregar as regras salvas; suas edições foram mantidas: {e:#}"
                    ));
                    false
                }
            }
        };
        cx.notify();
        discarded
    }
    fn add_rule(&mut self, cx: &mut Context<Self>) {
        let path = self.executable.read(cx).text().trim().to_owned();
        if path.is_empty()
            || !path.to_lowercase().ends_with(".exe")
            || (!self.demo && !std::path::Path::new(&path).is_file())
        {
            self.error = Some("Selecione um executável .exe existente.".to_owned());
        } else {
            let policy = RoutingPolicy::new(&path);
            if self.policies.iter().any(|p| {
                p.application_id
                    .eq_ignore_ascii_case(&policy.application_id)
            }) {
                self.error = Some("Este aplicativo já possui uma regra.".to_owned());
            } else {
                self.policies.push(policy);
                self.dirty = true;
                self.error = None;
                self.executable
                    .update(cx, |input, cx| input.set_text("", cx));
            }
        }
        cx.notify();
    }
    fn browse_application(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_dialog_open {
            return;
        }
        self.file_dialog_open = true;
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Adicionar aplicativo".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update(cx, |ws, cx| {
                ws.file_dialog_open = false;
                cx.notify();
            });
            if let Ok(Ok(Some(paths))) = result
                && let Some(path) = paths.first()
            {
                let path = path.to_string_lossy().into_owned();
                let _ = this.update(cx, |ws, cx| {
                    ws.executable
                        .update(cx, |input, cx| input.set_text(path, cx));
                    ws.add_rule(cx);
                });
            }
        })
        .detach();
    }
    fn import_rules(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.demo || self.dirty || !self.policies.is_empty() || self.file_dialog_open {
            self.error = Some("Importe em um perfil vazio, sem alterações pendentes.".to_owned());
            cx.notify();
            return;
        }
        self.file_dialog_open = true;
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Importar arquivo de regras JSON".into()),
        });
        cx.spawn_in(window, async move |this,cx| {
            let result = paths.await;
            let _ = this.update(cx, |ws, cx| { ws.file_dialog_open = false; cx.notify(); });
            if let Ok(Ok(Some(paths))) = result && let Some(path) = paths.first() {
                let imported = PolicyFile::new(path).load();
                let _ = this.update(cx, |ws,cx| {
                    if ws.dirty || !ws.policies.is_empty() {
                        ws.error = Some("O perfil mudou durante a escolha do arquivo.".to_owned());
                    } else {
                        match imported {
                            Ok(source) if source.revision.is_some() => {
                                ws.policies = source.policies;
                                ws.dirty = true;
                                ws.error = None;
                                ws.notice = Some("Regras importadas. Salve para criar a cópia neste perfil; o arquivo original foi preservado.".to_owned());
                            }
                            Ok(_) => ws.error = Some("O arquivo selecionado não existe.".to_owned()),
                            Err(e) => ws.error = Some(format!("Não foi possível importar: {e:#}")),
                        }
                    }
                    cx.notify();
                });
            }
        }).detach();
    }
    fn choose_rule_executable(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_dialog_open {
            return;
        }
        self.file_dialog_open = true;
        let application = self.policies[index].application_id.clone();
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Associar executável à regra".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = paths.await;
            let _ = this.update(cx, |ws, cx| {
                ws.file_dialog_open = false;
                cx.notify();
            });
            if let Ok(Ok(Some(paths))) = result
                && let Some(path) = paths.first()
            {
                let path = path.to_string_lossy().into_owned();
                let _ = this.update(cx, |ws, cx| {
                    if !path.to_lowercase().ends_with(".exe")
                        || !std::path::Path::new(&path).is_file()
                    {
                        ws.error = Some("Selecione um executável .exe existente.".to_owned());
                    } else if ws.policies.iter().any(|p| {
                        p.application_id != application
                            && p.executable_path
                                .as_deref()
                                .is_some_and(|s| s.eq_ignore_ascii_case(&path))
                    }) {
                        ws.error =
                            Some("Este executável já está associado a outra regra.".to_owned());
                    } else if let Some(policy) = ws
                        .policies
                        .iter_mut()
                        .find(|p| p.application_id == application)
                    {
                        policy.executable_path = Some(path);
                        ws.dirty = true;
                        ws.error = None;
                    } else {
                        ws.error = Some(
                            "A regra foi removida durante a escolha do executável.".to_owned(),
                        );
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }
    fn change_settings(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Settings)) {
        if let Err(e) = AppSettings::update(cx, change) {
            self.error = Some(format!("Não foi possível salvar a aparência: {e:#}"));
        }
        cx.notify();
    }
    fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.theme_picker.is_some() || self.default_busy || self.default_confirmation.is_some() {
            return;
        }
        let picker = cx.new(|cx| ThemePicker::new(window, cx));
        picker.focus_handle(cx).focus(window, cx);
        self.theme_subscription =
            Some(
                cx.subscribe_in(&picker, window, |ws, _, event, window, cx| {
                    if matches!(event, ThemePickerEvent::Closed) {
                        ws.theme_picker = None;
                        ws.theme_subscription = None;
                        ws.focus.focus(window, cx);
                    }
                    cx.notify();
                }),
            );
        self.theme_picker = Some(picker);
        cx.notify();
    }
    fn toggle_interface(&mut self, id: String, cx: &mut Context<Self>) {
        let mut selected = self.selected.clone();
        if selected.contains(&id) {
            selected.retain(|v| v != &id);
        } else {
            selected.push(id);
        }
        selected.sort();
        selected.dedup();
        if !self.demo {
            match serde_json::to_vec_pretty(&selected)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| {
                    atomic_write(&self.profile.join("interface-selection.json"), &bytes, true)
                }) {
                Ok(()) => {}
                Err(e) => {
                    self.error = Some(format!("Não foi possível salvar interfaces: {e:#}"));
                    cx.notify();
                    return;
                }
            }
        }
        self.selected = selected;
        self.sync_quality();
        cx.notify();
    }
    fn start_session(&mut self, cx: &mut Context<Self>) {
        if self.desktop_blocked() {
            return;
        }
        if self.dirty {
            self.error = Some("Salve as alterações antes de iniciar.".to_owned());
        } else if let Some(controller) = &self.controller
            && let Err(e) = controller.start(self.policy_file.path.clone(), self.allow_flags)
        {
            self.error = Some(format!("Não foi possível iniciar: {e:#}"));
        }
        cx.notify();
    }
    fn stop_session(&mut self, cx: &mut Context<Self>) {
        if self.waiting_for_stop
            || self.closing
            || self.session_state().busy
            || self.default_busy
            || self.default_confirmation.is_some()
        {
            return;
        }
        if let Some(controller) = &self.controller
            && let Err(e) = controller.stop()
        {
            self.error = Some(format!("Não foi possível parar: {e:#}"));
        }
        cx.notify();
    }
    pub fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restore_panel();
        if self
            .capture_active
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            self.error = Some("Pare e feche a coleta de pacotes antes de sair.".into());
            cx.notify();
            return;
        }
        if self.waiting_for_stop
            || self.closing
            || self.file_dialog_open
            || self
                .packet_dialog
                .load(std::sync::atomic::Ordering::Relaxed)
            || self.theme_picker.is_some()
            || self.default_busy
            || self.default_confirmation.is_some()
        {
            return;
        }
        match exit_decision(self.dirty, &self.session_state()) {
            ExitDecision::Busy => return,
            ExitDecision::ConfirmEdits => {
                self.closing = true;
                cx.notify();
                return;
            }
            ExitDecision::CleanupUnconfirmed => {
                self.page = Page::Diagnostics;
                self.error = Some("A limpeza da sessão não foi confirmada. Consulte o diagnóstico; o painel continuará disponível.".to_owned());
                cx.notify();
                return;
            }
            ExitDecision::Quit => {
                self.quality.stop();
                cx.quit();
                return;
            }
            ExitDecision::StopAndWait => {}
        }
        {
            self.stop_session(cx);
            self.waiting_for_stop = true;
            self.notice = Some("Aguardando o encerramento seguro da sessão…".to_owned());
            cx.spawn_in(window, async move |this, cx| {
                let deadline = Instant::now() + Duration::from_secs(60);
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    let Ok(done) = this.update_in(cx, |ws, window, cx| {
                        let state = ws.session_state();
                        if !state.running && !state.busy {
                            ws.waiting_for_stop = false;
                            ws.notice = None;
                            // Same guard as the first request: edits made during Stop must
                            // be confirmed again, and an unconfirmed cleanup cannot be bypassed.
                            ws.request_close(window, cx);
                            return true;
                        }
                        if Instant::now() >= deadline {
                            ws.waiting_for_stop = false;
                            ws.notice = None;
                            ws.error = Some("O encerramento ainda não foi confirmado. Consulte o diagnóstico antes de sair.".to_owned());
                            cx.notify();
                            return true;
                        }
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
    }
    fn session_state(&self) -> netlane_agent::SessionState {
        #[cfg(feature = "gui-test")]
        if let Some(state) = &self.proof_session {
            return state.clone();
        }
        self.controller
            .as_ref()
            .map(|c| c.state())
            .unwrap_or_else(|| netlane_agent::SessionState {
                cleanup_confirmed: true,
                detail: "Demonstração com dados fictícios. Roteamento indisponível.".to_owned(),
                ..Default::default()
            })
    }
    fn desktop_blocked(&self) -> bool {
        self.closing
            || self.waiting_for_stop
            || self.file_dialog_open
            || self
                .packet_dialog
                .load(std::sync::atomic::Ordering::Relaxed)
            || self.theme_picker.is_some()
            || self.session_state().busy
            || self.default_busy
            || self.default_confirmation.is_some()
    }
    fn tray_state(&self) -> netlane_windows::desktop::TrayState {
        let state = self.session_state();
        let blocked = self.desktop_blocked();
        netlane_windows::desktop::TrayState {
            status: if state.busy || self.waiting_for_stop || self.default_busy {
                "Operação em andamento"
            } else if state.running {
                "Sessão ativa · consulte o diagnóstico"
            } else if !state.cleanup_confirmed {
                "Limpeza não confirmada"
            } else if self.demo {
                "Demonstração · sessão indisponível"
            } else {
                "Sessão parada"
            }
            .to_owned(),
            enabled_rules: self.policies.iter().filter(|p| p.enabled).count(),
            total_rules: self.policies.len(),
            dirty: self.dirty,
            can_start: !blocked
                && !self.demo
                && !self.dirty
                && !state.running
                && state.cleanup_confirmed,
            can_stop: !blocked && state.running,
            can_exit: !blocked,
        }
    }
    pub fn install_desktop(&mut self, startup: bool, window: &mut Window, cx: &mut Context<Self>) {
        match Desktop::new(window, self.tray_state()) {
            Ok(desktop) => {
                self.desktop = Some(desktop);
                if startup {
                    self.request_hide(window, cx);
                }
            }
            Err(e) => {
                self.error = Some(format!(
                    "Bandeja indisponível: {e:#}. O painel permanece na barra de tarefas."
                ));
                window.activate_window();
            }
        }
        let subscription = cx.observe_window_activation(window, |ws, window, cx| {
            if window.is_window_active() {
                ws.refresh_startup();
                cx.notify();
            }
        });
        self._subscriptions.push(subscription);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this
                    .update_in(cx, |ws, w, cx| ws.poll_desktop(w, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
    pub fn request_hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.desktop_blocked() {
            self.restore_panel();
            return;
        }
        if let Some(desktop) = &self.desktop
            && desktop.available()
        {
            let _ = desktop;
            self.hide_pending = true;
            let weak = cx.entity().downgrade();
            window.defer(cx, move |_, cx| {
                let _ = weak.update(cx, |ws, _| {
                    if ws.hide_pending && !ws.desktop_blocked() {
                        ws.hide_pending = false;
                        if let Some(desktop) = &ws.desktop
                            && desktop.available()
                        {
                            desktop.hide();
                        }
                    }
                });
            });
        } else {
            self.request_close(window, cx);
        }
    }
    fn restore_panel(&mut self) {
        self.hide_pending = false;
        if let Some(desktop) = &self.desktop {
            desktop.restore();
        }
    }
    fn poll_desktop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use netlane_windows::desktop::TrayAction;
        let state = self.tray_state();
        let actions = self
            .desktop
            .as_mut()
            .map(|d| d.poll(state))
            .unwrap_or_default();
        for action in actions {
            match action {
                TrayAction::Unavailable => {
                    self.error = Some("A bandeja ficou indisponível. O painel foi restaurado na barra de tarefas.".to_owned());
                    cx.notify();
                }
                TrayAction::Open => self.restore_panel(),
                TrayAction::Diagnostics => {
                    if !self.closing && !self.file_dialog_open {
                        self.restore_panel();
                        self.page = Page::Diagnostics;
                        cx.notify();
                    }
                }
                TrayAction::Start => {
                    if self.tray_state().can_start {
                        self.restore_panel();
                        self.page = Page::Diagnostics;
                        self.notice = Some("Revise as regras e a autorização temporária; use Iniciar sessão para solicitar UAC.".to_owned());
                        cx.notify();
                    }
                }
                TrayAction::Stop => {
                    if self.tray_state().can_stop {
                        self.restore_panel();
                        self.page = Page::Diagnostics;
                        self.stop_session(cx);
                    }
                }
                TrayAction::Exit => {
                    if self.tray_state().can_exit {
                        self.request_close(window, cx);
                    }
                }
            }
        }
        if self
            .desktop
            .as_ref()
            .is_some_and(|d| d.available() && d.minimized())
        {
            if self.desktop_blocked() {
                self.restore_panel();
            } else {
                self.request_hide(window, cx);
            }
        }
    }
    fn refresh_startup(&mut self) {
        match self
            .startup_registration
            .as_ref()
            .map(|r| r.enabled())
            .transpose()
        {
            Ok(enabled) => self.startup_enabled = enabled.unwrap_or(false),
            Err(e) => {
                self.startup_enabled = false;
                self.error = Some(format!("Inicialização: {e:#}"));
            }
        }
    }
    fn toggle_startup(&mut self, cx: &mut Context<Self>) {
        if let Some(registration) = &self.startup_registration
            && registration.available()
        {
            match registration.set_enabled(!self.startup_enabled) {
                Ok(()) => {
                    self.refresh_startup();
                    self.notice =
                        Some("Preferência de inicialização confirmada pelo Windows.".to_owned());
                }
                Err(e) => {
                    self.error = Some(format!("Não foi possível alterar a inicialização: {e:#}"))
                }
            }
        }
        cx.notify();
    }
    fn button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: &str,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let t = cx.theme();
        div()
            .id(id)
            .px(px(12.))
            .py(px(6.))
            .rounded(px(4.))
            .border_1()
            .border_color(t.border)
            .bg(t.elevated)
            .text_color(t.text)
            .cursor_pointer()
            .hover(|s| s.bg(t.hover))
            .child(label.to_owned())
    }
    fn heading(&self, title: &str, subtitle: &str, cx: &App) -> gpui::Div {
        let t = cx.theme();
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .mb(px(22.))
            .child(
                div()
                    .text_size(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text_strong)
                    .child(title.to_owned()),
            )
            .child(div().text_color(t.text_muted).child(subtitle.to_owned()))
    }
    fn overview(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let settings = AppSettings::get(cx).clone();
        let mut cards = div().flex().flex_wrap().gap(px(16.));
        for a in self
            .snapshot
            .adapters
            .iter()
            .filter(|a| self.selected.contains(&a.id))
        {
            let usage = self.traffic.usage.get(&a.id).cloned().unwrap_or_default();
            let id = a.id.clone();
            let speed = |s: Option<f64>| s.map_or("—".to_owned(), |s| format!("{s:.2} Mbps"));
            let mut history = div().h(px(58.)).flex().items_end().gap(px(3.));
            let maximum = usage
                .history
                .iter()
                .filter_map(|(rx, _)| *rx)
                .fold(1.0, f64::max);
            for (i, (rx, _)) in usage.history.iter().enumerate() {
                history = history.child(
                    div()
                        .id(gpui::SharedString::from(format!("sample-{}-{i}", a.id)))
                        .w(px(6.))
                        .h(px(rx.unwrap_or(0.) as f32 / maximum as f32 * 50. + 2.))
                        .bg(t.accent.opacity(0.8))
                        .rounded(px(2.)),
                );
            }
            cards = cards.child(
                div()
                    .w(gpui::relative(0.48))
                    .min_w(px(300.))
                    .p(px(if settings.compact { 14. } else { 20. }))
                    .bg(t.panel)
                    .rounded(px(8.))
                    .border_1()
                    .border_color(t.border)
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(self.quality_indicator(a, cx))
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(16.))
                                    .child(a.name.clone()),
                            )
                            .child(
                                div()
                                    .text_color(if a.connected { t.ok } else { t.warning })
                                    .child(if a.connected {
                                        "Conectada"
                                    } else {
                                        "Desconectada"
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_color(t.text_muted)
                            .font_family(MONO_FONT)
                            .text_size(px(12.))
                            .child(if a.addresses.is_empty() {
                                "Sem endereço".to_owned()
                            } else {
                                a.addresses.join(" · ")
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.))
                                    .child(div().text_color(t.text_muted).child("Download"))
                                    .child(
                                        div()
                                            .text_size(px(21.))
                                            .text_color(t.text_strong)
                                            .child(speed(usage.download_mbps)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.))
                                    .child(div().text_color(t.text_muted).child("Upload"))
                                    .child(
                                        div()
                                            .text_size(px(21.))
                                            .text_color(t.text_strong)
                                            .child(speed(usage.upload_mbps)),
                                    ),
                            ),
                    )
                    .child(history)
                    .child(
                        div()
                            .text_color(t.text_muted)
                            .text_size(px(12.))
                            .child(format!(
                                "Recebido: {:.1} MB  ·  Enviado: {:.1} MB",
                                usage.received as f64 / 1_000_000.,
                                usage.sent as f64 / 1_000_000.
                            )),
                    )
                    .child(
                        self.button(
                            gpui::SharedString::from(format!("deselect-{id}")),
                            "Ocultar do painel",
                            cx,
                        )
                        .on_click(
                            cx.listener(move |ws, _, _, cx| ws.toggle_interface(id.clone(), cx)),
                        ),
                    ),
            );
        }
        if self.selected.is_empty() {
            cards = cards.child(
                div()
                    .p(px(24.))
                    .text_color(t.text_muted)
                    .child("Selecione as interfaces que deseja acompanhar."),
            );
        }
        let mut interfaces = div().flex().flex_wrap().gap(px(8.));
        for a in &self.snapshot.adapters {
            let id = a.id.clone();
            let selected = self.selected.contains(&id);
            interfaces = interfaces.child(
                self.button(
                    gpui::SharedString::from(format!("interface-{id}")),
                    &format!("{} {}", if selected { "✓" } else { "+" }, a.name),
                    cx,
                )
                .on_click(cx.listener(move |ws, _, _, cx| ws.toggle_interface(id.clone(), cx))),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(self.heading(
                "Suas conexões, em um só lugar",
                "Acompanhe o consumo por interface e escolha a saída dos aplicativos.",
                cx,
            ))
            .child(self.default_panel(cx))
            .child(self.quality_panel(cx))
            .child(cards)
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Interfaces no painel"),
            )
            .child(interfaces)
            .child(
                self.button("reset-measurement", "Reiniciar medição", cx)
                    .on_click(cx.listener(|ws, _, _, cx| {
                        ws.traffic.reset();
                        cx.notify();
                    })),
            )
            .child(div().text_color(t.text_muted).child(
                "Os totais contam apenas esta medição e incluem o tráfego de todos os aplicativos.",
            ))
    }
    fn connection_rows(&self, cx: &App) -> Vec<&netlane_core::Connection> {
        let query = self.filter.read(cx).text().to_lowercase();
        self.snapshot
            .connections
            .iter()
            .filter(|c| {
                format!(
                    "{} {} {} {} {}",
                    c.name,
                    c.pid,
                    c.local_address,
                    c.remote_address,
                    c.interface.as_deref().unwrap_or("")
                )
                .to_lowercase()
                .contains(&query)
            })
            .collect()
    }
    fn application_icon(&self, path: Option<&str>, color: gpui::Hsla) -> gpui::AnyElement {
        if let Some(image) = self.icons.image(path) {
            gpui::img(image)
                .size(px(20.))
                .flex_none()
                .into_any_element()
        } else {
            icon("desktop_windows", 20., color).into_any_element()
        }
    }
    fn refresh_icons(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = match self.page {
            Page::Rules => self
                .policies
                .iter()
                .filter_map(|p| p.executable_path.clone())
                .collect(),
            Page::Connections => {
                let rows = self.connection_rows(cx);
                let page = self.connection_page.min(rows.len().saturating_sub(1) / 50);
                rows.into_iter()
                    .skip(page * 50)
                    .take(50)
                    .filter_map(|c| c.executable.clone())
                    .collect()
            }
            _ => vec![],
        };
        let (requests, dropped) = self.icons.prepare(paths, Instant::now());
        for image in dropped {
            cx.drop_image(image, Some(window));
        }
        for request in requests {
            cx.spawn_in(window, async move |this, cx| {
                let pixels = cx
                    .background_executor()
                    .spawn(async move { request.provider.get(&request.path) })
                    .await;
                let _ = this.update_in(cx, |ws, w, cx| {
                    if let Some(old) =
                        ws.icons
                            .complete(&request.key, request.ticket, pixels, Instant::now())
                    {
                        cx.drop_image(old, Some(w));
                    }
                    cx.notify();
                });
            })
            .detach();
        }
    }
    fn connections(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let rows = self.connection_rows(cx);
        let page = self.connection_page.min(rows.len().saturating_sub(1) / 50);
        let mut list = div().w_full().min_w_0().flex().flex_col();
        list = list.child(
            div()
                .flex()
                .py(px(10.))
                .px(px(12.))
                .bg(t.panel)
                .text_color(t.text_muted)
                .child(
                    div()
                        .w(gpui::relative(0.22))
                        .flex_none()
                        .child("Aplicativo / PID"),
                )
                .child(
                    div()
                        .w(gpui::relative(0.18))
                        .flex_none()
                        .child("Regra configurada"),
                )
                .child(
                    div()
                        .w(gpui::relative(0.35))
                        .flex_none()
                        .child("Saída observada · IP local"),
                )
                .child(
                    div()
                        .w(gpui::relative(0.25))
                        .flex_none()
                        .child("Destino TCP/IPv4"),
                ),
        );
        for (i, c) in rows.iter().skip(page * 50).take(50).enumerate() {
            let rule = self.policies.iter().find(|p| {
                c.executable
                    .as_deref()
                    .zip(p.executable_path.as_deref())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            });
            list = list.child(
                div()
                    .id(("connection", i))
                    .flex()
                    .items_center()
                    .px(px(12.))
                    .py(px(if AppSettings::get(cx).compact {
                        7.
                    } else {
                        12.
                    }))
                    .border_b_1()
                    .border_color(t.divider)
                    .hover(|s| s.bg(t.hover))
                    .child(
                        div()
                            .w(gpui::relative(0.22))
                            .flex_none()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(self.application_icon(c.executable.as_deref(), t.text_muted))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .child(c.name.clone())
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(t.text_muted)
                                            .child(format!("PID {}", c.pid)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .w(gpui::relative(0.18))
                            .flex_none()
                            .min_w_0()
                            .text_color(t.text_muted)
                            .child(
                                rule.filter(|p| p.enabled)
                                    .map_or("Automático", |p| p.route_mode.label()),
                            ),
                    )
                    .child(
                        div()
                            .w(gpui::relative(0.35))
                            .flex_none()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                c.interface
                                    .clone()
                                    .unwrap_or_else(|| "Interface não identificada".to_owned()),
                            )
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(12.))
                                    .text_color(t.text_muted)
                                    .child(format!("{}:{}", c.local_address, c.local_port)),
                            ),
                    )
                    .child(
                        div()
                            .w(gpui::relative(0.25))
                            .flex_none()
                            .min_w_0()
                            .font_family(MONO_FONT)
                            .text_size(px(12.))
                            .text_color(t.text_muted)
                            .child(format!("{}:{}", c.remote_address, c.remote_port)),
                    ),
            );
        }
        if rows.is_empty() {
            list = list.child(
                div()
                    .p(px(28.))
                    .text_color(t.text_muted)
                    .child("Nenhuma conexão TCP/IPv4 estabelecida nesta leitura."),
            );
        }
        div().flex().flex_col().gap(px(16.)).child(self.heading("Conexões dos aplicativos","Compare a regra escolhida com a saída observada nas conexões atuais.",cx)).child(self.filter.clone())
            .child(list)
            .child(div().flex().gap(px(12.)).items_center().child(self.button("previous","Anterior",cx).on_click(cx.listener(move|ws,_,_,cx|{ws.connection_page=page.saturating_sub(1);cx.notify();})))
                .child(format!("{} conexões · página {}",rows.len(),page+1)).child(self.button("next","Próxima",cx).on_click(cx.listener(move|ws,_,_,cx|{let query=ws.filter.read(cx).text().to_lowercase();let count=ws.snapshot.connections.iter().filter(|c|format!("{} {} {} {} {}",c.name,c.pid,c.local_address,c.remote_address,c.interface.as_deref().unwrap_or("")).to_lowercase().contains(&query)).count();if (page+1)*50<count{ws.connection_page=page+1;}cx.notify();}))))
            .child(div().text_color(t.text_muted).child("A saída observada não comprova que a política causou o roteamento. Conexões antigas podem manter a saída anterior."))
    }
    fn rules(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let mut rows = div().flex().flex_col().gap(px(12.));
        let query = self.filter.read(cx).text().to_lowercase();
        for (i, p) in self.policies.iter().enumerate().filter(|(_, p)| {
            format!(
                "{} {}",
                p.application_id,
                p.executable_path.as_deref().unwrap_or("")
            )
            .to_lowercase()
            .contains(&query)
        }) {
            let mut modes = div().flex().gap(px(6.));
            for mode in RouteMode::ALL {
                modes = modes.child(
                    self.button(
                        gpui::SharedString::from(format!("mode-{i}-{mode:?}")),
                        mode.label(),
                        cx,
                    )
                    .when(p.route_mode == mode, |d| {
                        d.bg(t.selected).border_color(t.accent)
                    })
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        ws.policies[i].route_mode = mode;
                        ws.policies[i].interface_id = None;
                        ws.dirty = true;
                        cx.notify();
                    })),
                );
            }
            let mut targets = div().flex().flex_wrap().gap(px(6.));
            for a in self
                .snapshot
                .adapters
                .iter()
                .filter(|a| a.mode() == Some(p.route_mode))
            {
                let id = a.id.clone();
                targets = targets.child(
                    self.button(
                        gpui::SharedString::from(format!("target-{i}-{id}")),
                        &a.name,
                        cx,
                    )
                    .when(
                        p.interface_id
                            .as_deref()
                            .is_some_and(|s| netlane_core::same_interface(s, &id)),
                        |d| d.border_color(t.accent),
                    )
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        ws.policies[i].interface_id = Some(id.clone());
                        ws.dirty = true;
                        cx.notify();
                    })),
                );
            }
            if let Some(id) = &p.interface_id
                && !self
                    .snapshot
                    .adapters
                    .iter()
                    .any(|a| netlane_core::same_interface(&a.id, id))
            {
                targets = targets.child(
                    div()
                        .text_color(t.warning)
                        .child("Interface salva indisponível. A escolha será preservada."),
                );
            }
            rows = rows.child(
                div()
                    .p(px(16.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.panel)
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                self.application_icon(p.executable_path.as_deref(), t.text_muted),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(p.application_id.clone()),
                            )
                            .child(
                                self.button(
                                    ("enabled", i),
                                    if p.enabled { "Ativada" } else { "Desativada" },
                                    cx,
                                )
                                .on_click(cx.listener(
                                    move |ws, _, _, cx| {
                                        ws.policies[i].enabled = !ws.policies[i].enabled;
                                        ws.dirty = true;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(self.button(("delete", i), "Remover", cx).on_click(
                                cx.listener(move |ws, _, _, cx| {
                                    ws.policies.remove(i);
                                    ws.dirty = true;
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(MONO_FONT)
                                    .text_size(px(11.))
                                    .text_color(t.text_muted)
                                    .child(p.executable_path.clone().unwrap_or_else(|| {
                                        "Caminho do executável ausente".to_owned()
                                    })),
                            )
                            .child(
                                self.button(("choose-rule-exe", i), "Escolher .exe", cx)
                                    .on_click(cx.listener(move |ws, _, w, cx| {
                                        ws.choose_rule_executable(i, w, cx)
                                    })),
                            ),
                    )
                    .child(modes)
                    .child(targets)
                    .child(
                        self.button(
                            ("related", i),
                            if p.include_related_executables {
                                "✓ Incluir auxiliares conhecidos"
                            } else {
                                "Incluir auxiliares conhecidos"
                            },
                            cx,
                        )
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            ws.policies[i].include_related_executables =
                                !ws.policies[i].include_related_executables;
                            ws.dirty = true;
                            cx.notify();
                        })),
                    ),
            );
        }
        if self.policies.is_empty() {
            rows = rows.child(
                div()
                    .p(px(30.))
                    .text_color(t.text_muted)
                    .child("Adicione um aplicativo para escolher sua saída de rede."),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(self.heading(
                "Cada aplicativo, sua rota",
                "Edite as regras e salve para que sejam lidas pela próxima sessão.",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(div().flex_1().child(self.executable.clone()))
                    .child(
                        self.button("browse", "Escolher .exe", cx)
                            .on_click(cx.listener(|ws, _, w, cx| ws.browse_application(w, cx))),
                    )
                    .child(
                        self.button("add", "Adicionar", cx)
                            .on_click(cx.listener(|ws, _, _, cx| ws.add_rule(cx))),
                    ),
            )
            .child(self.filter.clone())
            .child(rows)
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        self.button("save", "Salvar alterações", cx)
                            .bg(t.accent)
                            .text_color(t.text_on_accent)
                            .on_click(cx.listener(|ws, _, _, cx| ws.save(cx))),
                    )
                    .child(
                        self.button("reload", "Recarregar", cx)
                            .on_click(cx.listener(|ws, _, _, cx| ws.reload(cx))),
                    )
                    .when(!self.demo && !self.dirty && self.policies.is_empty(), |d| {
                        d.child(
                            self.button("import", "Importar regras JSON", cx)
                                .on_click(cx.listener(|ws, _, w, cx| ws.import_rules(w, cx))),
                        )
                    })
                    .when(self.dirty, |d| {
                        d.child(self.button("discard", "Descartar alterações", cx).on_click(
                            cx.listener(|ws, _, _, cx| {
                                ws.discard(cx);
                            }),
                        ))
                    }),
            )
    }
    fn diagnostics(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let state = self
            .controller
            .as_ref()
            .map(|c| c.state())
            .unwrap_or_else(|| netlane_agent::SessionState {
                detail: "Demonstração com dados fictícios. Roteamento indisponível.".to_owned(),
                ..Default::default()
            });
        let mut details = div().flex().flex_col().gap(px(8.));
        for rule in &state.rules {
            details = details.child(
                div()
                    .p(px(10.))
                    .bg(t.panel)
                    .child(format!("{} · {}", rule.application, rule.detail)),
            );
        }
        div().flex().flex_col().gap(px(18.)).child(self.heading("Controle da sessão","O painel observa a rede sem elevação. O agente solicita UAC ao iniciar.",cx))
            .child(div().p(px(20.)).bg(t.panel).border_1().border_color(t.border).rounded(px(6.)).flex().flex_col().gap(px(14.)).child(div().font_weight(FontWeight::SEMIBOLD).child(state.detail))
                .child(self.button("flags",if self.allow_flags{"✓ Autorizar routepolicies temporário (IPv4/IPv6)"}else{"Autorizar routepolicies temporário (IPv4/IPv6)"},cx).when(state.running||state.busy,|d|d.text_color(t.text_disabled)).on_click(cx.listener(|ws,_,_,cx|{if ws.controller.as_ref().is_none_or(|c|!c.state().running&&!c.state().busy){ws.allow_flags = !ws.allow_flags;}cx.notify();})))
                .child(div().flex().gap(px(8.)).when(!self.demo&&!state.running&&!state.busy&&state.cleanup_confirmed,|d|d.child(self.button("start","Iniciar sessão",cx).on_click(cx.listener(|ws,_,_,cx|ws.start_session(cx)))))
                    .when(!self.demo&&state.running,|d|d.child(self.button("stop","Parar com segurança",cx).on_click(cx.listener(|ws,_,_,cx|ws.stop_session(cx)))))))
            .child(details).child(div().text_color(t.text_muted).child(format!("Arquivo de regras: {}",self.policy_file.path.display())))
            .child(div().text_color(t.text_muted).child("Uma política aceita pelo Windows ainda precisa de prova de tráfego. A sessão encerra e restaura suas opções temporárias ao sair do painel."))
    }
    fn settings(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let settings = AppSettings::get(cx).clone();
        let count = cx.global::<Themes>().all.len();
        div().flex().flex_col().gap(px(20.)).child(self.heading("Deixe o NetLane do seu jeito","Temas, legibilidade e densidade da interface.",cx))
            .child(div().p(px(20.)).bg(t.panel).rounded(px(6.)).border_1().border_color(t.border).flex().flex_col().gap(px(16.))
                .child(div().flex().items_center().gap(px(10.)).child(icon("palette",20.,t.accent)).child(div().flex_1().flex().flex_col().gap(px(4.)).child("Tema da interface").child(div().text_color(t.text_muted).child(format!("{} · {count} temas claros e escuros",t.name))))
                    .child(self.button("choose-theme","Selecionar tema",cx).border_color(t.focus).on_click(cx.listener(|ws,_,w,cx|ws.open_theme_picker(w,cx)))))
                .child(div().text_color(t.text_muted).child("Pesquise um tema e use ↑/↓ para a prévia. Enter confirma; Esc restaura o tema anterior.")))
            .child(div().p(px(20.)).bg(t.panel).rounded(px(6.)).border_1().border_color(t.border).flex().flex_col().gap(px(16.))
                .child(div().flex().items_center().gap(px(12.)).child(div().flex_1().child("Tamanho do texto"))
                    .child(self.button("font-smaller","−",cx).on_click(cx.listener(|ws,_,_,cx|ws.change_settings(cx,|s|s.font_size=s.font_size.saturating_sub(1)))))
                    .child(format!("{} px",settings.font_size)).child(self.button("font-larger","+",cx).on_click(cx.listener(|ws,_,_,cx|ws.change_settings(cx,|s|s.font_size=s.font_size.saturating_add(1))))))
                .child(div().flex().items_center().gap(px(12.)).child(div().flex_1().child("Densidade das listas"))
                    .child(self.button("density",if settings.compact{"Compacta"}else{"Confortável"},cx).on_click(cx.listener(|ws,_,_,cx|ws.change_settings(cx,|s|s.compact = !s.compact))))))
            .child(div().p(px(20.)).bg(t.panel).rounded(px(6.)).border_1().border_color(t.border).flex().flex_col().gap(px(8.))
                .child(gpui::img(if t.appearance == crate::theme::Appearance::Light { "brand/logo-light.png" } else { "brand/logo-dark.png" }).w(px(360.)).h(px(112.)))
                .child(div().font_weight(FontWeight::SEMIBOLD).child("NetLane · 0.4.0 Rust"))
                .child(div().text_color(t.text_muted).child(format!("Interface GPUI · aparência {:?}. Preferências salvas neste perfil.",t.appearance)))
                .child(div().font_family(MONO_FONT).text_size(px(11.)).text_color(t.text_muted).child(self.profile.display().to_string())))
            .child(div().p(px(20.)).bg(t.panel).rounded(px(6.)).border_1().border_color(t.border).flex().flex_col().gap(px(12.))
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Bandeja e inicialização"))
                .child(div().text_color(t.text_muted).child("X e Minimizar mantêm o painel na bandeja, com suas edições. Use Sair para encerrar com segurança."))
                .child(self.button("startup",if self.startup_enabled { "✓ Abrir com o Windows" } else { "Abrir com o Windows" },cx)
                    .when(self.startup_registration.as_ref().is_none_or(|r|!r.available()),|d|d.text_color(t.text_disabled).cursor_default())
                    .on_click(cx.listener(|ws,_,_,cx|ws.toggle_startup(cx))))
                .child(div().text_color(t.text_muted).child(if self.startup_registration.as_ref().is_some_and(|r|r.available()) {
                    "Ao entrar no Windows, abre na bandeja em observação. Iniciar roteamento continua sendo uma ação manual."
                } else { "A inicialização estará disponível no pacote Rust instalado. Demonstrações, builds de desenvolvimento e perfis alternativos não registram inicialização." })))
    }
}
impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_icons(window, cx);
        let t = cx.theme().clone();
        let settings = AppSettings::get(cx).clone();
        let mut nav = div().flex().flex_col().gap(px(4.)).p(px(10.));
        for page in [
            Page::Overview,
            Page::Packets,
            Page::Codex,
            Page::Connections,
            Page::Rules,
            Page::Diagnostics,
            Page::Settings,
        ] {
            nav = nav.child(
                div()
                    .id(("nav", page as usize))
                    .h(px(36.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .when(self.page == page, |d| {
                        d.bg(t.selected).text_color(t.text_strong)
                    })
                    .hover(|s| s.bg(t.hover))
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        ws.page = page;
                        ws.connection_page = 0;
                        cx.notify();
                    }))
                    .child(icon(
                        page.icon(),
                        18.,
                        if self.page == page {
                            t.accent
                        } else {
                            t.text_muted
                        },
                    ))
                    .child(page.title()),
            );
        }
        let content = match self.page {
            Page::Overview => self.overview(cx),
            Page::Packets => div().child(self.packets.clone()),
            Page::Codex => div().child(self.codex.clone()),
            Page::Connections => self.connections(cx),
            Page::Rules => self.rules(cx),
            Page::Diagnostics => self.diagnostics(cx),
            Page::Settings => self.settings(cx),
        };
        let status = if self.demo {
            "Demonstração · dados fictícios".to_owned()
        } else if let Some(controller) = &self.controller {
            controller.state().detail
        } else {
            "Observação de rede".to_owned()
        };
        let mut root = div()
            .id("workspace")
            .key_context(
                if self.theme_picker.is_some()
                    || self.closing
                    || self.default_confirmation.is_some()
                {
                    "NetLaneOverlay"
                } else {
                    "NetLane"
                },
            )
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .font_family(UI_FONT)
            .text_size(px(settings.font_size as f32))
            .bg(t.editor)
            .text_color(t.text)
            .on_action(cx.listener(|ws, _: &SelectTheme, w, cx| ws.open_theme_picker(w, cx)))
            .on_action(cx.listener(|ws, _: &SaveRules, _, cx| ws.save(cx)))
            .child(
                div()
                    .h(px(36.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px(px(14.))
                    .gap(px(10.))
                    .bg(t.chrome)
                    .border_b_1()
                    .border_color(t.divider)
                    .child(gpui::img("brand/icon-app.png").size(px(24.)))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("NetLane"))
                    .child(
                        div()
                            .flex_1()
                            .text_color(t.text_muted)
                            .text_size(px(11.))
                            .child("Diagnóstico de rede"),
                    )
                    .child(
                        self.button("title-theme", &t.name, cx)
                            .on_click(cx.listener(|ws, _, w, cx| ws.open_theme_picker(w, cx))),
                    )
                    .child(
                        self.button("exit", "Sair", cx)
                            .on_click(cx.listener(|ws, _, w, cx| ws.request_close(w, cx))),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .w(px(settings.sidebar_width as f32))
                            .flex_none()
                            .bg(t.panel)
                            .border_r_1()
                            .border_color(t.divider)
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .p(px(20.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(5.))
                                    .child(
                                        div()
                                            .text_size(px(18.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("NetLane"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(t.text_muted)
                                            .child("Ferramentas de rede"),
                                    ),
                            )
                            .child(nav)
                            .child(div().flex_1())
                            .child(
                                div()
                                    .p(px(14.))
                                    .text_size(px(11.))
                                    .text_color(t.text_muted)
                                    .child("Ctrl+Shift+T · Temas\nCtrl+S · Salvar regras"),
                            ),
                    )
                    .child(
                        div()
                            .id("page-scroll")
                            .flex_1()
                            .min_w_0()
                            .overflow_y_scroll()
                            .p(px(28.))
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .when_some(self.error.clone(), |d, error| {
                                d.child(
                                    div()
                                        .p(px(12.))
                                        .rounded(px(4.))
                                        .bg(t.error.opacity(0.12))
                                        .text_color(t.error)
                                        .child(error),
                                )
                            })
                            .when_some(self.snapshot.error.clone(), |d, error| {
                                d.child(div().p(px(12.)).text_color(t.warning).child(error))
                            })
                            .when_some(self.notice.clone(), |d, notice| {
                                d.child(div().p(px(12.)).text_color(t.ok).child(notice))
                            })
                            .child(content),
                    ),
            )
            .child(
                div()
                    .h(px(28.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px(px(12.))
                    .gap(px(12.))
                    .bg(t.chrome)
                    .border_t_1()
                    .border_color(t.divider)
                    .text_size(px(11.))
                    .text_color(t.text_muted)
                    .child(div().flex_1().overflow_hidden().child(status))
                    .child(if self.dirty {
                        "Alterações não salvas"
                    } else {
                        "Regras salvas"
                    })
                    .child("Rust / GPUI"),
            );
        if self.default_confirmation.is_some() {
            root = root.child(gpui::deferred(self.default_overlay(cx)));
        }
        if let Some(picker) = &self.theme_picker {
            root = root.child(gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_start()
                    .justify_center()
                    .pt(px(90.))
                    .font_family(UI_FONT)
                    .text_size(px(13.))
                    .bg(t.chrome.opacity(0.65))
                    .occlude()
                    .child(picker.clone()),
            ));
        }
        if self.closing {
            root = root.child(gpui::deferred(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(t.chrome.opacity(0.7))
                    .occlude()
                    .child(
                        div()
                            .w(px(450.))
                            .p(px(24.))
                            .bg(t.elevated)
                            .border_1()
                            .border_color(t.border)
                            .rounded(px(6.))
                            .flex()
                            .flex_col()
                            .gap(px(18.))
                            .child("Há alterações nas regras. Salve ou descarte antes de sair.")
                            .child(self.button("close-save", "Salvar e sair", cx).on_click(
                                cx.listener(|ws, _, w, cx| {
                                    ws.save(cx);
                                    if !ws.dirty {
                                        ws.closing = false;
                                        ws.request_close(w, cx);
                                    }
                                    cx.notify();
                                }),
                            ))
                            .child(
                                self.button("close-discard", "Descartar e sair", cx)
                                    .on_click(cx.listener(|ws, _, w, cx| {
                                        if ws.discard(cx) {
                                            ws.closing = false;
                                            ws.request_close(w, cx);
                                        }
                                    })),
                            )
                            .child(
                                self.button("close-cancel", "Continuar editando", cx)
                                    .on_click(cx.listener(|ws, _, _, cx| {
                                        ws.closing = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            ));
        }
        let _ = window;
        root
    }
}
