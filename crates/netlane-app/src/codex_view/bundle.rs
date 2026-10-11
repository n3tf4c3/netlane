use super::*;
use anyhow::{Result, ensure};
use netlane_ai::bundle::{self, Inputs, Opened, Source};
use netlane_capture::{
    analysis::{Report, Scan},
    lookup::RecoveredPacket,
};
use std::cell::RefCell;

pub struct Restored {
    pub scan: Scan,
    pub baseline: Option<Report>,
    pub baseline_source: Option<Source>,
    pub recovered: Option<RecoveredPacket>,
    pub number: u64,
    pub provenance: Option<String>,
}
pub struct InvestigationRestored(pub RefCell<Option<Restored>>);
impl EventEmitter<InvestigationRestored> for CodexView {}
impl Drop for CodexView {
    fn drop(&mut self) {
        self.bundle_cancel.store(true, Ordering::Relaxed);
    }
}
impl CodexView {
    pub(super) fn session_stamp(&self, cx: &App) -> (u64, Option<EvidenceContext>, String, String) {
        (
            self.notebook_revision,
            self.evidence.clone(),
            self.problem.read(cx).text().into(),
            self.question.read(cx).text().into(),
        )
    }
    fn register_source(&mut self, source: Source) -> Result<()> {
        if let Some(old) = self
            .sources
            .iter_mut()
            .find(|s| s.sha256.eq_ignore_ascii_case(&source.sha256))
        {
            *old = source;
        } else {
            ensure!(
                self.sources.len() < bundle::MAX_CAPTURES,
                "Fontes demais; salve e inicie outra investigação."
            );
            self.sources.push(source);
        }
        Ok(())
    }
    pub fn prepare_capture(
        &mut self,
        context: EvidenceContext,
        replace: bool,
        source: Option<Source>,
        baseline: Option<Source>,
        cx: &mut Context<Self>,
    ) {
        if self.answering || self.dialog.load(Ordering::Relaxed) {
            return;
        }
        for source in [source, baseline].into_iter().flatten() {
            if let Err(error) = self.register_source(source) {
                self.status = error.to_string();
                cx.notify();
                return;
            }
        }
        self.prepare_selection(context, replace, cx);
    }
    fn start_bundle(&mut self, total: u64, cx: &mut Context<Self>) {
        self.dialog.store(true, Ordering::Relaxed);
        self.bundle_busy = true;
        self.bundle_cancel = Arc::new(AtomicBool::new(false));
        self.bundle_progress = Arc::new(bundle::Progress::default());
        self.bundle_progress.total.store(total, Ordering::Relaxed);
        let progress = self.bundle_progress.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let Ok(done) = this.update(cx, |this, cx| {
                    if !this.bundle_busy || !Arc::ptr_eq(&progress, &this.bundle_progress) {
                        return true;
                    }
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
        cx.notify();
    }
    fn end_bundle(&mut self) {
        self.dialog.store(false, Ordering::Relaxed);
        self.bundle_busy = false;
    }
    fn save_bundle_into(&mut self, directory: PathBuf, cx: &mut Context<Self>) -> Result<()> {
        ensure!(
            !self.answering && !self.dialog.load(Ordering::Relaxed),
            "Aguarde a operação atual antes de salvar."
        );
        ensure!(
            self.note_input.read(cx).text().trim().is_empty(),
            "Registre ou limpe a anotação em edição antes de salvar."
        );
        let current = self.evidence.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Prepare uma captura e registre uma anotação ou conversa antes de salvar com dumps."
            )
        })?;
        let sources = bundle::plan(&self.notebook, &current, &self.sources)?;
        let book = self.notebook.clone();
        let stamp = self.session_stamp(cx);
        let inputs = Inputs {
            problem: stamp.2.clone(),
            question: stamp.3.clone(),
        };
        let total = sources.iter().map(|s| s.bytes).sum::<u64>() * 2;
        self.start_bundle(total, cx);
        self.status = "Copiando e conferindo dumps integrais, caderno e relatórios…".into();
        let cancel = self.bundle_cancel.clone();
        let progress = self.bundle_progress.clone();
        let target = directory.clone();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                bundle::save_with_inputs(&target, &book, &current, &sources, &inputs, &cancel, &progress)
            }).await;
            let _ = this.update(cx, |this, cx| {
                this.end_bundle();
                match result {
                    Ok(manifest) => { this.saved_revision = stamp.0; this.bundle_saved = Some(stamp); this.status = format!("Investigação salva com {} dumps integrais, conversa e relatórios em {}. O pacote contém dados em texto aberto; revise antes de compartilhar.", manifest.captures.len(), directory.display()); }
                    Err(error) => this.status = format!("Investigação não concluída: {error:#}. Parciais preservados em {}; o pacote não foi anunciado como completo.", directory.display()),
                }
                cx.notify();
            });
        }).detach();
        Ok(())
    }
    pub(super) fn save_bundle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.answering || self.dialog.load(Ordering::Relaxed) {
            return;
        }
        let validation = self
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Prepare uma captura antes de salvar com dumps."))
            .and_then(|c| bundle::plan(&self.notebook, c, &self.sources));
        if let Err(error) = validation {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        if !self.note_input.read(cx).text().trim().is_empty() {
            self.status = "Registre ou limpe a anotação em edição antes de salvar.".into();
            cx.notify();
            return;
        }
        self.dialog.store(true, Ordering::Relaxed);
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                "Salvar dumps integrais, conversa e relatórios em uma pasta nova (texto aberto)"
                    .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            let parent = match paths.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let _ = this.update(cx, |this, cx| {
                this.dialog.store(false, Ordering::Relaxed);
                if let Some(parent) = parent
                    && let Err(error) = this.save_bundle_into(
                        parent.join(format!(
                            "NetLane-investigacao-com-capturas-{}",
                            uuid::Uuid::new_v4()
                        )),
                        cx,
                    )
                {
                    this.status = format!("Investigação não salva: {error:#}");
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn can_open_bundle(&self, cx: &App) -> Result<()> {
        self.can_open_notebook(cx)?;
        ensure!(
            !self.capture_active.load(Ordering::Relaxed),
            "Pare a coleta antes de reabrir outra investigação com dumps."
        );
        ensure!(
            self.evidence.is_none() && self.notebook.is_empty()
                || self.bundle_saved.as_ref() == Some(&self.session_stamp(cx)),
            "Salve a investigação com capturas antes de abrir outra. Para descartar o rascunho atual, use Nova investigação."
        );
        Ok(())
    }
    fn apply_bundle(&mut self, opened: Opened, cx: &mut Context<Self>) -> Result<()> {
        self.can_open_bundle(cx)?;
        let baseline_source = opened
            .current
            .analysis
            .as_ref()
            .and_then(|a| a.baseline_sha256.as_ref())
            .and_then(|hash| {
                opened
                    .sources
                    .iter()
                    .find(|s| s.sha256.eq_ignore_ascii_case(hash))
            })
            .cloned();
        let provenance = opened
            .sources
            .iter()
            .find(|s| s.sha256.eq_ignore_ascii_case(&opened.current.sha256))
            .and_then(|s| s.provenance.clone());
        let number = opened.current.packets[0].number;
        self.apply_notebook(opened.notebook, cx)?;
        self.sources = opened.sources;
        self.conversation_capture = Some(opened.current.sha256.clone());
        self.evidence = Some(opened.current);
        self.problem
            .update(cx, |f, cx| f.set_text(opened.inputs.problem, cx));
        self.question
            .update(cx, |f, cx| f.set_text(opened.inputs.question, cx));
        self.preview = None;
        self.bundle_saved = Some(self.session_stamp(cx));
        self.status = format!(
            "Investigação reaberta com {} dumps conferidos; captura e pacote #{number} restaurados. Revise o contexto para continuar em uma nova conversa do provedor.",
            self.sources.len()
        );
        cx.emit(InvestigationRestored(RefCell::new(Some(Restored {
            scan: opened.scan,
            baseline: opened.baseline,
            baseline_source,
            recovered: opened.recovered,
            number,
            provenance,
        }))));
        cx.notify();
        Ok(())
    }
    fn open_bundle_from(&mut self, path: PathBuf, cx: &mut Context<Self>) -> Result<()> {
        self.can_open_bundle(cx)?;
        let stamp = self.session_stamp(cx);
        self.start_bundle(0, cx);
        self.status = "Conferindo caderno, relatórios e dumps antes de reabrir…".into();
        let cancel = self.bundle_cancel.clone();
        let progress = self.bundle_progress.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { bundle::load(&path, &cancel, &progress) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.end_bundle();
                if this.session_stamp(cx) != stamp {
                    this.status =
                        "A investigação mudou durante a leitura; seu trabalho foi preservado."
                            .into();
                } else if let Err(error) = result.and_then(|opened| this.apply_bundle(opened, cx)) {
                    this.status = format!(
                        "Investigação não reaberta: {error:#}. O trabalho atual foi preservado."
                    );
                }
                cx.notify();
            });
        })
        .detach();
        Ok(())
    }
    pub(super) fn open_bundle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.can_open_bundle(cx) {
            self.status = error.to_string();
            cx.notify();
            return;
        }
        self.dialog.store(true, Ordering::Relaxed);
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Abrir investigação com bundle.json, captures, notebook e reports".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match paths.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let _ = this.update(cx, |this, cx| {
                this.dialog.store(false, Ordering::Relaxed);
                if let Some(path) = path
                    && let Err(error) = this.open_bundle_from(path, cx)
                {
                    this.status = format!("Investigação não reaberta: {error:#}");
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn restore_current_dump(&mut self, cx: &mut Context<Self>) {
        if self.answering || self.dialog.load(Ordering::Relaxed) {
            return;
        }
        if self.capture_active.load(Ordering::Relaxed) {
            self.status = "Pare a coleta antes de trocar o dump exibido na tabela.".into();
            cx.notify();
            return;
        }
        let Some(context) = self.evidence.clone() else {
            return;
        };
        let Some(source) = self
            .sources
            .iter()
            .find(|s| s.sha256.eq_ignore_ascii_case(&context.sha256))
            .cloned()
        else {
            self.status = "Dump deste contexto não vinculado. Abra uma investigação com capturas ou prepare o original na tela Pacotes.".into();
            cx.notify();
            return;
        };
        let baseline_source = context
            .analysis
            .as_ref()
            .and_then(|a| a.baseline_sha256.as_ref())
            .and_then(|hash| {
                self.sources
                    .iter()
                    .find(|s| s.sha256.eq_ignore_ascii_case(hash))
            })
            .cloned();
        if context
            .analysis
            .as_ref()
            .is_some_and(|a| a.baseline_sha256.is_some())
            && baseline_source.is_none()
        {
            self.status = "Referência desta seleção indisponível. Prepare também o dump do baseline antes de reabrir.".into();
            cx.notify();
            return;
        }
        let total = source.bytes + baseline_source.as_ref().map_or(0, |s| s.bytes);
        self.start_bundle(total, cx);
        self.status = "Reabrindo o dump da seleção e conferindo as referências…".into();
        let cancel = self.bundle_cancel.clone();
        let progress = self.bundle_progress.clone();
        let stamp = self.session_stamp(cx);
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let scan = bundle::read_source(&source, &cancel, &progress)?;
                let baseline = baseline_source.as_ref().map(|s| bundle::read_source(s, &cancel, &progress).map(|s| s.report)).transpose()?;
                let number = context.packets[0].number;
                let recovered = bundle::recover_selection(&scan, number, &cancel, &progress)?;
                anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Leitura cancelada.");
                Ok::<_, anyhow::Error>(Restored { scan, baseline, baseline_source, recovered, number, provenance: source.provenance })
            }).await;
            let _ = this.update(cx, |this, cx| { this.end_bundle();
                if this.session_stamp(cx) != stamp { this.status = "O contexto mudou durante a leitura; a tabela foi preservada.".into(); }
                else { match result { Ok(ready) => { cx.emit(InvestigationRestored(RefCell::new(Some(ready)))); this.status = "Dump da seleção reaberto e conferido. A conversa e as anotações foram preservadas.".into(); }, Err(error) => this.status = format!("Dump não reaberto: {error:#}. A tabela foi preservada.") } }
                cx.notify();
            });
        }).detach();
    }
}

#[cfg(feature = "gui-test")]
impl CodexView {
    pub fn proof_bundle_reset(&mut self, cx: &mut Context<Self>) {
        self.confirm_new = true;
        self.reset(cx);
        assert!(self.notebook.is_empty() && self.sources.is_empty());
    }
    pub fn proof_bundle_save(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        assert_eq!(self.sources.len(), 2);
        self.proof_analysis(cx);
        assert_eq!(self.notebook.turns().len(), 1);
        self.note_input.update(cx, |f, cx| {
            f.set_text(
                "Hipótese fictícia: conferir o pacote recuperado e o baseline.",
                cx,
            )
        });
        self.add_note(cx);
        self.suggest("Rascunho preservado: qual seria a próxima evidência?", cx);
        self.notes_open = true;
        self.save_bundle_into(path.to_path_buf(), cx).unwrap();
    }
    pub fn proof_bundle_ready(&self, opened: bool) -> bool {
        !self.bundle_busy
            && !self.dialog.load(Ordering::Relaxed)
            && self.status.starts_with(if opened {
                "Investigação reaberta"
            } else {
                "Investigação salva"
            })
    }
    pub fn proof_bundle_open(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        assert!(self.proof_bundle_ready(false), "{}", self.status);
        let stamp = self.session_stamp(cx);
        self.question
            .update(cx, |f, cx| f.set_text("Edição ainda não salva", cx));
        assert!(self.can_open_bundle(cx).is_err());
        assert!(!self.allow_close(cx));
        self.question
            .update(cx, |f, cx| f.set_text(stamp.3.clone(), cx));
        self.capture_active.store(true, Ordering::Relaxed);
        assert!(self.can_open_bundle(cx).is_err());
        self.capture_active.store(false, Ordering::Relaxed);
        self.open_bundle_from(path.to_path_buf(), cx).unwrap();
    }
    pub fn proof_bundle_check(&self, path: &std::path::Path, cx: &App) {
        assert!(self.proof_bundle_ready(true), "{}", self.status);
        assert_eq!(self.notebook.turns().len(), 1);
        assert_eq!(self.notebook.notes().len(), 1);
        assert_eq!(self.sources.len(), 2);
        assert!(self.sources.iter().all(|s| s.path.starts_with(path)));
        assert!(self.sources.iter().any(|s| {
            s.provenance
                .as_ref()
                .is_some_and(|p| p.contains("sem captura de rede"))
        }));
        assert_eq!(
            self.question.read(cx).text(),
            "Rascunho preservado: qual seria a próxima evidência?"
        );
        assert_eq!(
            self.evidence.as_ref().unwrap(),
            &self.notebook.turns()[0].draft.context
        );
        assert!(self.conversation > self.notebook.turns()[0].conversation);
        assert!(self.preview.is_none());
        assert_eq!(self.bundle_saved.as_ref(), Some(&self.session_stamp(cx)));
    }
}
