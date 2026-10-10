//! IPv4 priority planning and recovery. No Windows calls or network I/O.
use crate::persistence::{
    atomic_write_checked, exclusive_file, read_bounded, revision, without_bom,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
use uuid::Uuid;

const LIMIT: usize = 64;
const MAX_JOURNAL: u64 = 65_536;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    #[serde(rename = "Cabo")]
    Cable,
    #[serde(rename = "Wi-Fi")]
    Wifi,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Interface {
    pub id: Uuid,
    pub name: String,
    pub kind: Kind,
    pub index: u32,
    pub connected: bool,
    pub automatic_metric: bool,
    pub metric: u32,
    pub default_route_metric: Option<u32>,
    pub saved_metric: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Route {
    pub index: u32,
    pub name: String,
    pub interface_metric: u32,
    pub route_metric: u32,
}
impl Route {
    pub fn total(&self) -> u64 {
        u64::from(self.interface_metric) + u64::from(self.route_metric)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Snapshot {
    pub interfaces: Vec<Interface>,
    pub routes: Vec<Route>,
}
impl Snapshot {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.interfaces.len() <= LIMIT && self.routes.len() <= 256,
            "Inventário de conexões excede o limite."
        );
        let valid_name =
            |s: &str| !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control);
        let mut ids = std::collections::HashSet::new();
        let mut indices = std::collections::HashSet::new();
        for i in &self.interfaces {
            ensure!(
                !i.id.is_nil()
                    && i.index != 0
                    && valid_name(&i.name)
                    && ids.insert(i.id)
                    && indices.insert(i.index),
                "Inventário de conexões inválido."
            );
        }
        for r in &self.routes {
            ensure!(r.index != 0 && valid_name(&r.name), "Rota padrão inválida.");
            if let Some(i) = self.interfaces.iter().find(|i| i.index == r.index) {
                ensure!(
                    i.connected && i.metric == r.interface_metric,
                    "Leitura inconsistente da prioridade IPv4. Atualize a leitura."
                );
            }
        }
        for i in self.interfaces.iter().filter(|i| i.connected) {
            let minimum = self
                .routes
                .iter()
                .filter(|r| r.index == i.index)
                .map(|r| r.route_metric)
                .min();
            ensure!(
                i.default_route_metric == minimum,
                "Leitura inconsistente das rotas padrão. Atualize a leitura."
            );
        }
        Ok(())
    }
    pub fn current_label(&self) -> String {
        let Some(best) = self.routes.iter().min_by_key(|r| r.total()) else {
            return "Sem rota padrão IPv4 disponível".into();
        };
        if self
            .routes
            .iter()
            .any(|r| r.index != best.index && r.total() == best.total())
        {
            "Prioridades iguais — Windows decide".into()
        } else {
            format!("Atual: {}", best.name)
        }
    }
    pub fn interface(&self, id: Uuid) -> Result<&Interface> {
        self.interfaces.iter().find(|i| i.id == id).ok_or_else(|| {
            anyhow::anyhow!(
                "Uma interface anterior está indisponível. Reconecte-a antes de continuar."
            )
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Metric {
    pub id: Uuid,
    pub automatic_metric: bool,
    pub metric: u32,
}
impl Metric {
    pub fn of(i: &Interface) -> Self {
        Self {
            id: i.id,
            automatic_metric: i.automatic_metric,
            metric: i.metric,
        }
    }
    pub fn matches(&self, i: &Interface) -> bool {
        self.id == i.id
            && self.automatic_metric == i.automatic_metric
            && if self.automatic_metric {
                i.saved_metric.is_none_or(|m| m == 0)
            } else {
                self.metric == i.metric && i.saved_metric == Some(self.metric)
            }
    }
}
fn validate_metrics(metrics: &[Metric], nonempty: bool) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    ensure!(
        metrics.len() <= LIMIT
            && (!nonempty || !metrics.is_empty())
            && metrics.iter().all(|m| !m.id.is_nil()
                && (m.automatic_metric || m.metric > 0)
                && ids.insert(m.id)),
        "Lista de prioridades inválida."
    );
    Ok(())
}
pub fn plan(snapshot: &Snapshot, preferred: Uuid, managed: &[Uuid]) -> Result<Vec<Metric>> {
    snapshot.validate()?;
    let selected = snapshot.interface(preferred)?;
    ensure!(
        selected.connected && selected.default_route_metric.is_some(),
        "Conecte a interface escolhida à rede antes de aplicar."
    );
    let affected: Vec<_> = snapshot
        .interfaces
        .iter()
        .filter(|i| {
            i.id == preferred || i.default_route_metric.is_some() || managed.contains(&i.id)
        })
        .collect();
    ensure!(
        affected.iter().all(|i| Metric::of(i).matches(i)),
        "A prioridade ativa difere da configuração salva no Windows. Essa configuração foi preservada; confira antes de aplicar."
    );
    let total = u64::from(selected.default_route_metric.unwrap()) + 5;
    ensure!(
        total + 50 <= u64::from(u32::MAX),
        "A prioridade dessa rota não pode ser ajustada."
    );
    ensure!(
        !snapshot
            .routes
            .iter()
            .any(|r| !affected.iter().any(|i| i.index == r.index) && r.total() <= total),
        "Outra conexão, como uma VPN, tem prioridade. Ajuste essa conexão antes de escolher o padrão."
    );
    Ok(affected
        .into_iter()
        .map(|i| Metric {
            id: i.id,
            automatic_metric: false,
            metric: if i.id == preferred {
                5
            } else {
                (total + 50) as u32
            },
        })
        .collect())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Request {
    pub expected: Snapshot,
    pub changes: Vec<Metric>,
    pub preferred_interface: Option<Uuid>,
}
impl Request {
    pub fn verify_after(&self, after: &Snapshot) -> Result<()> {
        after.validate()?;
        for change in &self.changes {
            let original = self.expected.interface(change.id)?;
            let actual = after.interface(change.id)?;
            ensure!(
                actual.index == original.index
                    && actual.kind == original.kind
                    && change.matches(actual),
                "O Windows não confirmou a identidade e as prioridades solicitadas."
            );
        }
        if let Some(preferred) = self.preferred_interface {
            let i = after.interface(preferred)?;
            let best = after.routes.iter().min_by_key(|r| r.total());
            ensure!(
                i.connected
                    && best.is_some_and(|r| r.index == i.index
                        && !after
                            .routes
                            .iter()
                            .any(|other| other.index != i.index && other.total() <= r.total())),
                "A conexão escolhida não se tornou a rota padrão IPv4."
            );
        }
        Ok(())
    }
    pub fn validate_against(&self, actual: &Snapshot) -> Result<()> {
        actual.validate()?;
        self.expected.validate()?;
        validate_metrics(&self.changes, true)?;
        // A new competing route between preview and authorization must also invalidate the request.
        let routes = |s: &Snapshot| {
            let mut rows: Vec<_> = s
                .routes
                .iter()
                .map(|r| (r.index, r.interface_metric, r.route_metric))
                .collect();
            rows.sort_unstable();
            rows
        };
        ensure!(
            routes(actual) == routes(&self.expected),
            "As rotas mudaram. Atualize a leitura e tente novamente."
        );
        for change in &self.changes {
            let a = actual.interface(change.id)?;
            let e = self.expected.interface(change.id)?;
            ensure!(
                a.index == e.index
                    && a.kind == e.kind
                    && a.connected == e.connected
                    && a.default_route_metric == e.default_route_metric
                    && Metric::of(e).matches(a),
                "A configuração mudou. Atualize a leitura e tente novamente."
            );
        }
        if let Some(selected) = self.preferred_interface {
            let mut expected = plan(
                actual,
                selected,
                &self.changes.iter().map(|m| m.id).collect::<Vec<_>>(),
            )?;
            let mut changes = self.changes.clone();
            expected.sort_by_key(|m| m.id);
            changes.sort_by_key(|m| m.id);
            ensure!(
                expected == changes,
                "Alteração não corresponde à conexão escolhida."
            );
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct OperationResult {
    pub snapshot: Snapshot,
    pub success: bool,
    pub detail: String,
    pub rollback_confirmed: bool,
}
pub trait Settings {
    fn read(&mut self) -> Result<Snapshot>;
    /// Must finish the exact write before returning, including an error after partial application.
    fn write(&mut self, metric: &Metric) -> Result<()>;
}
/// Kept until the authenticated UI acknowledges receipt. A lost channel requires rollback.
pub struct Receipt {
    pub result: OperationResult,
    original: Vec<Interface>,
    changes: Vec<Metric>,
}
impl Receipt {
    pub fn revert(&mut self, settings: &mut impl Settings) {
        let mut errors = Vec::new();
        for original in self.original.iter().rev() {
            let attempt = (|| {
                let now = settings.read()?;
                now.validate()?;
                let i = now.interface(original.id)?;
                let metric = Metric::of(original);
                ensure!(
                    i.index == original.index && i.kind == original.kind,
                    "A identidade da interface mudou durante a operação."
                );
                if metric.matches(i) {
                    return Ok(());
                }
                // A concurrent external edit is never overwritten by cleanup.
                let active_matches = |m: &Metric| {
                    m.automatic_metric == i.automatic_metric
                        && (m.automatic_metric || m.metric == i.metric)
                };
                let saved_matches = |m: &Metric| {
                    if m.automatic_metric {
                        i.saved_metric.is_none_or(|v| v == 0)
                    } else {
                        i.saved_metric == Some(m.metric)
                    }
                };
                let change = self
                    .changes
                    .iter()
                    .find(|m| m.id == original.id)
                    .expect("touched change exists");
                // Active and persistent stores may be only partly updated by a failed CIM writer.
                ensure!(
                    (active_matches(&metric) || active_matches(change))
                        && (saved_matches(&metric) || saved_matches(change)),
                    "Prioridade alterada externamente durante a operação."
                );
                settings.write(&metric)
            })();
            if let Err(e) = attempt {
                errors.push(e.to_string());
            }
        }
        match settings.read().and_then(|s| {
            s.validate()?;
            Ok(s)
        }) {
            Ok(s) => {
                if !self.original.iter().all(|m| {
                    s.interface(m.id).is_ok_and(|i| {
                        i.index == m.index && i.kind == m.kind && Metric::of(m).matches(i)
                    })
                }) {
                    errors.push("Restauração não confirmada.".into());
                }
                self.result.snapshot = s;
            }
            Err(e) => {
                self.result.snapshot = Snapshot::default();
                errors.push(format!("Não foi possível conferir a restauração: {e}"));
            }
        }
        self.result.success = false;
        self.result.rollback_confirmed = errors.is_empty();
        self.result.detail.push_str(&if errors.is_empty() {
            " Alteração revertida.".into()
        } else {
            format!(" Restauração incompleta: {}", errors.join(" | "))
        });
        self.original.clear();
    }
}
pub fn apply(
    settings: &mut impl Settings,
    request: &Request,
    alive: impl Fn() -> bool,
) -> Result<Receipt> {
    let before = settings.read()?;
    request.validate_against(&before)?;
    let mut receipt = Receipt {
        result: OperationResult {
            snapshot: before.clone(),
            success: false,
            detail: String::new(),
            rollback_confirmed: false,
        },
        original: vec![],
        changes: request.changes.clone(),
    };
    let attempt = (|| {
        let mut ordered = request.changes.clone();
        // Raise alternatives first; lower the preferred interface last.
        ordered.sort_by_key(|m| Some(m.id) == request.preferred_interface);
        for m in &ordered {
            ensure!(alive(), "A janela controladora encerrou a operação.");
            let original = before.interface(m.id)?;
            let current = settings.read()?;
            current.validate()?;
            let actual = current.interface(m.id)?;
            ensure!(
                actual.index == original.index
                    && actual.kind == original.kind
                    && Metric::of(original).matches(actual),
                "A prioridade mudou durante a operação. A configuração externa foi preservada."
            );
            ensure!(alive(), "A janela controladora encerrou a operação.");
            receipt.original.push(original.clone());
            settings.write(m)?;
        }
        ensure!(alive(), "A janela controladora encerrou a operação.");
        let after = settings.read()?;
        request.verify_after(&after)?;
        receipt.result.snapshot = after;
        Ok(())
    })();
    match attempt {
        Ok(()) => {
            receipt.result.success = true;
            receipt.result.detail = if request.preferred_interface.is_some() {
                "Conexão padrão IPv4 aplicada. A escolha permanece após fechar o NetLane."
            } else {
                "Configuração anterior restaurada."
            }
            .into();
        }
        Err(e) => {
            receipt.result.detail = e.to_string();
            receipt.revert(settings);
        }
    }
    Ok(receipt)
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Journal {
    pub original: Vec<Metric>,
    pub applied: Vec<Metric>,
    pub previous: Vec<Metric>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
impl Journal {
    pub fn validate(&self) -> Result<()> {
        validate_metrics(&self.original, true)?;
        validate_metrics(&self.applied, false)?;
        validate_metrics(&self.previous, false)?;
        ensure!(
            self.applied
                .iter()
                .chain(&self.previous)
                .all(|m| self.original.iter().any(|o| o.id == m.id)),
            "Registro de restauração incompleto."
        );
        Ok(())
    }
    pub fn check_known(&self, snapshot: &Snapshot) -> Result<()> {
        self.validate()?;
        snapshot.validate()?;
        for original in &self.original {
            let i = snapshot.interface(original.id)?;
            ensure!(
                self.applied
                    .iter()
                    .chain(&self.previous)
                    .chain(&self.original)
                    .any(|m| m.id == original.id && m.matches(i)),
                "A prioridade foi alterada fora do NetLane. A configuração externa foi preservada; confira antes de restaurar."
            );
        }
        Ok(())
    }
    pub fn prepare(
        snapshot: &Snapshot,
        preferred: Uuid,
        existing: Option<&Self>,
    ) -> Result<(Self, Request)> {
        if let Some(j) = existing {
            j.check_known(snapshot)?;
        }
        let mut journal = existing.cloned().unwrap_or_default();
        let changes = plan(
            snapshot,
            preferred,
            &journal.original.iter().map(|m| m.id).collect::<Vec<_>>(),
        )?;
        for change in &changes {
            let current = Metric::of(snapshot.interface(change.id)?);
            if !journal.original.iter().any(|o| o.id == change.id) {
                journal.original.push(current.clone());
            }
        }
        journal.previous = journal.applied.clone();
        for change in &changes {
            upsert(
                &mut journal.previous,
                Metric::of(snapshot.interface(change.id)?),
            );
            upsert(&mut journal.applied, change.clone());
        }
        journal.validate()?;
        Ok((
            journal,
            Request {
                expected: snapshot.clone(),
                changes,
                preferred_interface: Some(preferred),
            },
        ))
    }
    pub fn restoration(&self, snapshot: &Snapshot) -> Result<Option<Request>> {
        self.check_known(snapshot)?;
        let changes: Vec<_> = self
            .original
            .iter()
            .filter(|m| !m.matches(snapshot.interface(m.id).expect("checked above")))
            .cloned()
            .collect();
        Ok((!changes.is_empty()).then(|| Request {
            expected: snapshot.clone(),
            changes,
            preferred_interface: None,
        }))
    }
    pub fn confirmed(&self, snapshot: &Snapshot) -> Result<Self> {
        snapshot.validate()?;
        ensure!(
            self.applied
                .iter()
                .all(|m| snapshot.interface(m.id).is_ok_and(|i| m.matches(i))),
            "A aplicação não foi confirmada; recuperação preservada."
        );
        let mut confirmed = self.clone();
        confirmed.previous.clear();
        Ok(confirmed)
    }
}
fn upsert(list: &mut Vec<Metric>, metric: Metric) {
    list.retain(|m| m.id != metric.id);
    list.push(metric);
    list.sort_by_key(|m| m.id);
}
#[derive(Clone)]
pub struct JournalFile {
    pub path: PathBuf,
}
impl JournalFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn load(&self) -> Result<(Option<Journal>, Option<String>)> {
        let Some(bytes) = read_bounded(&self.path, MAX_JOURNAL)? else {
            return Ok((None, None));
        };
        let journal: Journal = serde_json::from_slice(without_bom(&bytes))?;
        journal.validate()?;
        Ok((Some(journal), Some(revision(&bytes))))
    }
    /// Caller also holds the operation lease across preparation, IPC and finalization.
    pub fn save(&self, journal: &Journal, expected: Option<&str>) -> Result<String> {
        journal.validate()?;
        std::fs::create_dir_all(
            self.path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Pasta de recuperação inválida."))?,
        )?;
        let _lock = exclusive_file(&self.path.with_extension("json.lock"))?;
        ensure!(
            self.load()?.1.as_deref() == expected,
            "O registro de recuperação mudou. Releia antes de continuar."
        );
        let bytes = serde_json::to_vec_pretty(journal)?;
        ensure!(
            bytes.len() as u64 <= MAX_JOURNAL,
            "Registro de recuperação excede o limite."
        );
        atomic_write_checked(&self.path, &bytes, true, expected)?;
        Ok(revision(&bytes))
    }
    pub fn clear(&self, expected: Option<&str>) -> Result<()> {
        let _lock = exclusive_file(&self.path.with_extension("json.lock"))?;
        ensure!(
            self.load()?.1.as_deref() == expected,
            "O registro de recuperação mudou. A configuração foi preservada."
        );
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}
