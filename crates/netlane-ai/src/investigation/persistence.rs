use super::*;
use anyhow::Context as _;
use std::{fs::File, io::Read};

fn bounded<'de, D, T, const N: usize>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Items<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Items<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "no máximo {N} registros")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut items = Vec::new();
            while let Some(item) = sequence.next_element()? {
                if items.len() == N {
                    return Err(serde::de::Error::custom(format!(
                        "Mais de {N} registros no caderno."
                    )));
                }
                items.push(item);
            }
            Ok(items)
        }
    }
    deserializer.deserialize_seq(Items::<T, N>(std::marker::PhantomData))
}
pub(super) fn turns<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<Turn>, D::Error> {
    bounded::<D, _, MAX_TURNS>(d)
}
pub(super) fn notes<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<Note>, D::Error> {
    bounded::<D, _, MAX_NOTES>(d)
}
pub(super) fn note_changes<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<NoteChange>, D::Error> {
    bounded::<D, _, MAX_NOTE_CHANGES>(d)
}
pub(super) fn files<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<FileHash>, D::Error> {
    bounded::<D, _, 2>(d)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    schema: u32,
    notebook: Notebook,
}

fn plain_path(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "Links não são aceitos no caderno."
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Junções/links não são aceitos no caderno."
        );
    }
    ensure!(
        if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        },
        "Tipo de arquivo do caderno inválido."
    );
    Ok(())
}

fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    plain_path(path, false)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "Arquivo do caderno excede o limite permitido."
    );
    Ok(bytes)
}

impl Notebook {
    /// Explicit, bounded import of the three known files. Never follows paths in JSON.
    /// Hashes check bundle consistency, not authorship or the truth of a hypothesis.
    pub fn load(directory: &Path) -> Result<Self> {
        ensure!(
            directory.is_absolute()
                && !directory.to_string_lossy().starts_with("\\\\")
                && !directory.to_string_lossy().starts_with("//"),
            "Escolha uma pasta absoluta local para abrir o caderno."
        );
        for parent in directory.ancestors() {
            plain_path(parent, true)?;
        }
        let receipt: Receipt =
            serde_json::from_slice(&read_file(&directory.join("receipt.json"), 64 * 1024)?)
                .context("Recibo de investigação inválido.")?;
        ensure!(
            receipt.schema == 1 && receipt.files.len() == 2,
            "Formato de recibo de investigação incompatível."
        );
        let mut json = None;
        let mut markdown_seen = false;
        for entry in &receipt.files {
            ensure!(
                entry.bytes <= MAX_NOTEBOOK_BYTES
                    && entry.sha256.len() == 64
                    && entry.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "Limite ou hash do caderno inválido."
            );
            ensure!(
                matches!(
                    entry.file.as_str(),
                    "investigation.json" | "investigation.md"
                ),
                "O recibo contém um nome de arquivo inesperado."
            );
            let bytes = read_file(&directory.join(&entry.file), entry.bytes)?;
            ensure!(
                bytes.len() == entry.bytes
                    && format!("{:x}", Sha256::digest(&bytes)).eq_ignore_ascii_case(&entry.sha256),
                "Caderno diverge do recibo: {}.",
                entry.file
            );
            if entry.file == "investigation.json" {
                ensure!(json.is_none(), "Arquivo duplicado no recibo.");
                json = Some(bytes);
            } else {
                ensure!(!markdown_seen, "Arquivo duplicado no recibo.");
                markdown_seen = true;
            }
        }
        ensure!(markdown_seen, "Markdown do caderno ausente no recibo.");
        let bundle: Bundle =
            serde_json::from_slice(&json.context("JSON do caderno ausente no recibo.")?)
                .context("JSON de investigação inválido.")?;
        ensure!(bundle.schema == 1, "Formato de investigação incompatível.");
        let mut notebook = bundle.notebook;
        // Do not trust an imported flag to turn a foreign reference into evidence.
        for turn in &mut notebook.turns {
            turn.references_valid = checked_draft(&turn.draft)?;
        }
        notebook.validate()?;
        ensure!(
            notebook.turns.len() == receipt.turns && notebook.notes.len() == receipt.notes,
            "Contagens do caderno divergem do recibo."
        );
        Ok(notebook)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.is_empty() && self.turns.len() <= MAX_TURNS && self.notes.len() <= MAX_NOTES,
            "Contagens da investigação inválidas."
        );
        let valid_time = |value: u64| value <= 253_402_300_799; // through year 9999
        for (index, turn) in self.turns.iter().enumerate() {
            ensure!(
                turn.id == index + 1
                    && turn.conversation > 0
                    && turn.conversation < u64::MAX
                    && turn.created_unix_seconds.is_none_or(valid_time),
                "Identificador ou horário de turno inválido."
            );
            ensure!(
                checked_draft(&turn.draft)? == turn.references_valid,
                "Estado das referências do turno inconsistente."
            );
        }
        for (index, note) in self.notes.iter().enumerate() {
            ensure!(
                note.id == index + 1
                    && !note.text.trim().is_empty()
                    && note.text.len() <= MAX_NOTE_BYTES
                    && note.created_unix_seconds.is_none_or(valid_time),
                "Identificador, texto ou horário de anotação inválido."
            );
            note.context.validate()?;
            if let Some(id) = note.source_turn {
                ensure!(
                    self.turns
                        .iter()
                        .any(|t| t.id == id && t.draft.context == note.context),
                    "A anotação não corresponde às evidências do turno de origem."
                );
            }
            ensure!(
                note.state_history.len() <= MAX_NOTE_CHANGES
                    && note
                        .state_history
                        .iter()
                        .all(|entry| valid_time(entry.unix_seconds)),
                "Histórico de estados acima do limite ou horário inválido."
            );
            if let Some(created) = note.created_unix_seconds {
                ensure!(
                    note.state_history
                        .first()
                        .is_some_and(|entry| entry.state == NoteState::Investigating
                            && entry.unix_seconds == created),
                    "Registro inicial da anotação inválido."
                );
            }
            ensure!(
                note.state_history
                    .windows(2)
                    .all(|pair| pair[1].state == pair[0].state.next()),
                "Transição de estado da anotação inválida."
            );
            ensure!(
                note.state_history
                    .last()
                    .is_none_or(|entry| entry.state == note.state),
                "Estado atual da anotação diverge do histórico."
            );
        }
        Ok(())
    }

    pub fn next_conversation(&self, current: u64) -> Result<u64> {
        let next = self
            .turns
            .iter()
            .map(|turn| turn.conversation)
            .max()
            .unwrap_or(0)
            .max(current)
            .checked_add(1)
            .filter(|next| *next < u64::MAX)
            .ok_or_else(|| anyhow::anyhow!("Limite de identificadores de conversa atingido."))?;
        Ok(next)
    }
}
