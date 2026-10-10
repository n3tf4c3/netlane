use crate::{
    demo,
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
    Connections,
    Rules,
    Diagnostics,
    Settings,
}
impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Visão geral",
            Self::Connections => "Conexões",
            Self::Rules => "Regras",
            Self::Diagnostics => "Diagnóstico",
            Self::Settings => "Configurações",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Overview => "lan",
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
}
impl Workspace {
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
    pub fn proof_saved_theme(&self) -> Option<String> {
        AppSettings::load(self.profile.join("appearance.json"))
            .unwrap()
            .value
            .theme
    }
    pub fn new(
        profile: PathBuf,
        is_demo: bool,
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
            started: Instant::now(),
            controller,
            allow_flags: false,
            theme_picker: None,
            theme_subscription: None,
            _subscriptions: vec![subscription, add_subscription],
            focus: cx.focus_handle(),
            connection_page: 0,
            closing: false,
            waiting_for_stop: false,
        };
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
                                }
                                Err(e) => {
                                    ws.snapshot = NetworkSnapshot {
                                        error: Some(format!("Coleta indisponível: {e:#}")),
                                        ..Default::default()
                                    };
                                    ws.traffic.reset();
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
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Adicionar aplicativo".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
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
        if self.demo || self.dirty || !self.policies.is_empty() {
            self.error = Some("Importe em um perfil vazio, sem alterações pendentes.".to_owned());
            cx.notify();
            return;
        }
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Importar arquivo de regras JSON".into()),
        });
        cx.spawn_in(window, async move |this,cx| {
            if let Ok(Ok(Some(paths))) = paths.await && let Some(path) = paths.first() {
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
        let application = self.policies[index].application_id.clone();
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Associar executável à regra".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
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
        if self.theme_picker.is_some() {
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
        cx.notify();
    }
    fn start_session(&mut self, cx: &mut Context<Self>) {
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
        if let Some(controller) = &self.controller
            && let Err(e) = controller.stop()
        {
            self.error = Some(format!("Não foi possível parar: {e:#}"));
        }
        cx.notify();
    }
    pub fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.waiting_for_stop {
            return;
        }
        if self.dirty {
            self.closing = true;
            cx.notify();
            return;
        }
        if self.controller.as_ref().is_some_and(|c| {
            let s = c.state();
            s.running || s.busy
        }) {
            self.stop_session(cx);
            self.waiting_for_stop = true;
            self.notice = Some("Aguardando o encerramento seguro da sessão…".to_owned());
            cx.spawn_in(window, async move |this, cx| {
                let deadline = Instant::now() + Duration::from_secs(60);
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    let Ok(done) = this.update(cx, |ws, cx| {
                        let state = ws
                            .controller
                            .as_ref()
                            .map(|c| c.state())
                            .unwrap_or_default();
                        if !state.running && !state.busy {
                            ws.waiting_for_stop = false;
                            if state.cleanup_confirmed {
                                cx.quit();
                            } else {
                                ws.error = Some(state.detail);
                                cx.notify();
                            }
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
        } else {
            cx.quit();
        }
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
                    .w(px(360.))
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
                            .child(icon("lan", 20., t.accent))
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
    fn connections(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let query = self.filter.read(cx).text().to_lowercase();
        let rows: Vec<_> = self
            .snapshot
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
            .collect();
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
                            .flex_col()
                            .gap(px(3.))
                            .child(c.name.clone())
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t.text_muted)
                                    .child(format!("PID {}", c.pid)),
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
                            .child(icon("desktop_windows", 18., t.text_muted))
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
                                ws.dirty = false;
                                ws.reload(cx);
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
                .child(div().font_weight(FontWeight::SEMIBOLD).child("NetLane · 0.4.0 Rust"))
                .child(div().text_color(t.text_muted).child(format!("Interface GPUI · aparência {:?}. Preferências salvas neste perfil.",t.appearance)))
                .child(div().font_family(MONO_FONT).text_size(px(11.)).text_color(t.text_muted).child(self.profile.display().to_string())))
    }
}
impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let settings = AppSettings::get(cx).clone();
        let mut nav = div().flex().flex_col().gap(px(4.)).p(px(10.));
        for page in [
            Page::Overview,
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
            .key_context(if self.theme_picker.is_some() || self.closing {
                "NetLaneOverlay"
            } else {
                "NetLane"
            })
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
                    .child(icon("lan", 18., t.accent))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("NetLane"))
                    .child(
                        div()
                            .flex_1()
                            .text_color(t.text_muted)
                            .text_size(px(11.))
                            .child("Cada app, sua rota"),
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
                                            .child("Sua rede"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(t.text_muted)
                                            .child("Interfaces e aplicativos"),
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
                                        ws.dirty = false;
                                        ws.closing = false;
                                        ws.request_close(w, cx);
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
