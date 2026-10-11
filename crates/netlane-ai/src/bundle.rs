//! Portable local investigation: bounded notebook plus verified full dumps.
//! Source paths never enter JSON or AI context. A manifest is published last;
//! cancellation/errors preserve partial output without declaring it complete.
use crate::{Context, investigation::Notebook};
use anyhow::{Context as _, Result, ensure};
use netlane_capture::{
    CaptureFile,
    analysis::{self, Report, Scan},
    lookup::{self, RecoveredPacket},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

pub const MAX_CAPTURES: usize = 128;
pub const MAX_CAPTURE_BYTES: u64 = analysis::MAX_DUMP_BYTES;
const MAX_MANIFEST_BYTES: u64 = 512 * 1024;
const COPY_BUFFER: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Source {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub packets: u64,
    pub format: String,
    pub provenance: Option<String>,
}
impl Source {
    pub fn new(path: PathBuf, capture: &CaptureFile) -> Self {
        Self {
            path,
            sha256: capture.sha256.clone(),
            bytes: capture.file_bytes,
            packets: capture.total_packets,
            format: capture.format.label().into(),
            provenance: None,
        }
    }
    fn entry(&self) -> Entry {
        let hash = self.sha256.to_ascii_lowercase();
        Entry {
            file: format!(
                "captures/{hash}.{}",
                if self.format == "PCAP" {
                    "pcap"
                } else {
                    "pcapng"
                }
            ),
            sha256: hash,
            bytes: self.bytes,
            packets: self.packets,
            format: self.format.clone(),
            provenance: self.provenance.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub file: String,
    pub sha256: String,
    pub bytes: u64,
    pub packets: u64,
    pub format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    #[serde(deserialize_with = "entries")]
    pub captures: Vec<Entry>,
    pub current: Context,
    #[serde(default)]
    pub inputs: Inputs,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inputs {
    pub problem: String,
    pub question: String,
}
impl Inputs {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.problem.len() <= crate::MAX_PROBLEM_BYTES && self.question.len() <= 4096,
            "Rascunho de retomada excede o limite."
        );
        Ok(())
    }
}
fn entries<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Vec<Entry>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<Entry>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "até {MAX_CAPTURES} capturas")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element()? {
                if values.len() == MAX_CAPTURES {
                    return Err(serde::de::Error::custom("Capturas demais na investigação."));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    d.deserialize_seq(Visitor)
}
#[derive(Default)]
pub struct Progress {
    pub bytes: AtomicU64,
    pub total: AtomicU64,
    pub files: AtomicU64,
}
pub struct Opened {
    pub notebook: Notebook,
    pub current: Context,
    pub sources: Vec<Source>,
    pub scan: Scan,
    pub baseline: Option<Report>,
    pub recovered: Option<RecoveredPacket>,
    pub inputs: Inputs,
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Operação cancelada; arquivos parciais preservados."
    );
    Ok(())
}
fn plain(path: &Path, directory: bool) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    ensure!(
        !m.file_type().is_symlink(),
        "Links não são aceitos na investigação."
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            m.file_attributes() & 0x400 == 0,
            "Junções/links não são aceitos na investigação."
        );
    }
    ensure!(
        if directory { m.is_dir() } else { m.is_file() },
        "Tipo de arquivo inválido na investigação."
    );
    Ok(())
}
fn bounded_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    local(path, false)?;
    let file = File::open(path)?;
    let before = file.metadata()?;
    ensure!(
        before.len() <= limit,
        "Arquivo da investigação excede o limite."
    );
    let mut bytes = Vec::new();
    (&file).take(limit + 1).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    ensure!(
        bytes.len() as u64 <= limit
            && bytes.len() as u64 == before.len()
            && before.len() == after.len()
            && before.modified().ok() == after.modified().ok(),
        "Arquivo da investigação mudou durante a leitura."
    );
    Ok(bytes)
}
fn local(path: &Path, directory: bool) -> Result<()> {
    ensure!(
        path.is_absolute()
            && !path.to_string_lossy().starts_with("\\\\")
            && !path.to_string_lossy().starts_with("//"),
        "Escolha um caminho absoluto local."
    );
    for component in path.components() {
        if let std::path::Component::Normal(value) = component {
            ensure!(
                !value.to_string_lossy().contains(':'),
                "Fluxos alternativos não são aceitos."
            );
        }
        ensure!(
            !matches!(component, std::path::Component::ParentDir),
            "Caminhos relativos não são aceitos."
        );
    }
    plain(path, directory)?;
    for ancestor in path.ancestors().skip(1) {
        plain(ancestor, true)?;
    }
    Ok(())
}
fn contexts<'a>(book: &'a Notebook, current: &'a Context) -> impl Iterator<Item = &'a Context> {
    book.turns()
        .iter()
        .map(|t| &t.draft.context)
        .chain(book.notes().iter().map(|n| &n.context))
        .chain(std::iter::once(current))
}
pub fn required(book: &Notebook, current: &Context) -> BTreeSet<String> {
    contexts(book, current)
        .flat_map(|c| {
            std::iter::once(&c.sha256)
                .chain(c.analysis.as_ref().and_then(|a| a.baseline_sha256.as_ref()))
        })
        .map(|s| s.to_ascii_lowercase())
        .collect()
}
fn validate(book: &Notebook, current: &Context, captures: &[Entry]) -> Result<u64> {
    book.validate()?;
    current.validate()?;
    ensure!(
        !captures.is_empty() && captures.len() <= MAX_CAPTURES,
        "Contagem de capturas inválida."
    );
    let mut hashes = BTreeSet::new();
    let mut bytes = 0u64;
    for entry in captures {
        ensure!(
            entry.sha256.len() == 64
                && entry
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                && matches!(entry.format.as_str(), "PCAP" | "PCAPNG")
                && entry.bytes > 0
                && entry.bytes <= MAX_CAPTURE_BYTES
                && entry.packets <= analysis::MAX_DUMP_RECORDS
                && entry.provenance.as_ref().is_none_or(|s| s.len() <= 4096)
                && hashes.insert(entry.sha256.clone()),
            "Metadados de captura inválidos ou duplicados."
        );
        let source = Source {
            path: PathBuf::new(),
            sha256: entry.sha256.clone(),
            bytes: entry.bytes,
            packets: entry.packets,
            format: entry.format.clone(),
            provenance: entry.provenance.clone(),
        };
        ensure!(
            entry.file == source.entry().file,
            "Nome de captura inesperado; caminhos do manifesto não são seguidos."
        );
        bytes = bytes
            .checked_add(entry.bytes)
            .context("Orçamento de captura inválido.")?;
    }
    ensure!(
        bytes <= MAX_CAPTURE_BYTES,
        "A investigação excede 64 GiB de dumps. Divida-a antes de salvar."
    );
    ensure!(
        hashes == required(book, current),
        "As capturas não correspondem às referências da investigação, incluindo baselines."
    );
    for context in contexts(book, current) {
        let entry = captures
            .iter()
            .find(|e| e.sha256.eq_ignore_ascii_case(&context.sha256))
            .context("Dump da conversa ausente.")?;
        ensure!(
            entry.packets == context.total_packets && entry.format == context.format,
            "O contexto diverge dos totais/formato da captura."
        );
    }
    Ok(bytes)
}
pub fn plan(book: &Notebook, current: &Context, sources: &[Source]) -> Result<Vec<Source>> {
    ensure!(
        sources.len() <= MAX_CAPTURES,
        "Fontes demais; inicie outra investigação."
    );
    let mut unique = BTreeMap::new();
    for source in sources {
        ensure!(
            unique
                .insert(source.sha256.to_ascii_lowercase(), source.clone())
                .is_none(),
            "Fonte duplicada."
        );
    }
    let selected: Vec<_> = required(book, current).into_iter().map(|hash| unique.remove(&hash).with_context(|| format!("Dump {hash} indisponível. Abra o original e prepare-o para a análise antes de salvar com capturas."))).collect::<Result<_>>()?;
    validate(
        book,
        current,
        &selected.iter().map(Source::entry).collect::<Vec<_>>(),
    )?;
    Ok(selected)
}
struct Copying<'a> {
    reader: BufReader<File>,
    writer: BufWriter<File>,
    progress: &'a Progress,
}
impl Read for Copying<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let read = self.reader.read(bytes)?;
        self.writer.write_all(&bytes[..read])?;
        self.progress
            .bytes
            .fetch_add(read as u64, Ordering::Relaxed);
        Ok(read)
    }
}
fn hash_file(
    path: &Path,
    expected: &Entry,
    cancel: &AtomicBool,
    progress: &Progress,
) -> Result<()> {
    local(path, false)?;
    let mut input = BufReader::with_capacity(COPY_BUFFER, File::open(path)?);
    let mut buffer = vec![0; COPY_BUFFER];
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    loop {
        check_cancel(cancel)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes += count as u64;
        ensure!(bytes <= expected.bytes, "Cópia mudou de tamanho.");
        hash.update(&buffer[..count]);
        progress.bytes.fetch_add(count as u64, Ordering::Relaxed);
    }
    ensure!(
        bytes == expected.bytes && format!("{:x}", hash.finalize()) == expected.sha256,
        "A cópia gravada diverge do dump; pacote não concluído."
    );
    Ok(())
}
fn copy_source(
    source: &Source,
    root: &Path,
    cancel: &AtomicBool,
    progress: &Progress,
) -> Result<()> {
    check_cancel(cancel)?;
    local(&source.path, false)?;
    let input = File::open(&source.path)?;
    let before = input.metadata()?;
    ensure!(
        before.is_file() && before.len() == source.bytes,
        "Dump original mudou de tamanho."
    );
    let entry = source.entry();
    let target = root.join(&entry.file);
    let part = target.with_extension(format!(
        "{}.part",
        if source.format == "PCAP" {
            "pcap"
        } else {
            "pcapng"
        }
    ));
    let mut copying = Copying {
        reader: BufReader::with_capacity(COPY_BUFFER, input),
        writer: BufWriter::with_capacity(
            COPY_BUFFER,
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&part)?,
        ),
        progress,
    };
    let scan = analysis::read_dump(&mut copying, cancel, &analysis::Progress::default())?;
    let after = copying.reader.get_ref().metadata()?;
    ensure!(
        before.len() == after.len()
            && before.modified().ok() == after.modified().ok()
            && scan.capture.sha256.eq_ignore_ascii_case(&entry.sha256)
            && scan.capture.file_bytes == entry.bytes
            && scan.capture.total_packets == entry.packets
            && scan.capture.format.label() == entry.format,
        "Dump original mudou ou diverge da referência; pacote não concluído."
    );
    copying.writer.flush()?;
    copying.writer.get_ref().sync_all()?;
    drop(copying);
    hash_file(&part, &entry, cancel, progress)?;
    check_cancel(cancel)?;
    ensure!(!target.exists(), "Destino já existe; captura preservada.");
    fs::rename(&part, &target)?;
    progress.files.fetch_add(1, Ordering::Relaxed);
    Ok(())
}
fn report_files(draft: &crate::report::Draft) -> Result<Vec<(&'static str, Vec<u8>)>> {
    Ok(vec![
        ("report.md", draft.markdown()?.into_bytes()),
        ("report.html", draft.html()?.into_bytes()),
        ("context.json", serde_json::to_vec_pretty(draft)?),
    ])
}
fn verify_reports(root: &Path, book: &Notebook, cancel: &AtomicBool) -> Result<()> {
    local(&root.join("reports"), true)?;
    for turn in book.turns().iter().filter(|t| t.references_valid) {
        check_cancel(cancel)?;
        let directory = root.join(format!("reports/turn-{:04}", turn.id));
        let receipt: crate::report::Receipt =
            serde_json::from_slice(&bounded_file(&directory.join("receipt.json"), 64 * 1024)?)?;
        ensure!(
            receipt.schema == 1
                && receipt.files.len() == 3
                && receipt.source_sha256 == turn.draft.context.sha256
                && receipt.investigation_id == turn.draft.context.investigation_id
                && receipt.references_checked == turn.draft.response.is_some()
                && receipt.packet_numbers
                    == turn
                        .draft
                        .context
                        .packets
                        .iter()
                        .map(|p| p.number)
                        .collect::<Vec<_>>(),
            "Recibo de relatório inconsistente."
        );
        for (name, expected) in report_files(&turn.draft)? {
            ensure!(
                expected.len() <= 2 * 1024 * 1024,
                "Relatório excede o limite do pacote."
            );
            let actual = bounded_file(&directory.join(name), 2 * 1024 * 1024)?;
            ensure!(actual == expected, "Relatório diverge do turno registrado.");
            ensure!(
                receipt
                    .files
                    .iter()
                    .filter(|f| f.file == name
                        && f.bytes == actual.len()
                        && f.sha256 == format!("{:x}", Sha256::digest(&actual)))
                    .count()
                    == 1,
                "Hash do relatório inconsistente."
            );
        }
    }
    Ok(())
}
pub fn save(
    directory: &Path,
    book: &Notebook,
    current: &Context,
    sources: &[Source],
    cancel: &AtomicBool,
    progress: &Progress,
) -> Result<Manifest> {
    let inputs = book
        .turns()
        .last()
        .map_or_else(Inputs::default, |t| Inputs {
            problem: t.draft.problem.clone(),
            question: t.draft.question.clone(),
        });
    save_with_inputs(directory, book, current, sources, &inputs, cancel, progress)
}
pub fn save_with_inputs(
    directory: &Path,
    book: &Notebook,
    current: &Context,
    sources: &[Source],
    inputs: &Inputs,
    cancel: &AtomicBool,
    progress: &Progress,
) -> Result<Manifest> {
    inputs.validate()?;
    let selected = plan(book, current, sources)?;
    check_cancel(cancel)?;
    local(directory.parent().context("Pasta sem destino pai.")?, true)?;
    ensure!(directory.is_absolute(), "Escolha uma pasta absoluta local.");
    // Validate lexical destination too, including ADS and parent components.
    ensure!(!directory.components().any(|c| matches!(c, std::path::Component::ParentDir) || matches!(c, std::path::Component::Normal(v) if v.to_string_lossy().contains(':'))), "Destino inválido.");
    fs::create_dir(directory).context("A investigação exige uma pasta nova.")?;
    fs::create_dir(directory.join("captures"))?;
    let manifest = Manifest {
        schema: 1,
        captures: selected.iter().map(Source::entry).collect(),
        current: current.clone(),
        inputs: inputs.clone(),
    };
    progress.total.store(
        selected.iter().map(|s| s.bytes).sum::<u64>() * 2,
        Ordering::Relaxed,
    );
    book.save(&directory.join("notebook"))?;
    fs::create_dir(directory.join("reports"))?;
    for turn in book.turns().iter().filter(|t| t.references_valid) {
        check_cancel(cancel)?;
        turn.draft
            .save(&directory.join(format!("reports/turn-{:04}", turn.id)))?;
    }
    verify_reports(directory, book, cancel)?;
    for source in &selected {
        copy_source(source, directory, cancel, progress)?;
    }
    check_cancel(cancel)?;
    let data = serde_json::to_vec_pretty(&manifest)?;
    ensure!(
        data.len() as u64 <= MAX_MANIFEST_BYTES,
        "Manifesto excede o limite."
    );
    crate::report::publish(directory, "bundle.json", &data)?;
    Ok(manifest)
}
struct Counting<'a> {
    reader: BufReader<File>,
    progress: &'a Progress,
}
impl Read for Counting<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.reader.read(buffer)?;
        self.progress
            .bytes
            .fetch_add(read as u64, Ordering::Relaxed);
        Ok(read)
    }
}
pub fn read_source(source: &Source, cancel: &AtomicBool, progress: &Progress) -> Result<Scan> {
    check_cancel(cancel)?;
    local(&source.path, false)?;
    ensure!(source.bytes <= MAX_CAPTURE_BYTES, "Dump excede o limite.");
    let file = File::open(&source.path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() == source.bytes,
        "Captura ausente ou de tamanho divergente."
    );
    let mut reader = Counting {
        reader: BufReader::with_capacity(COPY_BUFFER, file),
        progress,
    };
    let mut scan = analysis::read_dump(&mut reader, cancel, &analysis::Progress::default())?;
    let after = reader.reader.get_ref().metadata()?;
    ensure!(
        before.len() == after.len()
            && before.modified().ok() == after.modified().ok()
            && scan.capture.sha256.eq_ignore_ascii_case(&source.sha256)
            && scan.capture.file_bytes == source.bytes
            && scan.capture.total_packets == source.packets
            && scan.capture.format.label() == source.format,
        "Captura alterada ou inconsistente."
    );
    scan.source = Some(source.path.clone());
    progress.files.fetch_add(1, Ordering::Relaxed);
    check_cancel(cancel)?;
    Ok(scan)
}
pub fn recover_selection(
    scan: &Scan,
    number: u64,
    cancel: &AtomicBool,
    progress: &Progress,
) -> Result<Option<RecoveredPacket>> {
    if scan.capture.packets.iter().any(|p| p.number == number) {
        return Ok(None);
    }
    progress
        .total
        .fetch_add(scan.capture.file_bytes, Ordering::Relaxed);
    let source = scan.source.as_ref().context("Dump original ausente.")?;
    local(source, false)?;
    let file = File::open(source)?;
    let before = file.metadata()?;
    let mut reader = Counting {
        reader: BufReader::with_capacity(COPY_BUFFER, file),
        progress,
    };
    let recovered = lookup::read_packet(
        &mut reader,
        &lookup::PacketAnchor::new(&scan.capture, number)?,
        cancel,
        &analysis::Progress::default(),
    )?;
    let after = reader.reader.get_ref().metadata()?;
    ensure!(
        before.len() == after.len() && before.modified().ok() == after.modified().ok(),
        "Dump mudou durante a recuperação do pacote."
    );
    Ok(Some(recovered))
}
pub fn load(directory: &Path, cancel: &AtomicBool, progress: &Progress) -> Result<Opened> {
    check_cancel(cancel)?;
    local(directory, true)?;
    let manifest_path = directory.join("bundle.json");
    local(&manifest_path, false)?;
    let data = bounded_file(&manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest: Manifest =
        serde_json::from_slice(&data).context("Manifesto da investigação inválido.")?;
    ensure!(manifest.schema == 1, "Versão de investigação incompatível.");
    manifest.inputs.validate()?;
    let notebook = Notebook::load(&directory.join("notebook"))?;
    verify_reports(directory, &notebook, cancel)?;
    let total = validate(&notebook, &manifest.current, &manifest.captures)?;
    progress.total.store(total, Ordering::Relaxed);
    let mut sources = Vec::new();
    let mut active = None;
    let mut baseline = None;
    for entry in &manifest.captures {
        check_cancel(cancel)?;
        let path = directory.join(&entry.file);
        local(&path, false)?;
        let file = File::open(&path)?;
        let before = file.metadata()?;
        ensure!(
            before.is_file() && before.len() == entry.bytes,
            "Captura do pacote ausente ou de tamanho divergente."
        );
        let mut reader = Counting {
            reader: BufReader::with_capacity(COPY_BUFFER, file),
            progress,
        };
        let mut scan = analysis::read_dump(&mut reader, cancel, &analysis::Progress::default())?;
        let after = reader.reader.get_ref().metadata()?;
        ensure!(
            before.len() == after.len()
                && before.modified().ok() == after.modified().ok()
                && scan.capture.sha256.eq_ignore_ascii_case(&entry.sha256)
                && scan.capture.file_bytes == entry.bytes
                && scan.capture.total_packets == entry.packets
                && scan.capture.format.label() == entry.format,
            "Captura do pacote alterada ou inconsistente."
        );
        scan.source = Some(path.clone());
        let mut source = Source::new(path, &scan.capture);
        source.provenance = entry.provenance.clone();
        sources.push(source);
        if manifest
            .current
            .analysis
            .as_ref()
            .and_then(|a| a.baseline_sha256.as_ref())
            .is_some_and(|hash| hash.eq_ignore_ascii_case(&entry.sha256))
        {
            baseline = Some(scan.report.clone());
        }
        if manifest.current.sha256.eq_ignore_ascii_case(&entry.sha256) {
            active = Some(scan);
        }
        progress.files.fetch_add(1, Ordering::Relaxed);
    }
    let scan = active.context("Captura atual ausente do pacote.")?;
    let number = manifest.current.packets[0].number;
    let recovered = recover_selection(&scan, number, cancel, progress)?;
    check_cancel(cancel)?;
    ensure!(
        bounded_file(&manifest_path, MAX_MANIFEST_BYTES)? == data,
        "Manifesto mudou durante a abertura."
    );
    ensure!(
        serde_json::to_vec(&Notebook::load(&directory.join("notebook"))?)?
            == serde_json::to_vec(&notebook)?,
        "Caderno mudou durante a abertura."
    );
    Ok(Opened {
        notebook,
        current: manifest.current,
        sources,
        scan,
        baseline,
        recovered,
        inputs: manifest.inputs,
    })
}
