//! Default-connection controls. Demo changes an in-memory fixture only.
use super::*;
use netlane_core::default_connection::{
    self as domain, Interface, Journal, JournalFile, Kind, Metric, Route, Settings as _, Snapshot,
};
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(super) enum Confirmation {
    Apply(Uuid),
    Restore,
}
fn demo_snapshot() -> Snapshot {
    let interfaces: Vec<_> = demo::snapshot()
        .adapters
        .iter()
        .enumerate()
        .map(|(n, a)| Interface {
            id: Uuid::parse_str(&a.id).unwrap(),
            name: a.name.clone(),
            kind: if n == 0 { Kind::Cable } else { Kind::Wifi },
            index: n as u32 + 1,
            connected: true,
            automatic_metric: true,
            metric: if n == 0 { 25 } else { 35 },
            default_route_metric: Some(0),
            saved_metric: None,
        })
        .collect();
    let routes = interfaces
        .iter()
        .map(|i| Route {
            index: i.index,
            name: i.name.clone(),
            interface_metric: i.metric,
            route_metric: 0,
        })
        .collect();
    Snapshot { interfaces, routes }
}
struct DemoSettings(Snapshot);
impl domain::Settings for DemoSettings {
    fn read(&mut self) -> anyhow::Result<Snapshot> {
        Ok(self.0.clone())
    }
    fn write(&mut self, m: &Metric) -> anyhow::Result<()> {
        let i = self.0.interfaces.iter_mut().find(|i| i.id == m.id).unwrap();
        i.automatic_metric = m.automatic_metric;
        i.metric = if m.automatic_metric {
            if i.kind == Kind::Cable { 25 } else { 35 }
        } else {
            m.metric
        };
        i.saved_metric = (!m.automatic_metric).then_some(m.metric);
        for route in self.0.routes.iter_mut().filter(|r| r.index == i.index) {
            route.interface_metric = i.metric;
        }
        Ok(())
    }
}
fn perform(
    profile: PathBuf,
    executable: PathBuf,
    action: Confirmation,
    expected_revision: Option<String>,
) -> anyhow::Result<(Snapshot, Option<Journal>, Option<String>, String)> {
    perform_with(
        profile,
        action,
        expected_revision,
        &mut netlane_windows::default_connection::WindowsSettings,
        |_, request| netlane_agent::default_connection::execute(&executable, request),
    )
}
fn perform_with<S: domain::Settings>(
    profile: PathBuf,
    action: Confirmation,
    expected_revision: Option<String>,
    settings: &mut S,
    mut execute: impl FnMut(&mut S, domain::Request) -> anyhow::Result<domain::OperationResult>,
) -> anyhow::Result<(Snapshot, Option<Journal>, Option<String>, String)> {
    use anyhow::ensure;
    std::fs::create_dir_all(&profile)?;
    let file = JournalFile::new(profile.join("default-connection.json"));
    let _lease = netlane_core::persistence::exclusive_file(
        &profile.join("default-connection.operation.lock"),
    )?;
    let (existing, revision) = file.load()?;
    ensure!(
        revision == expected_revision,
        "O registro de recuperação mudou desde a leitura. Atualize antes de continuar."
    );
    let before = settings.read()?;
    let (journal, request, pending_revision) = match action {
        Confirmation::Apply(preferred) => {
            let (journal, request) = Journal::prepare(&before, preferred, existing.as_ref())?;
            // Unelevated, durable recovery BEFORE UAC. The helper never receives a profile path.
            let rev = file.save(&journal, revision.as_deref())?;
            (journal, Some(request), Some(rev))
        }
        Confirmation::Restore => {
            let journal = existing
                .ok_or_else(|| anyhow::anyhow!("Nenhuma configuração anterior para restaurar."))?;
            let request = journal.restoration(&before)?;
            (journal, request, revision)
        }
    };
    let verification = request.clone();
    let detail = if let Some(request) = request {
        let response = execute(settings, request)?;
        if !response.success {
            return Ok((
                response.snapshot,
                Some(journal),
                pending_revision,
                response.detail,
            ));
        }
        response.detail
    } else {
        "Configuração anterior já está ativa.".into()
    };
    // Reread after the helper exited. A later external change cannot finalize recovery.
    let current = settings.read()?;
    if let Some(request) = verification {
        request.verify_after(&current)?;
    }
    match action {
        Confirmation::Apply(_) => {
            let confirmed = journal.confirmed(&current)?;
            let revision = file.save(&confirmed, pending_revision.as_deref())?;
            Ok((current, Some(confirmed), Some(revision), detail))
        }
        Confirmation::Restore => {
            ensure!(
                journal.restoration(&current)?.is_none(),
                "A configuração original não foi confirmada; recuperação preservada."
            );
            file.clear(pending_revision.as_deref())?;
            Ok((current, None, None, detail))
        }
    }
}
impl Workspace {
    pub(super) fn initialize_default(&mut self, cx: &mut Context<Self>) {
        if self.demo {
            self.update_default(demo_snapshot(), None);
            self.default_status = "Demonstração · Aplicar e Restaurar usam dados fictícios.".into();
            self.default_valid = true;
        } else {
            self.refresh_default(cx);
        }
    }
    fn update_default(&mut self, snapshot: Snapshot, journal: Option<Journal>) {
        let selected = self
            .default_selected
            .filter(|id| snapshot.interfaces.iter().any(|i| i.id == *id));
        self.default_selected = selected.or_else(|| {
            let best = snapshot.routes.iter().min_by_key(|r| r.total());
            snapshot
                .interfaces
                .iter()
                .find(|i| best.is_some_and(|r| r.index == i.index))
                .or(snapshot.interfaces.first())
                .map(|i| i.id)
        });
        self.default_snapshot = Some(snapshot);
        self.default_journal = journal;
    }
    pub(super) fn refresh_default(&mut self, cx: &mut Context<Self>) {
        if self.default_busy || self.default_reading || self.default_confirmation.is_some() {
            return;
        }
        if self.demo {
            cx.notify();
            return;
        }
        self.default_reading = true;
        self.default_valid = false;
        let profile = self.profile.clone();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let snapshot = netlane_windows::default_connection::WindowsSettings.read()?;
                let (journal, revision) = JournalFile::new(profile.join("default-connection.json")).load()?;
                Ok::<_, anyhow::Error>((snapshot, journal, revision))
            }).await;
            let _ = this.update(cx, |ws, cx| {
                ws.default_reading = false;
                match result {
                    Ok((snapshot, journal, revision)) => {
                        if ws.default_journal.is_some() && journal.is_none() {
                            ws.default_status = "O registro de recuperação foi removido externamente. Os originais em memória foram preservados; confira o backup antes de continuar.".into();
                            cx.notify();
                            return;
                        }
                        ws.default_valid = true;
                        ws.default_revision = revision;
                        ws.update_default(snapshot, journal);
                        ws.default_status = if ws.default_journal.is_some() { "Configuração anterior disponível para restauração." } else { "Selecione uma conexão e clique em Aplicar. O Windows solicitará autorização." }.into();
                    }
                    Err(e) => { ws.default_snapshot = None; ws.default_status = format!("Não foi possível ler prioridade/recuperação: {e:#}"); }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn default_can_apply(&self) -> bool {
        self.default_valid
            && !self.default_reading
            && !self.desktop_blocked()
            && self.default_selected.is_some_and(|id| {
                self.default_snapshot.as_ref().is_some_and(|s| {
                    s.interface(id)
                        .is_ok_and(|i| i.connected && i.default_route_metric.is_some())
                })
            })
            && (self.demo || self.default_agent().is_some_and(|p| p.is_file()))
    }
    fn default_can_restore(&self) -> bool {
        self.default_valid
            && !self.default_reading
            && !self.desktop_blocked()
            && self.default_journal.is_some()
            && (self.demo || self.default_agent().is_some_and(|p| p.is_file()))
    }
    fn default_agent(&self) -> Option<PathBuf> {
        std::env::current_exe()
            .ok()
            .map(|p| p.with_file_name("netlane-agent.exe"))
    }
    fn ask_default(&mut self, restore: bool, cx: &mut Context<Self>) {
        if if restore {
            !self.default_can_restore()
        } else {
            !self.default_can_apply()
        } {
            return;
        }
        self.default_confirmation = Some(if restore {
            Confirmation::Restore
        } else {
            Confirmation::Apply(self.default_selected.unwrap())
        });
        self.restore_panel();
        cx.notify();
    }
    fn confirm_default(&mut self, cx: &mut Context<Self>) {
        let Some(action) = self.default_confirmation.take() else {
            return;
        };
        if self.default_busy || self.session_state().busy {
            cx.notify();
            return;
        }
        if self.demo {
            let attempt = (|| {
                let mut settings = DemoSettings(self.default_snapshot.clone().unwrap());
                let before = settings.read()?;
                let (journal, request) = match action {
                    Confirmation::Apply(id) => {
                        let (journal, request) =
                            Journal::prepare(&before, id, self.default_journal.as_ref())?;
                        (journal, Some(request))
                    }
                    Confirmation::Restore => {
                        let journal = self.default_journal.clone().unwrap();
                        let request = journal.restoration(&before)?;
                        (journal, request)
                    }
                };
                if let Some(request) = request {
                    let receipt = domain::apply(&mut settings, &request, || true)?;
                    anyhow::ensure!(receipt.result.success, "{}", receipt.result.detail);
                }
                let snapshot = settings.read()?;
                let journal = if matches!(action, Confirmation::Apply(_)) {
                    Some(journal.confirmed(&snapshot)?)
                } else {
                    None
                };
                Ok::<_, anyhow::Error>((snapshot, journal))
            })();
            match attempt {
                Ok((s, j)) => {
                    self.update_default(s, j);
                    self.default_status = if matches!(action, Confirmation::Restore) {
                        "Demonstração · configuração automática restaurada · rede real preservada."
                    } else {
                        "Demonstração · prioridade fictícia aplicada · rede real preservada."
                    }
                    .into();
                }
                Err(e) => self.default_status = e.to_string(),
            }
            cx.notify();
            return;
        }
        let Some(executable) = self.default_agent() else {
            self.default_status = "Componente de conexão padrão indisponível.".into();
            cx.notify();
            return;
        };
        self.default_busy = true;
        self.default_status =
            "Aguardando autorização, conferência e encerramento do componente…".into();
        let profile = self.profile.clone();
        let revision = self.default_revision.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { perform(profile, executable, action, revision) })
                .await;
            let _ = this.update(cx, |ws, cx| {
                ws.default_busy = false;
                match result {
                    Ok((snapshot, journal, revision, detail)) => {
                        ws.default_valid = true;
                        ws.default_revision = revision;
                        ws.update_default(snapshot, journal);
                        ws.default_status = detail;
                    }
                    Err(e) => {
                        ws.default_valid = false;
                        ws.default_status = format!("{e:#} Atualize a leitura antes de continuar.");
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn default_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let mut choices = div().flex().flex_wrap().items_center().gap(px(8.));
        if let Some(snapshot) = &self.default_snapshot {
            for i in &snapshot.interfaces {
                if i.default_route_metric.is_none()
                    && !self
                        .default_journal
                        .as_ref()
                        .is_some_and(|j| j.original.iter().any(|m| m.id == i.id))
                {
                    continue;
                }
                let id = i.id;
                let available = i.connected && i.default_route_metric.is_some();
                let selected = self.default_selected == Some(id);
                let label = format!(
                    "{} {}{}",
                    if selected { "●" } else { "○" },
                    i.name,
                    if available { "" } else { " · indisponível" }
                );
                choices = choices.child(
                    self.button(("default-choice", i.index as usize), &label, cx)
                        .when(selected, |d| d.bg(t.selected).border_color(t.accent))
                        .when(
                            !available || self.desktop_blocked() || self.default_reading,
                            |d| d.opacity(0.5),
                        )
                        .on_click(cx.listener(move |ws, _, _, cx| {
                            if !ws.desktop_blocked() && !ws.default_reading {
                                ws.default_selected = Some(id);
                                cx.notify();
                            }
                        })),
                );
            }
        }
        choices = choices
            .child(
                self.button("default-apply", "Aplicar", cx)
                    .when(!self.default_can_apply(), |d| d.opacity(0.5))
                    .on_click(cx.listener(|ws, _, _, cx| ws.ask_default(false, cx))),
            )
            .child(
                self.button("default-restore", "Restaurar anterior", cx)
                    .when(!self.default_can_restore(), |d| d.opacity(0.5))
                    .on_click(cx.listener(|ws, _, _, cx| ws.ask_default(true, cx))),
            );
        div()
            .w_full()
            .min_w_0()
            .flex_none()
            .p(px(16.))
            .rounded(px(8.))
            .bg(t.panel)
            .border_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Conexão padrão · IPv4"),
                    )
                    .child(
                        div().text_color(t.accent).child(
                            self.default_snapshot
                                .as_ref()
                                .map_or("Aguardando leitura…".into(), Snapshot::current_label),
                        ),
                    )
                    .child(
                        self.button(
                            "default-refresh",
                            if self.default_reading {
                                "Lendo…"
                            } else {
                                "Atualizar"
                            },
                            cx,
                        )
                        .on_click(cx.listener(|ws, _, _, cx| ws.refresh_default(cx))),
                    ),
            )
            .child(choices)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(t.text_muted)
                    .child(self.default_status.clone()),
            )
    }
    pub(super) fn default_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let label = match self.default_confirmation {
            Some(Confirmation::Apply(id)) => format!(
                "Usar {} como conexão padrão?",
                self.default_snapshot
                    .as_ref()
                    .and_then(|s| s.interface(id).ok())
                    .map_or("essa conexão", |i| i.name.as_str())
            ),
            _ => "Restaurar a configuração anterior?".into(),
        };
        div().absolute().inset_0().flex().items_center().justify_center().bg(t.chrome.opacity(0.75)).occlude()
            .child(div().w(px(480.)).max_w(gpui::relative(0.9)).p(px(24.)).rounded(px(8.)).bg(t.elevated).border_1().border_color(t.border).flex().flex_col().gap(px(16.))
                .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child(label))
                .child(if self.demo { "Demonstração: esta ação altera somente os valores fictícios do painel." } else { "A prioridade IPv4 fica salva no Windows mesmo após fechar o NetLane. A configuração anterior será preservada para restauração. Conexões já abertas podem continuar pela saída anterior." })
                .child(div().text_color(t.text_muted).child("VPNs e rotas específicas podem definir outra saída. A prioridade não comprova acesso à Internet."))
                .child(div().flex().justify_end().gap(px(10.))
                    .child(self.button("default-cancel", "Cancelar", cx).on_click(cx.listener(|ws, _, _, cx| { ws.default_confirmation = None; cx.notify(); })))
                    .child(self.button("default-confirm", if self.demo { "Confirmar simulação" } else { "Confirmar e autorizar" }, cx).on_click(cx.listener(|ws, _, _, cx| ws.confirm_default(cx))))))
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_default_select(&mut self, cx: &mut Context<Self>) {
        assert!(self.demo && self.default_journal.is_none());
        self.page = Page::Overview;
        self.default_selected = Some(self.default_snapshot.as_ref().unwrap().interfaces[1].id);
        assert!(
            self.default_snapshot
                .as_ref()
                .unwrap()
                .interfaces
                .iter()
                .all(|i| i.automatic_metric)
        );
        self.ask_default(false, cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_default_apply(&mut self, cx: &mut Context<Self>) {
        assert!(self.default_confirmation.is_some());
        self.confirm_default(cx);
        assert_eq!(
            self.default_snapshot.as_ref().unwrap().current_label(),
            "Atual: Wi-Fi"
        );
        assert!(
            self.default_journal
                .as_ref()
                .unwrap()
                .original
                .iter()
                .all(|m| m.automatic_metric)
        );
        assert!(!self.profile.join("default-connection.json").exists());
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_default_restore(&mut self, cx: &mut Context<Self>) {
        self.ask_default(true, cx);
        self.confirm_default(cx);
        assert_eq!(
            self.default_snapshot.as_ref().unwrap().current_label(),
            "Atual: Ethernet"
        );
        assert!(
            self.default_snapshot
                .as_ref()
                .unwrap()
                .interfaces
                .iter()
                .all(|i| i.automatic_metric)
        );
        assert!(self.default_journal.is_none());
        assert!(!self.profile.join("default-connection.json").exists());
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_default_busy(&mut self, busy: bool) {
        assert!(self.demo);
        self.default_busy = busy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn journal_is_durable_before_authorization_and_cancelled_uac_needs_no_restore_launch() {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().to_owned();
        let file = JournalFile::new(profile.join("default-connection.json"));
        let mut settings = DemoSettings(demo_snapshot());
        let before = settings.0.clone();
        let action = Confirmation::Apply(before.interfaces[1].id);
        let result = perform_with(profile.clone(), action, None, &mut settings, |_, _| {
            let (pending, _) = file.load().unwrap();
            let pending = pending.unwrap();
            assert!(pending.original.iter().all(|m| m.automatic_metric));
            assert_eq!(
                pending
                    .applied
                    .iter()
                    .find(|m| m.id == before.interfaces[1].id)
                    .unwrap()
                    .metric,
                5
            );
            anyhow::bail!("Authorization cancelled fixture");
        });
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert_eq!(settings.0, before);
        let revision = file.load().unwrap().1;
        let (_, journal, revision, _) = perform_with(
            profile,
            Confirmation::Restore,
            revision,
            &mut settings,
            |_, _| panic!("Already original: must never launch"),
        )
        .unwrap();
        assert!(journal.is_none() && revision.is_none() && !file.path.exists());
    }
    #[test]
    fn removed_journal_and_changed_metric_before_finalization_preserve_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().to_owned();
        let file = JournalFile::new(profile.join("default-connection.json"));
        let mut settings = DemoSettings(demo_snapshot());
        let action = Confirmation::Apply(settings.0.interfaces[1].id);
        let result = perform_with(
            profile.clone(),
            action,
            None,
            &mut settings,
            |settings, request| {
                let receipt = domain::apply(settings, &request, || true)?;
                assert!(receipt.result.success);
                // External setting changes AFTER the receipt but BEFORE controller finalization.
                let i = &mut settings.0.interfaces[1];
                i.metric = 77;
                i.saved_metric = Some(77);
                settings.0.routes[1].interface_metric = 77;
                Ok(receipt.result)
            },
        );
        assert!(result.is_err());
        let (pending, revision) = file.load().unwrap();
        assert!(!pending.unwrap().previous.is_empty());
        assert_eq!(settings.0.interfaces[1].metric, 77);
        std::fs::remove_file(&file.path).unwrap();
        assert!(
            perform_with(profile, action, revision, &mut settings, |_, _| panic!(
                "Stale file: must never launch"
            ))
            .is_err()
        );
        assert!(!file.path.exists());
    }
}
