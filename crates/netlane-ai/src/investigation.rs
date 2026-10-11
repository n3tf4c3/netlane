//! Bounded, local investigation notebook. Recording a hypothesis never confirms it.
use crate::{
    Context,
    report::{Draft, FileHash, publish, section},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
mod import_tests;
mod persistence;
mod resume;

pub const MAX_TURNS: usize = 24;
pub const MAX_NOTES: usize = 32;
pub const MAX_NOTE_BYTES: usize = 4096;
pub const MAX_NOTE_CHANGES: usize = 32;
pub const MAX_NOTEBOOK_BYTES: usize = 24 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteState {
    Investigating,
    Confirmed,
    Discarded,
}
impl NoteState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Investigating => "Em investigação",
            Self::Confirmed => "Confirmada pelo usuário",
            Self::Discarded => "Descartada pelo usuário",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Investigating => Self::Confirmed,
            Self::Confirmed => Self::Discarded,
            Self::Discarded => Self::Investigating,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Turn {
    pub id: usize,
    pub conversation: u64,
    pub draft: Draft,
    pub references_valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_unix_seconds: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoteChange {
    pub unix_seconds: u64,
    pub state: NoteState,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: usize,
    pub text: String,
    pub state: NoteState,
    pub context: Context,
    pub source_turn: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_unix_seconds: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "persistence::note_changes"
    )]
    pub state_history: Vec<NoteChange>,
}
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Notebook {
    #[serde(deserialize_with = "persistence::turns")]
    turns: Vec<Turn>,
    #[serde(deserialize_with = "persistence::notes")]
    notes: Vec<Note>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub schema: u32,
    pub turns: usize,
    pub notes: usize,
    #[serde(deserialize_with = "persistence::files")]
    pub files: Vec<FileHash>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
/// Local-clock registration, displayed explicitly in UTC rather than packet time.
pub fn registration_label(value: u64) -> String {
    if value > 253_402_300_799 {
        return "Horário inválido".into();
    }
    let mut days = value / 86_400;
    let mut year = 1970 + (days / 146_097) * 400;
    days %= 146_097;
    let leap = |year: u64| {
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
    };
    loop {
        let length = if leap(year) { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for length in months {
        if days < length {
            break;
        }
        days -= length;
        month += 1;
    }
    let seconds = value % 86_400;
    format!(
        "{year:04}-{month:02}-{:02} {:02}:{:02}:{:02} UTC",
        days + 1,
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}
fn checked_draft(draft: &Draft) -> Result<bool> {
    draft.context.prompt(&draft.problem, &draft.question)?;
    let response = draft
        .response
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Turno sem resposta/status."))?;
    ensure!(
        response.text.len() <= 128 * 1024,
        "Resposta acima do limite."
    );
    for field in [&response.provider, &response.model, &response.status] {
        ensure!(
            !field.trim().is_empty() && field.len() <= 256 && !field.chars().any(char::is_control),
            "Metadados do turno inválidos."
        );
    }
    Ok(draft.context.validate_references(&response.text).is_ok())
}
impl Notebook {
    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty() && self.notes.is_empty()
    }
    pub fn can_start(&self) -> Result<()> {
        ensure!(
            self.turns.len() < MAX_TURNS,
            "O caderno chegou a 24 turnos. Salve-o antes de iniciar outra investigação."
        );
        Ok(())
    }
    pub fn record(&mut self, conversation: u64, draft: Draft) -> Result<usize> {
        self.can_start()?;
        ensure!(
            conversation > 0 && conversation < u64::MAX,
            "Identificador de conversa inválido."
        );
        let references_valid = checked_draft(&draft)?;
        let id = self.turns.len() + 1;
        self.turns.push(Turn {
            id,
            conversation,
            draft,
            references_valid,
            created_unix_seconds: Some(now()),
        });
        Ok(id)
    }
    pub fn add_note(
        &mut self,
        text: &str,
        context: Context,
        source_turn: Option<usize>,
    ) -> Result<usize> {
        ensure!(
            self.notes.len() < MAX_NOTES,
            "O caderno chegou a 32 anotações. Salve-o antes de iniciar outro."
        );
        ensure!(
            !text.trim().is_empty() && text.len() <= MAX_NOTE_BYTES,
            "Escreva uma anotação de até 4 KiB."
        );
        context.validate()?;
        if let Some(id) = source_turn {
            let source = self
                .turns
                .iter()
                .find(|t| t.id == id)
                .ok_or_else(|| anyhow::anyhow!("Turno de origem inexistente."))?;
            ensure!(
                source.draft.context == context,
                "A anotação deve conservar as evidências do turno de origem."
            );
        }
        let id = self.notes.len() + 1;
        let created = now();
        self.notes.push(Note {
            id,
            text: text.trim().into(),
            state: NoteState::Investigating,
            context,
            source_turn,
            created_unix_seconds: Some(created),
            state_history: vec![NoteChange {
                unix_seconds: created,
                state: NoteState::Investigating,
            }],
        });
        Ok(id)
    }
    pub fn cycle_note(&mut self, id: usize) -> Result<()> {
        let note = self
            .notes
            .iter_mut()
            .find(|n| n.id == id)
            .ok_or_else(|| anyhow::anyhow!("Anotação inexistente."))?;
        let needed = if note.state_history.is_empty() { 2 } else { 1 };
        ensure!(
            note.state_history.len() + needed <= MAX_NOTE_CHANGES,
            "A anotação chegou ao limite de 32 registros de estado. Salve o caderno antes de iniciar outro."
        );
        let timestamp = now();
        if note.state_history.is_empty() {
            note.state_history.push(NoteChange {
                unix_seconds: timestamp,
                state: note.state,
            });
        }
        note.state = note.state.next();
        note.state_history.push(NoteChange {
            unix_seconds: timestamp,
            state: note.state,
        });
        Ok(())
    }
    pub fn markdown(&self) -> String {
        let mut out="# NetLane — caderno de investigação\n\nRegistro local da conversa e das anotações. Hipóteses e respostas da IA exigem revisão humana. Os estados das anotações são decisões do usuário. Não contém os bytes dos pacotes.\n\n".to_owned();
        for turn in &self.turns {
            out.push_str(&format!(
                "## Turno {} · conversa {}\n\n",
                turn.id, turn.conversation
            ));
            section(
                &mut out,
                "Evidências no envio",
                &format!(
                    "SHA-256: {}\nPacotes: {:?}\nReferências na resposta: {}",
                    turn.draft.context.sha256,
                    turn.draft
                        .context
                        .packets
                        .iter()
                        .map(|p| p.number)
                        .collect::<Vec<_>>(),
                    if turn.references_valid {
                        "conferidas contra este contexto; não validam a conclusão"
                    } else {
                        "INVÁLIDAS; resposta conservada apenas como registro"
                    }
                ),
            );
            section(&mut out, "Problema relatado", &turn.draft.problem);
            section(&mut out, "Você", &turn.draft.question);
            section(
                &mut out,
                "Horário de registro",
                &turn
                    .created_unix_seconds
                    .map_or("Não registrado nesta exportação antiga.".into(), |v| {
                        format!("{} (relógio do Windows).", registration_label(v))
                    }),
            );
            if let Some(response) = &turn.draft.response {
                section(&mut out, "Codex", &response.text);
                section(
                    &mut out,
                    "Estado do turno",
                    &format!(
                        "{} · {} · {}",
                        response.provider, response.model, response.status
                    ),
                );
            }
        }
        for note in &self.notes {
            out.push_str(&format!("## Anotação {}\n\n", note.id));
            section(&mut out, "Estado", note.state.label());
            section(&mut out, "Hipótese ou observação", &note.text);
            section(
                &mut out,
                "Histórico de estados",
                &if note.state_history.is_empty() {
                    "Não registrado nesta exportação antiga.".into()
                } else {
                    note.state_history
                        .iter()
                        .map(|change| {
                            format!(
                                "{}: {}",
                                registration_label(change.unix_seconds),
                                change.state.label()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                },
            );
            section(
                &mut out,
                "Origem",
                &format!(
                    "SHA-256: {}\nPacotes: {:?}\nTurno: {:?}",
                    note.context.sha256,
                    note.context
                        .packets
                        .iter()
                        .map(|p| p.number)
                        .collect::<Vec<_>>(),
                    note.source_turn
                ),
            );
        }
        out
    }
    pub fn save(&self, directory: &Path) -> Result<Receipt> {
        ensure!(!self.is_empty(), "O caderno está vazio.");
        self.validate()?;
        let json = serde_json::to_vec_pretty(&serde_json::json!({"schema":1,"notebook":self}))?;
        ensure!(
            json.len() <= MAX_NOTEBOOK_BYTES,
            "Caderno excede o limite de exportação."
        );
        let files = [
            ("investigation.json", json),
            ("investigation.md", self.markdown().into_bytes()),
        ];
        let receipt = Receipt {
            schema: 1,
            turns: self.turns.len(),
            notes: self.notes.len(),
            files: files
                .iter()
                .map(|(name, bytes)| FileHash {
                    file: (*name).into(),
                    bytes: bytes.len(),
                    sha256: format!("{:x}", Sha256::digest(bytes)),
                })
                .collect(),
        };
        fs::create_dir(directory)?;
        for (name, bytes) in files {
            publish(directory, name, &bytes)?;
            ensure!(
                fs::read(directory.join(name))? == bytes,
                "Caderno gravado diverge; recibo não publicado."
            );
        }
        publish(
            directory,
            "receipt.json",
            &serde_json::to_vec_pretty(&receipt)?,
        )?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn draft() -> Draft {
        let context = Context {
            investigation_id: "qa".into(),
            sha256: "A".repeat(64),
            format: "PCAP".into(),
            total_packets: 1,
            inspected_packets: 1,
            omitted_packets: 0,
            analysis: None,
            packets: vec![crate::PacketEvidence {
                number: 1,
                interface: "fixture".into(),
                section: 0,
                link_type: 1,
                unix_nanos: None,
                captured_bytes: 64,
                original_bytes: 64,
                protocol: "TCP".into(),
                source: "192.0.2.10".into(),
                destination: "198.51.100.20".into(),
                observation: "RST".into(),
            }],
        };
        Draft {
            context,
            problem: "Falha intermitente".into(),
            question: "Pode ser recusa de conexão?".into(),
            response: Some(crate::report::Response {
                provider: "Codex simulado".into(),
                model: "fixture".into(),
                status: "Concluído; revisão pendente".into(),
                text: "Hipótese: verificar as recusas.".into(),
            }),
        }
    }
    #[test]
    fn notes_keep_turn_evidence_and_require_human_state_changes() -> Result<()> {
        let mut notebook = Notebook::default();
        let mut sent = draft();
        let id = notebook.record(1, sent.clone())?;
        notebook.add_note("<script>hipótese</script>", sent.context.clone(), Some(id))?;
        sent.context.sha256 = "B".repeat(64);
        assert!(
            notebook
                .add_note("Origem trocada", sent.context.clone(), Some(id))
                .is_err()
        );
        notebook.record(2, sent)?;
        assert_eq!(notebook.notes()[0].state, NoteState::Investigating);
        assert_eq!(notebook.notes()[0].context.sha256, "A".repeat(64));
        notebook.cycle_note(1)?;
        assert_eq!(notebook.notes()[0].state, NoteState::Confirmed);
        let folder = tempfile::tempdir()?;
        let destination = folder.path().join("notebook");
        let receipt = notebook.save(&destination)?;
        assert_eq!((receipt.turns, receipt.notes), (2, 1));
        assert!(notebook.save(&destination).is_err());
        let md = fs::read_to_string(destination.join("investigation.md"))?;
        assert!(md.contains("    <script>hipótese</script>"));
        Ok(())
    }
    #[test]
    fn invalid_and_partial_answers_remain_flagged_and_history_is_not_evicted() -> Result<()> {
        let mut notebook = Notebook::default();
        let mut response = draft();
        response.response.as_mut().unwrap().text = format!("[[packet:{}:99]]", "A".repeat(64));
        notebook.record(1, response)?;
        assert!(!notebook.turns()[0].references_valid);
        for _ in 1..MAX_TURNS {
            notebook.record(1, draft())?;
        }
        assert!(notebook.can_start().is_err());
        assert!(notebook.record(1, draft()).is_err());
        assert_eq!(notebook.turns().len(), MAX_TURNS);
        Ok(())
    }
}
