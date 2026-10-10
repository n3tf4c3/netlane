//! Reviewed capture context, bounded Codex conversation and explicit authentication.
use crate::{
    text_input::TextInput,
    theme::{ActiveTheme, MONO_FONT},
};
use gpui::{
    App, ClipboardItem, Context, Entity, EventEmitter, FontWeight, Render, Subscription, Window,
    div, prelude::*, px,
};
use netlane_ai::{AgentHandle, Command, Event as AgentEvent, ModelChoice, ModelOption};
use netlane_ai::{
    Context as EvidenceContext,
    codex::{Probe, ProbeState},
};
use std::{path::PathBuf, time::Duration};
pub struct ContextPrepared;
pub struct CodexView {
    demo: bool,
    profile: PathBuf,
    evidence: Option<EvidenceContext>,
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
    conversation: u64,
    answering: bool,
    answer: String,
    sent_context: Option<EvidenceContext>,
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<ContextPrepared> for CodexView {}
impl CodexView {
    pub fn new(demo: bool, profile: PathBuf, cx: &mut Context<Self>) -> Self {
        let problem =
            cx.new(|cx| TextInput::new("", "Descreva o problema observado neste computador", cx));
        let question = cx.new(|cx| {
            TextInput::new(
                "Quais fatos, hipóteses e próximos passos estes pacotes permitem levantar?",
                "Pergunta para o Codex",
                cx,
            )
        });
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
        let subscriptions = [&problem, &question]
            .into_iter()
            .map(|input| {
                cx.subscribe(input, |this, _, _, cx| {
                    this.preview = None;
                    cx.notify();
                })
            })
            .collect();
        Self {
            demo,
            profile,
            evidence: None,
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
            conversation: 1,
            answering: false,
            answer: String::new(),
            sent_context: None,
            _subscriptions: subscriptions,
        }
    }
    pub fn prepare(&mut self, mut context: EvidenceContext, cx: &mut Context<Self>) {
        if self.answering {
            self.status="A resposta pertence à análise atual. Cancele ou aguarde antes de trocar as evidências.".into();
            cx.emit(ContextPrepared);
            cx.notify();
            return;
        }
        if let Some(previous) = &self.evidence
            && previous.sha256 == context.sha256
        {
            context.investigation_id = previous.investigation_id.clone();
            for packet in &previous.packets {
                if !context.packets.iter().any(|p| p.number == packet.number) {
                    context.packets.push(packet.clone());
                }
            }
            context.packets.sort_by_key(|p| p.number);
        }
        match context.validate() {
            Ok(()) => {
                if self
                    .evidence
                    .as_ref()
                    .is_some_and(|prior| prior.sha256 != context.sha256)
                {
                    self.reset_conversation();
                }
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
        if self.answering {
            self.status = "Cancele ou aguarde a resposta antes de abrir outra análise.".into();
            cx.notify();
            return;
        }
        self.reset_conversation();
        self.evidence = None;
        self.preview = None;
        self.problem.update(cx, |f, cx| f.set_text("", cx));
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
        self.answer.clear();
        self.sent_context = None;
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
                            if disconnected {
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
                self.agent_ready = false;
                self.answering = false;
                self.answer.clear();
                self.agent_status = "Conta desconectada do perfil NetLane.".into();
            }
            AgentEvent::Ready { agent } => {
                self.agent_ready = true;
                self.agent_status = agent;
            }
            AgentEvent::Models { options, selected } => {
                self.models = options;
                self.model = Some(selected);
            }
            AgentEvent::Text { conversation, text }
                if self.answering && conversation == self.conversation =>
            {
                if self.answer.len() + text.len() <= 128 * 1024 {
                    self.answer.push_str(&text);
                }
            }
            AgentEvent::Done { conversation, note } if conversation == self.conversation => {
                self.answering = false;
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
                self.answering = false;
                self.status = message;
            }
            AgentEvent::Blocked { conversation, what } if conversation == self.conversation => {
                self.status = format!("Capacidade recusada: {what}. Encerrando o agente.");
            }
            AgentEvent::Closed { message } => {
                self.agent = None;
                self.agent_events = None;
                self.agent_ready = false;
                self.answering = false;
                self.agent_status = message;
            }
            _ => {}
        }
        cx.notify();
    }
    fn send(&mut self, cx: &mut Context<Self>) {
        if self.answering {
            return;
        }
        let Some(context) = &self.evidence else {
            return;
        };
        let Ok(prompt) =
            context.prompt(self.problem.read(cx).text(), self.question.read(cx).text())
        else {
            self.status = "Confira problema, pergunta e limites do contexto.".into();
            cx.notify();
            return;
        };
        if self.preview.as_ref() != Some(&prompt) {
            self.status = "Revise o contexto atualizado antes de enviar.".into();
            cx.notify();
            return;
        }
        if !self.agent_ready {
            self.status = "Conecte o Codex e entre com ChatGPT antes de enviar.".into();
            cx.notify();
            return;
        }
        self.sent_context = Some(context.clone());
        self.answer.clear();
        if self.demo {
            self.preview = None;
            let number = context.packets[0].number;
            let hash = context.sha256.clone();
            self.answering = true;
            self.agent_event(AgentEvent::Text {conversation:self.conversation,text:format!("Demonstração de análise, sem provedor real. O resumo local cobre {} pacotes; os pacotes anexados são amostras. Referência verificável: [[packet:{hash}:{number}]]. Diferenças em relação ao baseline são indícios e exigem conferir filtro, período e ponto de coleta.",context.inspected_packets)},cx);
            self.agent_event(
                AgentEvent::Done {
                    conversation: self.conversation,
                    note: Some("Resposta fictícia. Nenhuma inferência enviada.".into()),
                },
                cx,
            );
        } else if let Some(agent) = &self.agent {
            match agent.send(Command::Prompt {
                conversation: self.conversation,
                text: prompt,
            }) {
                Ok(()) => {
                    self.preview = None;
                    self.answering = true;
                    self.status = "Analisando o contexto revisado…".into();
                }
                Err(error) => self.status = error.to_string(),
            }
        }
        cx.notify();
    }
    fn cycle_model(&mut self, cx: &mut Context<Self>) {
        if self.answering || self.models.is_empty() {
            return;
        }
        let next = self
            .model
            .as_ref()
            .and_then(|m| self.models.iter().position(|option| option.id == m.model))
            .map_or(0, |index| (index + 1) % self.models.len());
        let selected = ModelChoice {
            model: self.models[next].id.clone(),
            effort: "medium".into(),
        };
        self.agent_command(Command::SetModel(selected), cx);
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
    fn button(id: &'static str, label: &str, cx: &App) -> gpui::Stateful<gpui::Div> {
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
}
impl Render for CodexView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme().clone();
        let mut view = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .min_w_0()
            .child(
                div()
                    .text_size(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Análise com Codex"),
            )
            .child(div().text_color(t.text_muted).child(
                "Revise as evidências da captura e converse com Codex sobre fatos, hipóteses e referências.",
            ))
            .child(div().text_color(t.accent).child(self.status.clone()));
        if let Some(context) = &self.evidence {
            view=view.child(div().p(px(14.)).bg(t.panel).rounded(px(6.)).flex().flex_col().gap(px(6.))
                .child(format!("Selecionados: {} · {} · Pacotes no arquivo: {}",context.packets.len(),context.format,context.total_packets))
                .child(format!("Pacotes: {}",context.packets.iter().map(|p|format!("#{}",p.number)).collect::<Vec<_>>().join(", ")))
                .child(div().font_family(MONO_FONT).text_size(px(11.)).child(format!("SHA-256 {}",context.sha256)))
                .child(div().text_color(t.text_muted).child("Inclui endereços, portas, horários e resumos. Bytes dos pacotes e caminhos de arquivos ficam fora do contexto.")))
                .child(div().child("Problema observado").child(self.problem.clone()))
                .child(div().child("Pergunta para o Codex").child(self.question.clone()))
                .child(div().flex().flex_wrap().gap(px(8.))
                    .child(Self::button("codex-review","Revisar contexto",cx).on_click(cx.listener(|this,_,_,cx|this.review(cx))))
                    .child(Self::button("codex-copy","Copiar para Codex",cx).on_click(cx.listener(|this,_,_,cx|this.copy(cx))))
                    .child(Self::button("codex-send",if self.answering {"Analisando…"}else if self.demo {"Simular análise"}else{"Enviar contexto revisado"},cx).on_click(cx.listener(|this,_,_,cx|this.send(cx))))
                    .when(self.answering,|d|d.child(Self::button("codex-interrupt","Interromper resposta",cx).on_click(cx.listener(|this,_,_,cx|{this.agent_command(Command::Cancel {conversation:this.conversation},cx);this.status="Interrupção solicitada. Aguardando encerramento do turno…".into();cx.notify();}))))
                    .child(Self::button("codex-reset","Nova análise",cx).on_click(cx.listener(|this,_,_,cx|this.reset(cx)))));
        } else {
            view=view.child(div().p(px(20.)).bg(t.panel).rounded(px(6.)).child("Em Pacotes, selecione uma linha e clique Preparar para Codex. Você pode juntar até 16 pacotes da mesma captura."));
        }
        if let Some(preview) = &self.preview {
            view = view.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(format!("Contexto para revisão · {} bytes", preview.len()))
                    .child(
                        div()
                            .id("codex-context-scroll")
                            .max_h(px(260.))
                            .overflow_y_scroll()
                            .p(px(14.))
                            .bg(t.panel)
                            .rounded(px(6.))
                            .font_family(MONO_FONT)
                            .text_size(px(12.))
                            .child(preview.clone()),
                    ),
            );
        }
        view=view.child(div().p(px(14.)).bg(t.panel).rounded(px(6.)).flex().flex_col().gap(px(8.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Codex integrado"))
            .child(self.agent_status.clone())
            .child(div().flex().flex_wrap().gap(px(8.))
                .when(self.agent.is_none(),|d|d.child(Self::button("codex-connect",if self.demo {"Simular conexão"}else{"Conectar Codex"},cx).on_click(cx.listener(|this,_,_,cx|this.connect_agent(cx)))))
                .when(self.agent.is_some()&&!self.agent_ready,|d|d
                    .child(Self::button("codex-login","Entrar com ChatGPT",cx).on_click(cx.listener(|this,_,_,cx|this.agent_command(Command::Login,cx))))
                    .child(Self::button("codex-login-cancel","Cancelar login",cx).on_click(cx.listener(|this,_,_,cx|this.agent_command(Command::CancelLogin,cx)))))
                .when(self.agent.is_some()&&self.agent_ready&&!self.answering,|d|d.child(Self::button("codex-logout","Sair da conta",cx).on_click(cx.listener(|this,_,_,cx|this.agent_command(Command::Logout,cx)))))
                .when(self.agent_ready&&!self.models.is_empty(),|d|d.child(Self::button("codex-model",&format!("Modelo: {}",self.model.as_ref().map_or("—",|m|m.model.as_str())),cx).on_click(cx.listener(|this,_,_,cx|this.cycle_model(cx))))))
            .child(div().text_size(px(11.)).text_color(t.text_muted).child("Perfil próprio, catálogo sem ferramentas e conversa em memória. Enviar usa sua conta; o acesso a cada modelo é confirmado pelo provedor.")));
        if !self.answer.is_empty() {
            view=view.child(div().flex().flex_col().gap(px(8.))
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Resposta à pergunta atual"))
                .child(div().id("codex-answer-scroll").max_h(px(260.)).overflow_y_scroll().p(px(14.)).bg(t.panel).rounded(px(6.)).child(self.visible_answer().to_owned()))
                .child(div().text_size(px(11.)).text_color(t.text_muted).child(format!("{} bytes na resposta · visor limitado aos últimos 16 KiB; a cópia inclui a resposta completa.",self.answer.len())))
                .child(Self::button("codex-copy-answer","Copiar resposta",cx).on_click(cx.listener(|this,_,_,cx|{cx.write_to_clipboard(ClipboardItem::new_string(this.answer.clone()));this.status="Resposta copiada. Revise as referências antes de usar em relatório.".into();cx.notify();}))));
        }
        let busy = self.probe.as_ref().is_some_and(|p| !p.state().done);
        view.child(div().p(px(16.)).bg(t.panel).rounded(px(6.)).flex().flex_col().gap(px(10.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Conexão do Codex"))
            .child("Verificação independente de metadados. Consulta somente configuração, conta e catálogo, sem iniciar inferência.")
            .child(div().flex().flex_wrap().gap(px(8.))
                .child(Self::button("codex-check",if self.demo {"Simular verificação"} else {"Verificar conexão"},cx).on_click(cx.listener(|this,_,_,cx|this.check_connection(cx))))
                .when(busy,|d|d.child(Self::button("codex-cancel-probe","Cancelar verificação",cx).on_click(cx.listener(|this,_,_,cx|{if let Some(probe)=&this.probe {probe.cancel();this.probe_state.detail="Encerrando verificação…".into();cx.notify();}}))))
                .when(!self.demo,|d|d.child(Self::button("codex-exe","Editar caminho",cx).on_click(cx.listener(|this,_,_,cx|{this.edit_executable = !this.edit_executable;cx.notify();})))))
            .when(self.edit_executable,|d|d.child(self.executable.clone()))
            .child(self.probe_state.detail.clone())
            .when(self.probe_state.connected&&!self.demo,|d|d.child(format!("{} modelos no catálogo do CLI · acesso de conta ainda precisa ser confirmado.",self.probe_state.models.len()))))
    }
}
