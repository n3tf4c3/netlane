//! Reviewed capture context, bounded Codex conversation and explicit authentication.
use crate::{
    text_input::{TextInput, TextInputEvent},
    theme::{ActiveTheme, MONO_FONT},
};
use gpui::{
    App, ClipboardItem, Context, Entity, EventEmitter, FontWeight, Render, ScrollHandle,
    Subscription, Window, div, prelude::*, px,
};
use netlane_ai::{AgentHandle, Command, Event as AgentEvent, ModelChoice, ModelOption};
use netlane_ai::{
    Context as EvidenceContext,
    codex::{Probe, ProbeState},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{path::PathBuf, time::Duration};
pub(crate) mod bundle;
mod chat;
mod notebook;
mod resume;
pub struct ContextPrepared;
pub struct NotebookOpened;
/// A question asked without a capture. Kept for this session only, outside the
/// notebook, whose turns are always tied to reviewed capture evidence.
pub(super) struct FreeTurn {
    question: String,
    answer: String,
    status: String,
    provider: String,
}
/// Display order of notebook turns and free questions in the conversation.
#[derive(Clone, Copy)]
pub(super) enum Entry {
    Turn(usize),
    Free(usize),
}
pub struct CodexView {
    demo: bool,
    profile: PathBuf,
    evidence: Option<EvidenceContext>,
    conversation_capture: Option<String>,
    problem: Entity<TextInput>,
    question: Entity<TextInput>,
    executable: Entity<TextInput>,
    edit_executable: bool,
    preview: Option<String>,
    status: String,
    probe: Option<Probe>,
    probe_state: ProbeState,
    agent: Option<AgentHandle>,
    agent_events: Option<netlane_ai::agent::EventReceiver>,
    agent_ready: bool,
    agent_status: String,
    models: Vec<ModelOption>,
    model: Option<ModelChoice>,
    model_pending: bool,
    conversation: u64,
    answering: bool,
    answer: String,
    sent_context: Option<EvidenceContext>,
    sent_report: Option<netlane_ai::report::Draft>,
    notebook: netlane_ai::investigation::Notebook,
    note_input: Entity<TextInput>,
    note_context: Option<(EvidenceContext, Option<usize>)>,
    last_turn: Option<usize>,
    history_page: Option<usize>,
    chat_scroll: ScrollHandle,
    notebook_revision: u64,
    saved_revision: u64,
    confirm_new: bool,
    connection_open: bool,
    notes_open: bool,
    problem_open: bool,
    dialog: Arc<AtomicBool>,
    capture_active: Arc<AtomicBool>,
    sources: Vec<netlane_ai::bundle::Source>,
    bundle_cancel: Arc<AtomicBool>,
    bundle_progress: Arc<netlane_ai::bundle::Progress>,
    bundle_busy: bool,
    bundle_saved: Option<(u64, Option<EvidenceContext>, String, String)>,
    free_turns: Vec<FreeTurn>,
    order: Vec<Entry>,
    /// Question of the free turn in progress, if any.
    sent_free: Option<String>,
    /// Opt-in: live diagnostic snapshots are sent automatically while true.
    following: bool,
    follow_confirm: bool,
    follow_confirmed: bool,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<ContextPrepared> for CodexView {}
impl EventEmitter<NotebookOpened> for CodexView {}
impl CodexView {
    pub fn new(
        demo: bool,
        profile: PathBuf,
        dialog: Arc<AtomicBool>,
        capture_active: Arc<AtomicBool>,
        cx: &mut Context<Self>,
    ) -> Self {
        let problem =
            cx.new(|cx| TextInput::new("", "Descreva o problema observado neste computador", cx));
        let note_input =
            cx.new(|cx| TextInput::new("", "Hipótese, possível problema ou algo a verificar", cx));
        let question = cx.new(|cx| TextInput::new("", "Pergunte ao Codex sobre esta captura…", cx));
        let discovered = if demo {
            None
        } else {
            netlane_ai::codex::discover()
        };
        let executable = cx.new(|cx| {
            TextInput::new(
                discovered.map_or_else(String::new, |p| p.to_string_lossy().into_owned()),
                "Caminho completo de codex.exe",
                cx,
            )
        });
        let mut subscriptions = [&problem, &question]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |this, _, event, cx| {
                    if matches!(event, TextInputEvent::Changed) {
                        let current = this.evidence.as_ref().and_then(|c| {
                            c.prompt(this.problem.read(cx).text(), this.question.read(cx).text())
                                .ok()
                        });
                        if this.preview != current {
                            this.preview = None;
                        }
                        cx.notify();
                    }
                })
            })
            .collect::<Vec<_>>();
        subscriptions.push(cx.subscribe(&question, |this, _, event, cx| {
            if matches!(event, TextInputEvent::Confirmed) {
                // Without capture evidence only the typed text leaves; while following, the
                // user already authorized the snapshot metadata: no review step either.
                if this.preview.is_some() || this.evidence.is_none() || this.following {
                    this.send(cx);
                } else {
                    this.review(cx);
                }
            }
        }));
        subscriptions.push(cx.subscribe(&note_input, |this, _, event, cx| {
            if matches!(event, TextInputEvent::Changed) {
                if this.note_input.read(cx).text().trim().is_empty() {
                    this.note_context = None;
                } else if this.note_context.is_none() {
                    this.note_context = this.evidence.clone().map(|c| (c, None));
                }
                cx.notify();
            }
        }));
        Self {
            demo,
            profile,
            evidence: None,
            conversation_capture: None,
            problem,
            question,
            executable,
            edit_executable: false,
            preview: None,
            status: "Selecione um pacote e use Preparar para Codex.".into(),
            probe: None,
            probe_state: ProbeState::default(),
            agent: None,
            agent_events: None,
            agent_ready: false,
            agent_status: "Conecte o Codex para conversar sobre o contexto revisado.".into(),
            models: Vec::new(),
            model: None,
            model_pending: false,
            conversation: 1,
            answering: false,
            answer: String::new(),
            sent_context: None,
            sent_report: None,
            notebook: Default::default(),
            note_input,
            note_context: None,
            last_turn: None,
            history_page: None,
            chat_scroll: ScrollHandle::new(),
            notebook_revision: 0,
            saved_revision: 0,
            confirm_new: false,
            connection_open: false,
            notes_open: false,
            problem_open: false,
            dialog,
            capture_active,
            sources: vec![],
            bundle_cancel: Arc::new(AtomicBool::new(false)),
            bundle_progress: Arc::new(netlane_ai::bundle::Progress::default()),
            bundle_busy: false,
            bundle_saved: None,
            free_turns: Vec::new(),
            order: Vec::new(),
            sent_free: None,
            following: false,
            follow_confirm: false,
            follow_confirmed: false,
            _subscriptions: subscriptions,
        }
    }
    #[cfg(feature = "gui-test")]
    pub fn prepare(&mut self, context: EvidenceContext, cx: &mut Context<Self>) {
        self.prepare_selection(context, false, cx);
    }
    pub fn prepare_selection(
        &mut self,
        context: EvidenceContext,
        replace: bool,
        cx: &mut Context<Self>,
    ) {
        if self.answering || self.dialog.load(Ordering::Relaxed) {
            self.status="A resposta pertence à análise atual. Cancele ou aguarde antes de trocar as evidências.".into();
            cx.emit(ContextPrepared);
            cx.notify();
            return;
        }
        match context.select(self.evidence.as_ref(), replace) {
            Ok(context) => {
                if self
                    .conversation_capture
                    .as_ref()
                    .is_some_and(|hash| hash != &context.sha256)
                {
                    self.reset_conversation();
                }
                self.conversation_capture = Some(context.sha256.clone());
                self.evidence = Some(context);
                self.preview = None;
                self.status =
                    "Metadados selecionados. Revise o contexto antes de copiar para o Codex."
                        .into();
            }
            Err(error) => self.status = error.to_string(),
        }
        cx.emit(ContextPrepared);
        cx.notify();
    }
    fn remove_packet(&mut self, number: u64, cx: &mut Context<Self>) {
        if self.answering {
            return;
        }
        if let Some(context) = &mut self.evidence {
            context.packets.retain(|p| p.number != number);
            if context.packets.is_empty() {
                self.evidence = None;
            }
            self.preview = None;
            self.status = "Anexo removido do próximo envio. Histórico e anotações preservados; revise a nova seleção.".into();
        }
        cx.notify();
    }
    fn review(&mut self, cx: &mut Context<Self>) {
        let result = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Prepare primeiro os pacotes da análise."))
            .and_then(|data| {
                data.prompt(self.problem.read(cx).text(), self.question.read(cx).text())
            });
        match result {
            Ok(preview) => {
                self.preview = Some(preview);
                self.status="Este é o texto completo que será copiado. Confira os endereços e as evidências.".into();
            }
            Err(error) => {
                self.preview = None;
                self.status = error.to_string();
            }
        }
        cx.notify();
    }
    fn copy(&mut self, cx: &mut Context<Self>) {
        let current = self.evidence.as_ref().and_then(|c| {
            c.prompt(self.problem.read(cx).text(), self.question.read(cx).text())
                .ok()
        });
        if let Some(current) = current
            && self.preview.as_ref() == Some(&current)
        {
            cx.write_to_clipboard(ClipboardItem::new_string(current));
            self.status =
                "Contexto copiado. Cole na sua conversa com Codex para continuar a análise.".into();
        } else {
            self.status = "Revise o contexto atualizado antes de copiar.".into();
        }
        cx.notify();
    }
    fn reset(&mut self, cx: &mut Context<Self>) {
        if self.answering || self.dialog.load(Ordering::Relaxed) {
            self.status = "Cancele ou aguarde a resposta antes de abrir outra análise.".into();
            cx.notify();
            return;
        }
        if (!self.notebook.is_empty() || !self.note_input.read(cx).text().trim().is_empty())
            && !self.confirm_new
        {
            self.confirm_new = true;
            self.status = "Salve o caderno para conservar a conversa e as anotações antes de iniciar outra investigação.".into();
            cx.notify();
            return;
        }
        self.reset_conversation();
        self.notebook = Default::default();
        self.free_turns.clear();
        self.order.clear();
        self.following = false;
        self.sources.clear();
        self.bundle_saved = None;
        self.notebook_revision = 0;
        self.saved_revision = 0;
        self.history_page = None;
        self.chat_scroll.set_offset(Default::default());
        self.confirm_new = false;
        self.note_context = None;
        self.note_input.update(cx, |f, cx| f.set_text("", cx));
        self.evidence = None;
        self.preview = None;
        self.problem.update(cx, |f, cx| f.set_text("", cx));
        self.question.update(cx, |f, cx| f.set_text("", cx));
        self.status = "Nova análise. Selecione os pacotes que deseja levar ao Codex.".into();
        cx.notify();
    }
    fn reset_conversation(&mut self) {
        if let Some(agent) = &self.agent {
            let _ = agent.send(Command::Reset {
                conversation: self.conversation,
            });
        }
        self.conversation = self.conversation.wrapping_add(1);
        self.conversation_capture = None;
        self.answer.clear();
        self.sent_context = None;
        self.sent_report = None;
        self.last_turn = None;
    }
    fn connect_agent(&mut self, cx: &mut Context<Self>) {
        if self.demo {
            self.agent_ready = true;
            self.agent_status = "Codex simulado · nenhuma conta ou inferência usada.".into();
            cx.notify();
            return;
        }
        if self.agent.is_some() {
            return;
        }
        let info = netlane_ai::bridge::CodexInfo {
            executable: PathBuf::from(self.executable.read(cx).text().trim()),
            version: netlane_ai::bridge::CODEX_VERSION.into(),
        };
        let (events, receiver) = netlane_ai::agent::events();
        match netlane_ai::bridge::start(
            &info,
            self.profile.join("codex-proposals/home"),
            self.profile.join("codex-proposals/room"),
            events,
        ) {
            Ok(agent) => {
                self.agent = Some(agent);
                self.agent_events = Some(receiver);
                self.agent_status = "Validando perfil dedicado e iniciando o Codex…".into();
                cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                        let Ok(done) = this.update(cx, |this, cx| {
                            let Some(receiver) = &mut this.agent_events else {
                                return true;
                            };
                            let mut batch = Vec::new();
                            let mut disconnected = false;
                            for _ in 0..32 {
                                match receiver.try_recv() {
                                    Ok(event) => batch.push(event),
                                    Err(netlane_ai::agent::TryRecvError::Empty) => break,
                                    Err(_) => {
                                        disconnected = true;
                                        break;
                                    }
                                }
                            }
                            for event in batch {
                                this.agent_event(event, cx);
                            }
                            if disconnected && this.agent_events.is_some() {
                                this.break_conversation(
                                    "Conexão encerrada sem confirmação; resposta parcial.",
                                );
                                this.agent = None;
                                this.agent_events = None;
                                this.agent_ready = false;
                                this.answering = false;
                                cx.notify();
                                return true;
                            }
                            this.agent_events.is_none()
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
            Err(error) => self.agent_status = error.to_string(),
        }
        cx.notify();
    }
    fn break_conversation(&mut self, reason: &str) {
        self.finish_turn(reason);
        self.conversation = self.conversation.wrapping_add(1);
        self.answering = false;
        self.model = None;
        self.models.clear();
        self.model_pending = false;
        // Keep immutable turns, annotations and the last report available locally.
        // Provider threads are ephemeral and cannot be resumed after logout/exit.
        self.status = "Conversa do provedor encerrada. O próximo envio inicia outra conversa; revise e envie explicitamente o contexto necessário. Histórico e caderno continuam disponíveis.".into();
        self.preview = None;
    }
    fn agent_command(&mut self, command: Command, cx: &mut Context<Self>) {
        if let Some(agent) = &self.agent {
            if let Err(error) = agent.send(command) {
                self.agent_status = error.to_string();
            }
        } else {
            self.agent_status = "Conecte o Codex primeiro.".into();
        }
        cx.notify();
    }
    fn agent_event(&mut self, event: AgentEvent, cx: &mut Context<Self>) {
        match event {
            AgentEvent::LoginRequired => {
                self.agent_ready = false;
                self.agent_status="Perfil pronto. Use Entrar com ChatGPT; sua conta pessoal do Codex não foi copiada.".into();
            }
            AgentEvent::LoginUrl { url } => {
                if netlane_ai::bridge::valid_login_url(&url) {
                    cx.open_url(&url);
                    self.agent_status =
                        "Conclua o login no navegador. O NetLane aguarda a confirmação.".into();
                }
            }
            AgentEvent::LoggedOut => {
                self.break_conversation("Conta desconectada; resposta possivelmente parcial.");
                self.agent_ready = false;
                self.answering = false;
                self.agent_status = "Conta desconectada. Ao entrar novamente, o Codex terá uma nova conversa; o histórico local foi preservado.".into();
            }
            AgentEvent::Ready { agent } => {
                self.agent_ready = true;
                self.agent_status = agent;
            }
            AgentEvent::Models { options, selected } => {
                self.models = options;
                self.model = Some(selected);
                self.model_pending = false;
            }
            AgentEvent::Text { conversation, text }
                if self.answering && conversation == self.conversation =>
            {
                if self.answer.len() + text.len() <= 128 * 1024 {
                    self.follow_answer();
                    self.answer.push_str(&text);
                }
            }
            AgentEvent::Done { conversation, note }
                if self.answering && conversation == self.conversation =>
            {
                self.finish_turn(if self.demo {
                    "Resposta fictícia; nenhuma inferência enviada."
                } else if note.is_some() {
                    "Turno encerrado com aviso; pode conter resposta parcial."
                } else {
                    "Turno concluído; conteúdo ainda requer revisão."
                });
                self.status = note.unwrap_or_else(|| {
                    "Resposta concluída. Revise fatos, hipóteses e referências.".into()
                });
                if let Some(context) = &self.sent_context
                    && context.validate_references(&self.answer).is_err()
                {
                    self.status="A resposta contém referência fora das evidências enviadas. Confira antes de usar em relatório.".into();
                }
            }
            AgentEvent::Failed {
                conversation,
                message,
            } if conversation.is_none_or(|id| id == self.conversation) => {
                self.finish_turn("Falha no turno; resposta parcial.");
                self.status = message;
            }
            AgentEvent::Blocked { conversation, what } if conversation == self.conversation => {
                self.status = format!("Capacidade recusada: {what}. Encerrando o agente.");
            }
            AgentEvent::Closed { message } => {
                self.break_conversation("Agente encerrado; resposta parcial.");
                self.agent = None;
                self.agent_events = None;
                self.agent_ready = false;
                self.answering = false;
                self.agent_status = format!(
                    "{message} Ao reconectar, o Codex terá uma nova conversa; o histórico local foi preservado."
                );
            }
            _ => {}
        }
        cx.notify();
    }
    /// Shared gate for every kind of send.
    fn can_send(&mut self) -> bool {
        if self.dialog.load(Ordering::Relaxed) {
            self.status = "Aguarde a abertura ou gravação atual antes de enviar.".into();
            return false;
        }
        if self.answering {
            return false;
        }
        if !self.agent_ready {
            self.status = "Conecte o Codex e entre com ChatGPT antes de enviar.".into();
            return false;
        }
        if !self.demo && (self.model_pending || self.model.is_none()) {
            self.status = "Aguarde a confirmação do modelo antes de enviar.".into();
            return false;
        }
        true
    }
    fn send(&mut self, cx: &mut Context<Self>) {
        if !self.can_send() {
            cx.notify();
            return;
        }
        let Some(context) = self.evidence.clone() else {
            self.send_free(cx);
            return;
        };
        if let Err(error) = self.notebook.can_start() {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        let problem = self.problem.read(cx).text().to_owned();
        let question = self.question.read(cx).text().to_owned();
        let Ok(prompt) = context.prompt(&problem, &question) else {
            self.status = "Confira problema, pergunta e limites do contexto.".into();
            cx.notify();
            return;
        };
        // Following was confirmed for this metadata shape; otherwise review first.
        if !self.following && self.preview.as_ref() != Some(&prompt) {
            self.status = "Revise o contexto atualizado antes de enviar.".into();
            cx.notify();
            return;
        }
        if self.submit(context, prompt, problem, question, cx) {
            self.question.update(cx, |field, cx| field.set_text("", cx));
        }
        cx.notify();
    }
    /// Sends one evidence turn and records it in the notebook when it ends.
    fn submit(
        &mut self,
        context: EvidenceContext,
        prompt: String,
        problem: String,
        question: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let sent_report = netlane_ai::report::Draft {
            context: context.clone(),
            problem,
            question,
            response: Some(netlane_ai::report::Response {
                provider: if self.demo { "Codex simulado" } else { "Codex" }.into(),
                model: if self.demo {
                    "fictício".into()
                } else {
                    self.model.as_ref().map_or("não informado".into(), |m| {
                        format!("{} (esforço {})", m.model, m.effort)
                    })
                },
                status: "Turno em andamento.".into(),
                text: String::new(),
            }),
        };
        if self.demo {
            self.begin_turn(Some(context.clone()), Some(sent_report), None);
            let number = context.packets[0].number;
            let hash = context.sha256.clone();
            self.agent_event(AgentEvent::Text {conversation:self.conversation,text:format!("Demonstração de análise, sem provedor real. O resumo local cobre {} pacotes; os pacotes anexados são amostras. Referência verificável: [[packet:{hash}:{number}]]. Diferenças em relação ao baseline são indícios e exigem conferir filtro, período e ponto de coleta.",context.inspected_packets)},cx);
            self.agent_event(
                AgentEvent::Done {
                    conversation: self.conversation,
                    note: Some("Resposta fictícia. Nenhuma inferência enviada.".into()),
                },
                cx,
            );
            return true;
        }
        let Some(agent) = &self.agent else {
            return false;
        };
        match agent.send(Command::PromptWithModel {
            conversation: self.conversation,
            text: prompt,
            model: self.model.clone().unwrap(),
        }) {
            Ok(()) => {
                self.begin_turn(Some(context), Some(sent_report), None);
                self.status = "Analisando o contexto revisado…".into();
                true
            }
            Err(error) => {
                self.status = error.to_string();
                false
            }
        }
    }
    fn begin_turn(
        &mut self,
        context: Option<EvidenceContext>,
        report: Option<netlane_ai::report::Draft>,
        free: Option<String>,
    ) {
        self.last_turn = None;
        self.sent_context = context;
        self.sent_report = report;
        self.sent_free = free;
        self.answer.clear();
        self.preview = None;
        self.answering = true;
        self.history_page = None;
        self.chat_scroll.scroll_to_bottom();
    }
    /// No capture attached: only the typed problem and question are sent.
    fn send_free(&mut self, cx: &mut Context<Self>) {
        let problem = self.problem.read(cx).text().to_owned();
        let question = self.question.read(cx).text().to_owned();
        let prompt = match netlane_ai::general_prompt(&problem, &question) {
            Ok(prompt) => prompt,
            Err(error) => {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        if self.demo {
            self.begin_turn(None, None, Some(question));
            self.agent_event(
                AgentEvent::Text {
                    conversation: self.conversation,
                    text: "Demonstração: resposta fictícia a uma pergunta sem captura anexada. Para analisar tráfego, anexe pacotes em Pacotes.".into(),
                },
                cx,
            );
            self.agent_event(
                AgentEvent::Done {
                    conversation: self.conversation,
                    note: Some("Resposta fictícia. Nenhuma inferência enviada.".into()),
                },
                cx,
            );
        } else if let Some(agent) = &self.agent {
            match agent.send(Command::PromptWithModel {
                conversation: self.conversation,
                text: prompt,
                model: self.model.clone().unwrap(),
            }) {
                Ok(()) => {
                    self.begin_turn(None, None, Some(question));
                    self.status = "Pensando…".into();
                }
                Err(error) => {
                    self.status = error.to_string();
                    cx.notify();
                    return;
                }
            }
        }
        self.question.update(cx, |field, cx| field.set_text("", cx));
        cx.notify();
    }
    pub fn following(&self) -> bool {
        self.following && self.agent_ready && !self.answering
    }
    fn toggle_follow(&mut self, cx: &mut Context<Self>) {
        if self.following {
            self.following = false;
            self.status = "Acompanhamento desligado.".into();
        } else if !self.follow_confirmed {
            self.follow_confirm = true;
        } else {
            self.start_following();
        }
        cx.notify();
    }
    fn start_following(&mut self) {
        self.follow_confirm = false;
        self.follow_confirmed = true;
        self.following = true;
        self.status = if !self.agent_ready {
            "Acompanhamento ligado; conecte o Codex para começar."
        } else if self.capture_active.load(Ordering::Relaxed) {
            "Acompanhando: o primeiro resumo sai agora e depois a cada 30 s com pacotes novos; suas perguntas levam a foto mais recente."
        } else {
            "Acompanhamento ligado. Inicie uma captura em modo Diagnóstico em Pacotes."
        }
        .into();
    }
    /// Opt-in automatic turn while a diagnostic capture runs. The user confirmed
    /// this mode once; the context is the same metadata-only shape as a manual send
    /// and the turn is recorded in the notebook. Returns whether it was sent.
    pub fn follow_snapshot(
        &mut self,
        context: EvidenceContext,
        question: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.following() || !self.can_send() {
            return false;
        }
        if let Err(error) = self.notebook.can_start() {
            self.following = false;
            self.status = format!("Acompanhamento pausado: {error}");
            cx.notify();
            return false;
        }
        let problem = self.problem.read(cx).text().to_owned();
        let prompt = match context.prompt(&problem, &question) {
            Ok(prompt) => prompt,
            Err(error) => {
                self.status = format!("Resumo de acompanhamento recusado: {error}");
                cx.notify();
                return false;
            }
        };
        // Same conversation across snapshots: each one has a new hash by design.
        self.conversation_capture = Some(context.sha256.clone());
        self.evidence = Some(context.clone());
        let sent = self.submit(context, prompt, problem, question, cx);
        if sent {
            self.status = "Acompanhamento: resumo da captura enviado ao Codex.".into();
        }
        cx.notify();
        sent
    }
    fn report_draft(
        &self,
        include_answer: bool,
        cx: &App,
    ) -> anyhow::Result<netlane_ai::report::Draft> {
        if include_answer {
            anyhow::ensure!(
                !self.answering,
                "Aguarde o encerramento do turno antes de exportar."
            );
            let mut draft = self
                .sent_report
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Nenhuma resposta recebida."))?;
            draft.response.as_mut().unwrap().text = self.answer.clone();
            draft.validate()?;
            Ok(draft)
        } else {
            let context = self
                .evidence
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Prepare as evidências primeiro."))?;
            let problem = self.problem.read(cx).text().to_owned();
            let question = self.question.read(cx).text().to_owned();
            let prompt = context.prompt(&problem, &question)?;
            anyhow::ensure!(
                self.preview.as_ref() == Some(&prompt),
                "Revise o contexto atualizado antes de exportar o relatório local."
            );
            Ok(netlane_ai::report::Draft {
                context,
                problem,
                question,
                response: None,
            })
        }
    }
    fn export_report(&mut self, include_answer: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.load(Ordering::Relaxed) {
            return;
        }
        let draft = match self.report_draft(include_answer, cx) {
            Ok(d) => d,
            Err(e) => {
                self.status = format!("Relatório recusado: {e:#}");
                cx.notify();
                return;
            }
        };
        self.dialog.store(true, Ordering::Relaxed);
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Escolha onde salvar uma nova pasta de relatório Markdown e HTML".into()),
        });
        cx.spawn_in(window,async move|this,cx|{
            let parent=match paths.await {Ok(Ok(Some(paths)))=>paths.into_iter().next(),_=>None};
            let Some(parent)=parent else {let _=this.update(cx,|this,cx|{this.dialog.store(false,Ordering::Relaxed);cx.notify();});return;};
            let directory=parent.join(format!("NetLane-relatorio-{}",uuid::Uuid::new_v4()));let destination=directory.clone();
            let _=this.update(cx,|this,cx|{this.status="Salvando rascunho e contexto em segundo plano…".into();cx.notify();});
            let result=cx.background_executor().spawn(async move{draft.save(&destination)}).await;
            let _=this.update(cx,|this,cx|{
                this.dialog.store(false,Ordering::Relaxed);
                this.status=match result {Ok(_)=>format!("Rascunho Markdown/HTML, contexto e hashes salvos em {}. Revise antes de compartilhar; inclui endereços e textos selecionados.",directory.display()),Err(e)=>format!("Relatório não concluído: {e:#}. Arquivos parciais, se houver, preservados em {}.",directory.display())};cx.notify();
            });
        }).detach();
    }
    fn cycle_model(&mut self, cx: &mut Context<Self>) {
        if self.answering || self.model_pending || self.models.is_empty() {
            return;
        }
        let next = self
            .model
            .as_ref()
            .and_then(|m| self.models.iter().position(|option| option.id == m.model))
            .map_or(0, |index| (index + 1) % self.models.len());
        let option = &self.models[next];
        // Keep the chosen effort when the next model supports it.
        let effort = self
            .model
            .as_ref()
            .map(|m| m.effort.clone())
            .filter(|effort| option.efforts.contains(effort))
            .unwrap_or_else(|| "medium".into());
        self.request_model(
            ModelChoice {
                model: option.id.clone(),
                effort,
            },
            cx,
        );
    }
    fn cycle_effort(&mut self, cx: &mut Context<Self>) {
        if self.answering || self.model_pending {
            return;
        }
        let Some(current) = self.model.clone() else {
            return;
        };
        let Some(efforts) = self
            .models
            .iter()
            .find(|option| option.id == current.model)
            .map(|option| &option.efforts)
            .filter(|efforts| !efforts.is_empty())
        else {
            return;
        };
        let next = efforts
            .iter()
            .position(|effort| *effort == current.effort)
            .map_or(0, |index| (index + 1) % efforts.len());
        let effort = efforts[next].clone();
        self.request_model(
            ModelChoice {
                model: current.model,
                effort,
            },
            cx,
        );
    }
    /// The bridge confirms with a Models event; sending waits for it.
    fn request_model(&mut self, selected: ModelChoice, cx: &mut Context<Self>) {
        if let Some(agent) = &self.agent {
            match agent.send(Command::SetModel(selected)) {
                Ok(()) => {
                    self.model_pending = true;
                    self.agent_status = "Aguardando confirmação do modelo…".into();
                }
                Err(error) => self.agent_status = error.to_string(),
            }
        }
        cx.notify();
    }
    fn visible_answer(&self) -> &str {
        let mut start = self.answer.len().saturating_sub(16 * 1024);
        while !self.answer.is_char_boundary(start) {
            start += 1;
        }
        &self.answer[start..]
    }
    fn check_connection(&mut self, cx: &mut Context<Self>) {
        if self.probe.as_ref().is_some_and(|p| !p.state().done) {
            return;
        }
        if self.demo {
            self.probe_state = ProbeState {
                done: true,
                connected: true,
                detail: "Verificação fictícia. Nenhum processo Codex foi iniciado.".into(),
                ..ProbeState::default()
            };
            cx.notify();
            return;
        }
        match Probe::start(
            PathBuf::from(self.executable.read(cx).text().trim()),
            self.profile.clone(),
        ) {
            Ok(probe) => {
                self.probe_state = probe.state();
                self.probe = Some(probe);
                cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                        let Ok(done) = this.update(cx, |this, cx| {
                            let Some(probe) = &this.probe else {
                                return true;
                            };
                            let state = probe.state();
                            if state.done {
                                this.probe_state = state;
                                this.probe = None;
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
            Err(error) => {
                self.probe_state = ProbeState {
                    done: true,
                    detail: error.to_string(),
                    ..ProbeState::default()
                }
            }
        }
        cx.notify();
    }
    fn button(id: impl Into<gpui::ElementId>, label: &str, cx: &App) -> gpui::Stateful<gpui::Div> {
        let t = cx.theme();
        div()
            .id(id)
            .px(px(12.))
            .py(px(7.))
            .rounded(px(4.))
            .bg(t.elevated)
            .border_1()
            .border_color(t.border)
            .cursor_pointer()
            .hover(|d| d.bg(t.hover))
            .child(label.to_owned())
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_review(&mut self, cx: &mut Context<Self>) {
        self.review(cx);
        self.check_connection(cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_context(&self) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|p| p.contains("192.0.2.10") && !p.contains("0000  "))
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_analysis(&mut self, cx: &mut Context<Self>) {
        if self.question.read(cx).text().trim().is_empty() {
            self.question.update(cx, |q, cx| {
                q.set_text(
                    "Quais hipóteses estes pacotes sustentam e o que falta verificar?",
                    cx,
                )
            });
        }
        self.connect_agent(cx);
        self.review(cx);
        self.send(cx);
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_answer(&self) -> bool {
        self.sent_context.as_ref().is_some_and(|c| {
            c.analysis
                .as_ref()
                .is_some_and(|a| a.analyzed_packets == 10_000)
                && c.validate_references(&self.answer).is_ok()
        }) && self.answer.contains("Demonstração")
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_report(&mut self, directory: &std::path::Path, cx: &mut Context<Self>) {
        let original = self.report_draft(true, cx).unwrap();
        self.question.update(cx, |q, cx| {
            q.set_text("Pergunta alterada depois do envio", cx)
        });
        let snapshot = self.report_draft(true, cx).unwrap();
        assert_eq!(snapshot.question, original.question);
        assert_eq!(snapshot.context, original.context);
        assert!(
            snapshot
                .response
                .as_ref()
                .unwrap()
                .provider
                .contains("simulado")
        );
        let receipt = snapshot.save(&directory.join("assisted")).unwrap();
        assert!(receipt.references_checked);
        assert!(self.report_draft(false, cx).is_err());
        self.review(cx);
        let local = self.report_draft(false, cx).unwrap();
        assert!(local.response.is_none());
        assert_eq!(local.question, "Pergunta alterada depois do envio");
        local.save(&directory.join("local")).unwrap();
        self.status="Prova: rascunhos Markdown/HTML e hashes salvos. A resposta conserva o contexto/pergunta do envio; o relatório local usa o contexto revisado atual.".into();
        cx.notify();
    }
}
