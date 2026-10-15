use super::*;
use anyhow::{Result, ensure};
use netlane_ai::investigation::Notebook;

impl CodexView {
    pub(super) fn can_open_notebook(&self, cx: &App) -> Result<()> {
        ensure!(
            !self.answering && !self.dialog.load(Ordering::Relaxed),
            "Aguarde a resposta ou a operação atual antes de abrir um caderno."
        );
        ensure!(
            self.notebook_revision == self.saved_revision
                && self.note_input.read(cx).text().trim().is_empty(),
            "Salve o caderno e registre ou limpe a anotação em edição antes de abrir outra investigação. Para descartar, use Nova investigação."
        );
        Ok(())
    }

    pub(super) fn apply_notebook(
        &mut self,
        notebook: Notebook,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.can_open_notebook(cx)?;
        notebook.validate()?;
        let conversation = notebook.next_conversation(self.conversation)?;
        let last = notebook.turns().last().cloned();
        let context = last
            .as_ref()
            .map(|turn| turn.draft.context.clone())
            .or_else(|| notebook.notes().last().map(|note| note.context.clone()));
        self.reset_conversation();
        self.conversation = conversation;
        self.conversation_capture = context.as_ref().map(|c| c.sha256.clone());
        self.evidence = context;
        self.notebook = notebook;
        // Session-only free questions belong to the investigation being replaced.
        self.free_turns.clear();
        self.order.clear();
        self.following = false;
        self.sources.clear();
        self.bundle_saved = None;
        self.notebook_revision = self.notebook_revision.saturating_add(1);
        self.saved_revision = self.notebook_revision;
        self.note_context = None;
        self.last_turn = last.as_ref().map(|turn| turn.id);
        self.sent_context = last.as_ref().map(|turn| turn.draft.context.clone());
        self.sent_report = last.as_ref().map(|turn| turn.draft.clone());
        self.answer = last
            .as_ref()
            .and_then(|turn| turn.draft.response.as_ref())
            .map_or_else(String::new, |response| response.text.clone());
        self.problem.update(cx, |field, cx| {
            field.set_text(
                last.as_ref().map_or("", |turn| turn.draft.problem.as_str()),
                cx,
            )
        });
        self.question.update(cx, |field, cx| {
            field.set_text(
                "Como podemos retomar a investigação e qual evidência falta conferir?",
                cx,
            )
        });
        self.preview = None;
        self.notes_open = self.notebook.turns().is_empty();
        self.confirm_new = false;
        self.history_page = None;
        self.chat_scroll.scroll_to_bottom();
        self.status = format!(
            "Caderno aberto: {} turnos e {} anotações. O próximo envio usa uma nova conversa do provedor. Prepare um resumo e revise o contexto para retomar; estados e textos importados exigem sua revisão. O dump da tabela permanece separado.",
            self.notebook.turns().len(),
            self.notebook.notes().len()
        );
        cx.emit(NotebookOpened);
        cx.notify();
        Ok(())
    }

    pub(super) fn open_notebook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.can_open_notebook(cx) {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        let revision = self.notebook_revision;
        let question = self.question.read(cx).text().to_owned();
        let problem = self.problem.read(cx).text().to_owned();
        let evidence = self.evidence.clone();
        self.dialog.store(true, Ordering::Relaxed);
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                "Abrir a pasta do caderno com investigation.json, investigation.md e receipt.json"
                    .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match paths.await { Ok(Ok(Some(paths))) => paths.into_iter().next(), _ => None };
            let Some(path) = path else { let _ = this.update(cx, |this, cx| { this.dialog.store(false, Ordering::Relaxed); cx.notify(); }); return; };
            let _ = this.update(cx, |this, cx| { this.status = "Conferindo arquivos e limites do caderno…".into(); cx.notify(); });
            let result = cx.background_executor().spawn(async move { Notebook::load(&path) }).await;
            let _ = this.update(cx, |this, cx| {
                this.dialog.store(false, Ordering::Relaxed);
                if this.notebook_revision != revision || this.question.read(cx).text() != question || this.problem.read(cx).text() != problem || this.evidence != evidence {
                    this.status = "A investigação mudou durante a leitura. A abertura foi cancelada; seu trabalho atual foi preservado.".into();
                } else {
                    match result.and_then(|notebook| this.apply_notebook(notebook, cx)) {
                        Ok(()) => {},
                        Err(error) => this.status = format!("Caderno não aberto: {error:#}. A investigação atual foi preservada."),
                    }
                }
                cx.notify();
            });
        }).detach();
    }

    pub(super) fn prepare_resume(&mut self, cx: &mut Context<Self>) {
        if self.answering || self.dialog.load(Ordering::Relaxed) {
            self.status =
                "Aguarde a resposta ou a operação atual antes de preparar o resumo.".into();
            cx.notify();
            return;
        }
        let result = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Selecione as evidências da captura para retomar."))
            .and_then(|context| {
                self.notebook
                    .resume_question(context, self.question.read(cx).text())
            });
        match result {
            Ok(question) => {
                self.suggest(&question, cx);
                self.notes_open = false;
                self.status = "Resumo local preparado no campo de pergunta, com trechos desta captura. Revise e edite antes de enviar; o histórico completo não é anexado automaticamente.".into();
            }
            Err(error) => self.status = error.to_string(),
        }
        cx.notify();
    }

    #[cfg(feature = "gui-test")]
    pub fn proof_open_resume(&mut self, source: &std::path::Path, cx: &mut Context<Self>) {
        let snapshot = serde_json::to_value(&self.notebook).unwrap();
        let imported = Notebook::load(source).unwrap();
        let imported_snapshot = serde_json::to_value(&imported).unwrap();
        let next = imported.next_conversation(self.conversation).unwrap();
        self.notebook_revision += 1;
        assert!(self.apply_notebook(imported.clone(), cx).is_err());
        assert_eq!(serde_json::to_value(&self.notebook).unwrap(), snapshot);
        self.notebook_revision -= 1;
        self.note_input
            .update(cx, |field, cx| field.set_text("rascunho pendente", cx));
        assert!(self.apply_notebook(imported.clone(), cx).is_err());
        self.note_input
            .update(cx, |field, cx| field.set_text("", cx));
        assert!(self.apply_notebook(Notebook::default(), cx).is_err());
        assert_eq!(serde_json::to_value(&self.notebook).unwrap(), snapshot);
        self.dialog.store(true, Ordering::Relaxed);
        assert!(!self.allow_close(cx));
        let count = self.notebook.turns().len();
        self.send(cx);
        assert_eq!(self.notebook.turns().len(), count);
        self.dialog.store(false, Ordering::Relaxed);
        self.apply_notebook(imported, cx).unwrap();
        assert_eq!(self.conversation, next);
        assert_eq!(
            serde_json::to_value(&self.notebook).unwrap(),
            imported_snapshot
        );
        assert!(self.preview.is_none() && !self.answering);
        self.suggest("Quais evidências ainda precisamos conferir?", cx);
        self.prepare_resume(cx);
        assert_eq!(self.notebook.turns().len(), count);
        assert!(self.preview.is_none());
        assert!(self.question.read(cx).text().contains("Resumo local"));
        self.send(cx);
        assert_eq!(self.notebook.turns().len(), count);
        assert!(self.status.contains("Revise o contexto"));
        self.review(cx);
        self.status = "Caderno reaberto e resumo local preparado. Nenhum envio ao abrir; o próximo turno começa outra conversa. Revise o texto completo abaixo para retomar.".into();
        cx.notify();
    }

    #[cfg(feature = "gui-test")]
    pub fn proof_resume_send(&mut self, directory: &std::path::Path, cx: &mut Context<Self>) {
        let old = serde_json::to_value(self.notebook.turns()).unwrap();
        let count = self.notebook.turns().len();
        self.connect_agent(cx);
        self.send(cx);
        assert_eq!(self.notebook.turns().len(), count + 1);
        assert_eq!(
            self.notebook.turns().last().unwrap().conversation,
            self.conversation
        );
        assert_eq!(
            serde_json::to_value(&self.notebook.turns()[..count]).unwrap(),
            old
        );
        assert!(
            self.notebook
                .turns()
                .last()
                .unwrap()
                .draft
                .question
                .contains("Resumo local")
        );
        self.notebook.cycle_note(1).unwrap();
        self.notebook.cycle_note(1).unwrap();
        self.notebook_revision += 2;
        assert_eq!(self.notebook.notes()[0].state_history.len(), 3);
        assert!(!self.allow_close(cx));
        self.notebook.save(directory).unwrap();
        self.saved_revision = self.notebook_revision;
        assert!(self.allow_close(cx));
        self.notes_open = true;
        self.status = "Retomada fictícia verificada: contextos antigos preservados, nova conversa identificada e histórico de estados salvo. Nenhuma conta, inferência ou captura real.".into();
        cx.notify();
    }
}
