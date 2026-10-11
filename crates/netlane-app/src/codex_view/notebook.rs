use super::*;
use netlane_ai::investigation::MAX_NOTE_BYTES;

fn excerpt(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
impl CodexView {
    pub(super) fn follow_answer(&self) {
        if self.history_page.is_none()
            && self.chat_scroll.max_offset().y + self.chat_scroll.offset().y <= px(32.)
        {
            self.chat_scroll.scroll_to_bottom();
        }
    }
    pub fn allow_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.dialog.load(Ordering::Relaxed) {
            self.status = "Aguarde a abertura ou gravação do caderno antes de sair.".into();
        } else if self.answering {
            self.status = "Interrompa ou aguarde a resposta antes de sair da investigação.".into();
        } else if self.notebook_revision != self.saved_revision
            || !self.note_input.read(cx).text().trim().is_empty()
            || self
                .bundle_saved
                .as_ref()
                .is_some_and(|saved| saved != &self.session_stamp(cx))
        {
            self.status="Antes de sair, registre a anotação em edição e salve a investigação. Para descartar, use Nova investigação e confirme.".into();
        } else {
            return true;
        }
        cx.notify();
        false
    }
    pub(super) fn finish_turn(&mut self, status: &str) {
        if !self.answering {
            return;
        }
        self.follow_answer();
        self.answering = false;
        if let Some(draft) = &mut self.sent_report {
            if let Some(response) = &mut draft.response {
                response.status = status.into();
                response.text = self.answer.clone();
            }
            match self.notebook.record(self.conversation, draft.clone()) {
                Ok(id) => {
                    self.last_turn = Some(id);
                    self.notebook_revision += 1;
                }
                Err(e) => self.status = format!("Não foi possível registrar o turno: {e}"),
            }
        }
    }
    pub(super) fn suggest(&mut self, question: &str, cx: &mut Context<Self>) {
        self.question
            .update(cx, |input, cx| input.set_text(question, cx));
        self.preview = None;
        self.status = "Pergunta preparada. Revise o contexto e envie quando estiver pronto.".into();
        cx.notify();
    }
    fn note_from_turn(&mut self, id: usize, cx: &mut Context<Self>) {
        if !self.note_input.read(cx).text().trim().is_empty() {
            self.notes_open = true;
            self.status =
                "Registre ou limpe a anotação em edição antes de anotar outra resposta.".into();
            cx.notify();
            return;
        }
        if let Some(turn) = self.notebook.turns().iter().find(|t| t.id == id) {
            let text = turn.draft.response.as_ref().map_or("", |r| r.text.as_str());
            self.note_input
                .update(cx, |f, cx| f.set_text(excerpt(text, MAX_NOTE_BYTES), cx));
            self.note_context = Some((turn.draft.context.clone(), Some(id)));
            self.notes_open = true;
            self.status="Resposta colocada na anotação (até 4 KiB). Edite a hipótese e clique Registrar anotação.".into();
        }
        cx.notify();
    }
    pub(super) fn add_note(&mut self, cx: &mut Context<Self>) {
        let context = self
            .note_context
            .clone()
            .or_else(|| self.evidence.clone().map(|c| (c, None)));
        let Some((context, source)) = context else {
            self.status = "Prepare evidências para vincular a anotação.".into();
            cx.notify();
            return;
        };
        match self
            .notebook
            .add_note(self.note_input.read(cx).text(), context, source)
        {
            Ok(id) => {
                self.notebook_revision += 1;
                self.note_input.update(cx, |f, cx| f.set_text("", cx));
                self.note_context = None;
                self.status = format!(
                    "Anotação #{id} registrada como em investigação. A confirmação depende da sua revisão."
                );
            }
            Err(e) => self.status = e.to_string(),
        }
        cx.notify();
    }
    fn discuss_note(&mut self, id: usize, cx: &mut Context<Self>) {
        if self.answering {
            self.status = "Aguarde ou interrompa o turno antes de mudar as evidências.".into();
            cx.notify();
            return;
        }
        if let Some(note) = self.notebook.notes().iter().find(|n| n.id == id).cloned() {
            self.prepare_selection(note.context, true, cx);
            self.notes_open = false;
            self.suggest(&format!("Vamos investigar esta anotação ({}; trecho, não um fato comprovado): {}. Que evidências confirmam ou refutam essa hipótese e qual seria a próxima verificação?",note.state.label(),excerpt(&note.text,3500)),cx);
            self.status = format!(
                "Evidências da anotação #{id} restauradas no chat. Revise antes de enviar; o dump aberto na tabela não foi trocado."
            );
            cx.emit(ContextPrepared);
        }
        cx.notify();
    }
    pub(super) fn save_notebook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.load(Ordering::Relaxed) || self.answering {
            self.status = "Aguarde o turno ou a gravação atual antes de salvar o caderno.".into();
            cx.notify();
            return;
        }
        if !self.note_input.read(cx).text().trim().is_empty() {
            self.notes_open = true;
            self.status =
                "Registre ou limpe a anotação em edição antes de salvar o caderno.".into();
            cx.notify();
            return;
        }
        if self.notebook.is_empty() {
            self.status = "Registre uma anotação ou converse com Codex antes de salvar.".into();
            cx.notify();
            return;
        }
        let snapshot = self.notebook.clone();
        let revision = self.notebook_revision;
        self.dialog.store(true, Ordering::Relaxed);
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Salvar conversa e anotações em uma nova pasta de investigação".into()),
        });
        cx.spawn_in(window,async move|this,cx|{
            let parent=match paths.await {Ok(Ok(Some(paths)))=>paths.into_iter().next(),_=>None};
            let Some(parent)=parent else {let _=this.update(cx,|this,cx|{this.dialog.store(false,Ordering::Relaxed);cx.notify();});return;};
            let directory=parent.join(format!("NetLane-investigacao-{}",uuid::Uuid::new_v4()));let destination=directory.clone();
            let result=cx.background_executor().spawn(async move{snapshot.save(&destination)}).await;
            let _=this.update(cx,|this,cx|{
                this.dialog.store(false,Ordering::Relaxed);
                match result {Ok(_)=>{this.saved_revision=revision;this.status=format!("Conversa, anotações, contextos e hashes salvos em {}. Revise antes de compartilhar.",directory.display());},Err(e)=>this.status=format!("Caderno não concluído: {e:#}. Eventuais arquivos parciais foram preservados.")};
                cx.notify();
            });
        }).detach();
    }
    pub(super) fn render_chat(&self, height: f32, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let count = self.notebook.turns().len();
        let start = self
            .history_page
            .unwrap_or(count.saturating_sub(4))
            .min(count.saturating_sub(1));
        let mut history = div()
            .id("investigation-chat-scroll")
            .h(px(height))
            .overflow_y_scroll()
            .track_scroll(&self.chat_scroll)
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(10.))
            .bg(t.editor)
            .rounded(px(6.));
        for turn in self.notebook.turns().iter().skip(start).take(4) {
            let id = turn.id;
            let answer = turn.draft.response.as_ref();
            history=history.child(div().flex_none().flex().flex_col().gap(px(6.)).p(px(10.)).bg(t.panel).rounded(px(5.))
                .child(div().text_size(px(11.)).text_color(t.text_muted).child(format!("Turno #{id} · conversa {} · captura {}… · {}",turn.conversation,&turn.draft.context.sha256[..12], turn.created_unix_seconds.map_or("horário não registrado".into(), netlane_ai::investigation::registration_label))))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(t.accent).child("Você"))
                .child(excerpt(&turn.draft.question,4096).to_owned())
                .child(div().font_weight(FontWeight::SEMIBOLD).child(answer.map_or("Codex",|r|r.provider.as_str()).to_owned()))
                .child(excerpt(answer.map_or("",|r|r.text.as_str()),4096).to_owned())
                .child(div().text_size(px(11.)).text_color(if turn.references_valid {t.text_muted}else{t.warning}).child(if turn.references_valid {answer.map_or("",|r|r.status.as_str())}else{"Referência fora deste contexto. Resposta conservada como registro; confira as evidências."}.to_owned()))
                .child(div().flex().flex_wrap().gap(px(6.))
                    .child(Self::button(("turn-note",id),"Anotar resposta",cx).on_click(cx.listener(move|this,_,_,cx|this.note_from_turn(id,cx))))
                    .child(Self::button(("turn-copy",id),"Copiar completa",cx).on_click(cx.listener(move|this,_,_,cx|{
                        if let Some(response)=this.notebook.turns().iter().find(|t|t.id==id).and_then(|t|t.draft.response.as_ref()) {cx.write_to_clipboard(ClipboardItem::new_string(response.text.clone()));}
                    })))));
        }
        if self.answering {
            history = history.child(
                div()
                    .flex_none()
                    .p(px(10.))
                    .bg(t.panel)
                    .rounded(px(5.))
                    .child(
                        div()
                            .text_color(t.accent)
                            .child("Você · turno em andamento"),
                    )
                    .child(
                        self.sent_report
                            .as_ref()
                            .map_or("", |d| d.question.as_str())
                            .to_owned(),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Codex está analisando…"),
                    )
                    .child(self.visible_answer().to_owned()),
            );
        }
        if count == 0 && !self.answering {
            history=history.child("Converse sobre o que está vendo. Questione um achado, proponha outra explicação ou peça a próxima verificação.");
        }
        div().flex().flex_col().gap(px(6.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Chat da investigação"))
            .child(history)
            .child(div().flex().flex_wrap().gap(px(6.))
                .when(start>0,|d|d.child(Self::button("chat-previous","Turnos anteriores",cx).on_click(cx.listener(move|this,_,_,cx|{this.history_page=Some(start.saturating_sub(4));this.chat_scroll.set_offset(Default::default());cx.notify();}))))
                .when(start+4<count,|d|d.child(Self::button("chat-next","Turnos seguintes",cx).on_click(cx.listener(move|this,_,_,cx|{let next=(start+4).min(count.saturating_sub(4));this.history_page=if next+4>=count {None}else{Some(next)};this.chat_scroll.set_offset(Default::default());cx.notify();}))))
                .when(count>0,|d|d.child(Self::button("chat-latest","Última resposta",cx).on_click(cx.listener(|this,_,_,cx|{this.history_page=None;this.chat_scroll.scroll_to_bottom();cx.notify();}))))
                .child(div().text_size(px(11.)).text_color(t.text_muted).child(format!("{count}/24 turnos · até 4 KiB visíveis por resposta; cópia e caderno completos."))))
    }
    pub(super) fn render_notes(&self, cx: &mut Context<Self>) -> gpui::Div {
        let t = cx.theme().clone();
        let context = self
            .note_context
            .as_ref()
            .map(|(c, _)| c)
            .or(self.evidence.as_ref());
        let mut notes =
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .p(px(12.))
                .bg(t.panel)
                .rounded(px(6.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Hipóteses e anotações"),
                )
                .child(div().text_size(px(11.)).text_color(t.text_muted).child(
                    context.map_or_else(
                        || "Prepare evidências para registrar uma hipótese.".into(),
                        |c| {
                            format!(
                                "Nova anotação: captura {}… · pacotes {:?}",
                                &c.sha256[..12],
                                c.packets.iter().map(|p| p.number).collect::<Vec<_>>()
                            )
                        },
                    ),
                ))
                .child(self.note_input.clone())
                .child(
                    Self::button("notebook-add-note", "Registrar anotação", cx)
                        .on_click(cx.listener(|this, _, _, cx| this.add_note(cx))),
                );
        for note in self.notebook.notes() {
            let id = note.id;
            notes =
                notes.child(
                    div()
                        .p(px(8.))
                        .border_1()
                        .border_color(t.border)
                        .rounded(px(4.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_color(t.accent)
                                .child(format!("#{id} · {}", note.state.label())),
                        )
                        .child(excerpt(&note.text, 1024).to_owned())
                        .child(div().text_size(px(11.)).text_color(t.text_muted)
                            .child(if note.state_history.is_empty() { "Histórico de estados não registrado na exportação antiga.".into() } else {
                                let start = note.state_history.len().saturating_sub(3);
                                format!("Histórico · {} registros (últimos 3): {}", note.state_history.len(), note.state_history[start..].iter().map(|entry| format!("{}: {}", netlane_ai::investigation::registration_label(entry.unix_seconds), entry.state.label())).collect::<Vec<_>>().join(" · "))
                            }))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.text_muted)
                                .child(format!(
                                    "Captura {}… · origem {}",
                                    &note.context.sha256[..12],
                                    note.source_turn
                                        .map_or("anotação do usuário".into(), |n| format!(
                                            "turno #{n}"
                                        ))
                                )),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(6.))
                                .child(
                                    Self::button(("note-discuss", id), "Discutir no chat", cx)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.discuss_note(id, cx)
                                        })),
                                )
                                .child(
                                    Self::button(
                                        ("note-state", id),
                                        &format!("Marcar: {}", note.state.next().label()),
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            match this.notebook.cycle_note(id) {
                                                Ok(()) => {
                                                    this.notebook_revision += 1;
                                                    this.status = "Estado atualizado por você e registrado no histórico da anotação.".into();
                                                }
                                                Err(error) => this.status = error.to_string(),
                                            }
                                            cx.notify();
                                        },
                                    )),
                                ),
                        ),
                );
        }
        notes
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_notebook_tab(&mut self, cx: &mut Context<Self>) {
        self.notes_open = true;
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_recovery(&mut self, directory: &std::path::Path, cx: &mut Context<Self>) {
        let turns = self.notebook.turns().to_vec();
        let notes = self.notebook.notes().to_vec();
        let turns_snapshot = serde_json::to_value(&turns).unwrap();
        let notes_snapshot = serde_json::to_value(&notes).unwrap();
        let conversation = self.conversation;
        self.agent_event(AgentEvent::LoggedOut, cx);
        assert_eq!(self.conversation, conversation + 1);
        assert_eq!(
            serde_json::to_value(self.notebook.turns()).unwrap(),
            turns_snapshot
        );
        assert_eq!(
            serde_json::to_value(self.notebook.notes()).unwrap(),
            notes_snapshot
        );
        assert!(!self.agent_ready && self.preview.is_none());
        self.connect_agent(cx);
        self.suggest(
            "Recomeçando a conversa: quais evidências precisamos rever?",
            cx,
        );
        self.review(cx);
        self.send(cx);
        assert_eq!(
            self.notebook.turns().last().unwrap().conversation,
            conversation + 1
        );
        assert_eq!(
            serde_json::to_value(&self.notebook.turns()[..turns.len()]).unwrap(),
            turns_snapshot
        );
        let completed = self.notebook.turns().len();
        self.agent_event(
            AgentEvent::Closed {
                message: "Desconexão sintética.".into(),
            },
            cx,
        );
        assert_eq!(self.conversation, conversation + 2);
        self.agent_event(
            AgentEvent::Text {
                conversation: conversation + 1,
                text: "obsolete delta".into(),
            },
            cx,
        );
        assert!(!self.answer.contains("obsolete delta"));

        let original = self.evidence.clone().unwrap();
        let mut saturated = original.clone();
        saturated.packets = (1..=16)
            .map(|number| netlane_ai::PacketEvidence {
                number,
                ..original.packets[0].clone()
            })
            .collect();
        self.prepare_selection(saturated.clone(), true, cx);
        let mut next = original.clone();
        next.packets = vec![netlane_ai::PacketEvidence {
            number: 17,
            ..original.packets[0].clone()
        }];
        self.prepare(next.clone(), cx);
        assert_eq!(self.evidence.as_ref().unwrap(), &saturated);
        assert!(self.status.contains("16 pacotes"));
        self.prepare_selection(next, true, cx);
        assert_eq!(self.evidence.as_ref().unwrap().packets[0].number, 17);
        self.remove_packet(17, cx);
        assert!(self.evidence.is_none());
        assert_eq!(self.notebook.turns().len(), completed);
        assert_eq!(
            serde_json::to_value(self.notebook.notes()).unwrap(),
            notes_snapshot
        );
        let before_switch = self.conversation;
        let mut foreign = original.clone();
        foreign.sha256 = "D".repeat(64);
        self.prepare_selection(foreign, true, cx);
        assert_eq!(self.conversation, before_switch + 1);
        self.prepare_selection(original, true, cx);
        assert_eq!(self.conversation, before_switch + 2);
        assert_eq!(self.notebook.turns().len(), completed);

        self.suggest("Pergunta preservada enquanto o modelo é confirmado", cx);
        self.review(cx);
        self.demo = false;
        self.model_pending = true;
        self.send(cx);
        assert_eq!(self.notebook.turns().len(), completed);
        assert!(
            self.question
                .read(cx)
                .text()
                .contains("Pergunta preservada")
        );
        assert!(self.status.contains("confirmação do modelo"));
        self.agent_event(
            AgentEvent::Models {
                options: Vec::new(),
                selected: ModelChoice {
                    model: "synthetic-model".into(),
                    effort: "medium".into(),
                },
            },
            cx,
        );
        assert!(!self.model_pending);
        self.demo = true;
        self.notes_open = false;
        assert!(self.notebook.save(directory).unwrap().turns > turns.len());
        self.saved_revision = self.notebook_revision;
        assert!(self.allow_close(cx));
        self.status = "Reconexão fictícia: nova conversa do Codex identificada; histórico, anotações e perguntas preservados. Seleção substituída e removida sem apagar o caderno. Nenhum envio real.".into();
        cx.notify();
    }
    #[cfg(feature = "gui-test")]
    pub fn proof_interactive(&mut self, directory: &std::path::Path, cx: &mut Context<Self>) {
        let first = self.notebook.turns().last().unwrap().clone();
        self.suggest(
            "Que evidência confirmaria ou refutaria a hipótese de recusa de conexão?",
            cx,
        );
        self.review(cx);
        self.send(cx);
        assert!(self.notebook.turns().len() >= 2);
        assert!(
            self.notebook
                .turns()
                .iter()
                .any(|t| t.id == first.id && t.draft.question == first.draft.question)
        );
        self.note_from_turn(self.last_turn.unwrap(), cx);
        self.note_input.update(cx, |f, cx| {
            f.set_text(
                "Possível recusa de conexão: conferir RST e o período da falha.",
                cx,
            )
        });
        self.add_note(cx);
        assert_eq!(
            self.notebook.notes().last().unwrap().state,
            netlane_ai::investigation::NoteState::Investigating
        );
        let revision = self.notebook_revision;
        assert!(!self.allow_close(cx));
        self.reset(cx);
        assert!(self.confirm_new && self.notebook_revision == revision);
        self.confirm_new = false;
        let receipt = self.notebook.save(directory).unwrap();
        assert!(receipt.turns >= 2 && receipt.notes == 1);
        self.saved_revision = revision;
        assert!(self.allow_close(cx));
        self.discuss_note(1, cx);
        assert!(
            self.question
                .read(cx)
                .text()
                .contains("confirmam ou refutam")
        );
        assert!(self.preview.is_none());
        self.status="Conversa fictícia com perguntas sucessivas, anotação vinculada e caderno exportado. Nenhum envio real.".into();
        cx.notify();
    }
}
