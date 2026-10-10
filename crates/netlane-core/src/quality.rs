//! Manual quality sessions. No network I/O; monotonic windows and generation guards.
use crate::{
    Adapter,
    persistence::{atomic_write, read_bounded, without_bom},
};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::Ipv4Addr,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use url::{Host, Url};

pub const INTERVAL: Duration = Duration::from_secs(5);
pub const MAX_AGE: Duration = Duration::from_secs(60);
const WORKERS: usize = 4;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Icmp,
    Https,
}
impl Serialize for Mode {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_u8(if *self == Self::Icmp { 0 } else { 1 })
    }
}
impl<'de> Deserialize<'de> for Mode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        match v
            .as_str()
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| v.to_string())
            .as_str()
        {
            "icmp" | "0" => Ok(Self::Icmp),
            "https" | "1" => Ok(Self::Https),
            _ => Err(serde::de::Error::custom("Tipo de teste inválido.")),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub mode: Mode,
    pub host: String,
    pub label: String,
    pub port: u16,
    pub request_path: String,
}
impl Target {
    pub fn parse(mode: Mode, text: &str) -> Result<Self> {
        let text = text.trim();
        ensure!(
            !text.is_empty() && text.len() <= 512 && !text.chars().any(char::is_control),
            "Informe um destino de até 512 caracteres, sem controles."
        );
        if mode == Mode::Icmp {
            let address: Ipv4Addr = text
                .parse()
                .map_err(|_| anyhow::anyhow!("Para ping, informe um IPv4 válido, como 1.1.1.1."))?;
            ensure!(
                !address.is_unspecified() && address.octets()[0] < 224,
                "Informe um IPv4 unicast válido."
            );
            return Ok(Self {
                mode,
                host: address.to_string(),
                label: address.to_string(),
                port: 0,
                request_path: String::new(),
            });
        }
        let url = Url::parse(&if text.contains("://") {
            text.to_owned()
        } else {
            format!("https://{text}")
        })?;
        ensure!(
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
                && url.port_or_known_default().is_some_and(|p| p > 0),
            "Informe um site HTTPS sem credenciais ou fragmento."
        );
        let host = match url.host() {
            Some(Host::Domain(s)) => s.to_owned(),
            Some(Host::Ipv4(a)) => a.to_string(),
            _ => bail!("O destino HTTPS deve ter nome ou IPv4."),
        };
        ensure!(
            !host.is_empty() && url.as_str().len() <= 512,
            "Informe um site de até 512 caracteres após normalização."
        );
        let request_path = match url.query() {
            Some(q) => format!("{}?{q}", url.path()),
            None => url.path().to_owned(),
        };
        Ok(Self {
            mode,
            host,
            label: url.to_string(),
            port: url.port_or_known_default().unwrap(),
            request_path,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(rename = "Mode", alias = "mode")]
    pub mode: Mode,
    #[serde(rename = "Target", alias = "target")]
    pub target: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Icmp,
            target: "1.1.1.1".into(),
        }
    }
}
impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        let value: Self = match read_bounded(path, 4096)? {
            Some(bytes) => serde_json::from_slice(without_bom(&bytes))?,
            None => Self::default(),
        };
        Target::parse(value.mode, &value.target)?;
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        Target::parse(self.mode, &self.target)?;
        atomic_write(path, &serde_json::to_vec_pretty(self)?, true)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Success,
    NoResponse,
    Unavailable,
    Error,
}
#[derive(Clone, Debug)]
pub struct ProbeResult {
    pub outcome: Outcome,
    pub milliseconds: Option<f64>,
    pub detail: String,
    pub source: String,
    pub index: u32,
}
impl ProbeResult {
    pub fn new(outcome: Outcome, milliseconds: Option<f64>, detail: impl Into<String>) -> Self {
        Self {
            outcome,
            milliseconds,
            detail: detail.into(),
            source: String::new(),
            index: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rating {
    #[default]
    Unknown,
    Collecting,
    Good,
    Fair,
    Poor,
    NoResponse,
}
impl Rating {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Aguardando amostras",
            Self::Collecting => "Medindo",
            Self::Good => "Boa",
            Self::Fair => "Atenção",
            Self::Poor => "Ruim",
            Self::NoResponse => "Sem resposta",
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub samples: usize,
    pub replies: usize,
    pub latency: Option<f64>,
    pub jitter: Option<f64>,
    pub failure_percent: Option<f64>,
    pub rating: Rating,
}
#[derive(Default)]
pub struct Window {
    samples: VecDeque<(Duration, Option<f64>)>,
}
impl Window {
    pub fn add(&mut self, now: Duration, result: &ProbeResult) {
        match result.outcome {
            Outcome::Success
                if result
                    .milliseconds
                    .is_some_and(|v| v.is_finite() && v >= 0.) =>
            {
                self.samples.push_back((now, result.milliseconds))
            }
            Outcome::NoResponse => self.samples.push_back((now, None)),
            _ => self.samples.clear(),
        }
        while self.samples.len() > 12 {
            self.samples.pop_front();
        }
        self.expire(now);
    }
    fn expire(&mut self, now: Duration) {
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| now.saturating_sub(*at) >= MAX_AGE)
        {
            self.samples.pop_front();
        }
    }
    pub fn summarize(&mut self, now: Duration, mode: Mode) -> Summary {
        self.expire(now);
        let samples = self.samples.len();
        let replies = self
            .samples
            .iter()
            .filter_map(|(_, v)| *v)
            .collect::<Vec<_>>();
        let differences = self
            .samples
            .iter()
            .zip(self.samples.iter().skip(1))
            .filter_map(|((_, a), (_, b))| Some((a.as_ref()? - b.as_ref()?).abs()))
            .collect::<Vec<_>>();
        let latency =
            (!replies.is_empty()).then(|| replies.iter().sum::<f64>() / replies.len() as f64);
        let jitter = (!differences.is_empty())
            .then(|| differences.iter().sum::<f64>() / differences.len() as f64);
        let failure_percent =
            (samples > 0).then(|| 100. * (samples - replies.len()) as f64 / samples as f64);
        let (fair_latency, poor_latency, fair_jitter, poor_jitter) = if mode == Mode::Icmp {
            (80., 150., 15., 40.)
        } else {
            (500., 1500., 150., 400.)
        };
        let rating = if samples == 0 {
            Rating::Unknown
        } else if samples < 5 {
            Rating::Collecting
        } else if replies.is_empty() {
            Rating::NoResponse
        } else if failure_percent >= Some(20.)
            || latency >= Some(poor_latency)
            || jitter >= Some(poor_jitter)
        {
            Rating::Poor
        } else if failure_percent > Some(0.)
            || latency >= Some(fair_latency)
            || jitter >= Some(fair_jitter)
        {
            Rating::Fair
        } else {
            Rating::Good
        };
        Summary {
            samples,
            replies: replies.len(),
            latency,
            jitter,
            failure_percent,
            rating,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    id: uuid::Uuid,
    luid: u64,
    address: Ipv4Addr,
}
impl Identity {
    fn of(a: &Adapter) -> Option<Self> {
        (a.connected && a.ipv4 && a.luid > 0).then_some(())?;
        Some(Self {
            id: uuid::Uuid::parse_str(a.id.trim_matches(['{', '}'])).ok()?,
            luid: a.luid,
            address: a.addresses.iter().find_map(|s| {
                s.parse::<Ipv4Addr>()
                    .ok()
                    .filter(|a| !a.is_unspecified() && !a.is_loopback() && a.octets()[0] < 224)
            })?,
        })
    }
}
pub struct Row {
    identity: Option<Identity>,
    revision: u64,
    ticket: Option<u64>,
    checked: Option<Duration>,
    window: Window,
    pub last: Option<(Duration, ProbeResult)>,
    pub summary: Summary,
    pub state: String,
}
impl Row {
    fn new(identity: Option<Identity>, revision: u64) -> Self {
        let state = if identity.is_some() {
            "Aguardando amostras"
        } else {
            "Indisponível"
        }
        .into();
        Self {
            identity,
            revision,
            ticket: None,
            checked: None,
            window: Window::default(),
            last: None,
            summary: Summary::default(),
            state,
        }
    }
    pub fn label(&self) -> String {
        if !self.state.is_empty() {
            self.state.clone()
        } else if self.summary.rating == Rating::Collecting {
            format!("Medindo · {}/5", self.summary.samples)
        } else {
            self.summary.rating.label().into()
        }
    }
    pub fn latency_label(&self) -> String {
        if self.state.is_empty()
            && self.summary.samples > 0
            && let Some((_, result)) = &self.last
            && result.outcome == Outcome::Success
        {
            return format!("{:.0} ms", result.milliseconds.unwrap_or(0.));
        }
        "— ms".into()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Preparing,
    Active,
}
pub struct Request {
    pub adapter: Adapter,
    pub target: Target,
    pub destination: Ipv4Addr,
    pub cancel: Arc<AtomicBool>,
    generation: u64,
    ticket: u64,
    revision: u64,
}
#[derive(Default)]
pub struct Monitor {
    pub phase: Phase,
    pub target: Option<Target>,
    pub destination: Option<Ipv4Addr>,
    pub rows: HashMap<String, Row>,
    generation: u64,
    clock: u64,
    active: HashSet<u64>,
    cancel: Arc<AtomicBool>,
}
impl Monitor {
    pub fn begin(&mut self, target: Target) -> u64 {
        self.stop();
        self.phase = Phase::Preparing;
        self.target = Some(target);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.generation
    }
    pub fn cancellation(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }
    pub fn activate(&mut self, generation: u64, destination: Ipv4Addr) -> bool {
        if generation != self.generation
            || self.phase != Phase::Preparing
            || self.cancel.load(Ordering::Relaxed)
        {
            return false;
        }
        self.destination = Some(destination);
        self.phase = Phase::Active;
        true
    }
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.generation = self.generation.wrapping_add(1);
        self.phase = Phase::Idle;
        self.target = None;
        self.destination = None;
        self.rows.clear();
    }
    pub fn sync(&mut self, adapters: &[Adapter], selected: &[String], now: Duration) {
        if self.phase != Phase::Active {
            return;
        }
        let visible = adapters
            .iter()
            .filter(|a| selected.contains(&a.id))
            .take(128)
            .collect::<Vec<_>>();
        self.rows
            .retain(|id, _| visible.iter().any(|a| &a.id == id));
        for a in visible {
            let identity = Identity::of(a);
            if self.rows.get(&a.id).is_none_or(|r| r.identity != identity) {
                self.clock += 1;
                self.rows
                    .insert(a.id.clone(), Row::new(identity, self.clock));
            }
            let row = self.rows.get_mut(&a.id).unwrap();
            row.summary = row
                .window
                .summarize(now, self.target.as_ref().unwrap().mode);
            if row.state.is_empty() && row.summary.samples == 0 {
                row.state = "Aguardando amostras".into();
            }
        }
    }
    pub fn prepare(
        &mut self,
        adapters: &[Adapter],
        selected: &[String],
        now: Duration,
    ) -> Vec<Request> {
        self.sync(adapters, selected, now);
        if self.phase != Phase::Active {
            return vec![];
        }
        let mut requests = Vec::new();
        let mut ordered = adapters.iter().collect::<Vec<_>>();
        // Give interfaces without samples priority, then the oldest scheduled one.
        ordered.sort_by_key(|a| self.rows.get(&a.id).and_then(|row| row.checked));
        for a in ordered {
            if self.active.len() >= WORKERS {
                break;
            }
            let Some(row) = self.rows.get_mut(&a.id) else {
                continue;
            };
            if row.identity.is_none()
                || row.ticket.is_some()
                || row
                    .checked
                    .is_some_and(|last| now.saturating_sub(last) < INTERVAL)
            {
                continue;
            }
            self.clock += 1;
            let ticket = self.clock;
            row.ticket = Some(ticket);
            row.checked = Some(now);
            self.active.insert(ticket);
            requests.push(Request {
                adapter: a.clone(),
                target: self.target.clone().unwrap(),
                destination: self.destination.unwrap(),
                cancel: self.cancel.clone(),
                generation: self.generation,
                ticket,
                revision: row.revision,
            });
        }
        requests
    }
    pub fn complete(&mut self, request: Request, result: ProbeResult, now: Duration) -> bool {
        if !self.active.remove(&request.ticket)
            || self.phase != Phase::Active
            || request.generation != self.generation
        {
            return false;
        }
        let Some(row) = self.rows.get_mut(&request.adapter.id) else {
            return false;
        };
        if row.revision != request.revision || row.ticket != Some(request.ticket) {
            return false;
        }
        row.ticket = None;
        row.state = match result.outcome {
            Outcome::Unavailable => "Indisponível",
            Outcome::Error => "Erro no teste",
            _ => "",
        }
        .into();
        if result.outcome == Outcome::Success
            && !result
                .milliseconds
                .is_some_and(|v| v.is_finite() && v >= 0.)
        {
            row.state = "Erro no teste".into();
        }
        row.window.add(now, &result);
        row.summary = row.window.summarize(now, request.target.mode);
        row.last = Some((now, result));
        true
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn success(ms: f64) -> ProbeResult {
        ProbeResult::new(Outcome::Success, Some(ms), "fixture")
    }
    fn adapter() -> Adapter {
        Adapter {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Fixture".into(),
            kind: "Ethernet".into(),
            addresses: vec!["192.0.2.3".into()],
            connected: true,
            has_gateway: true,
            luid: 4,
            ipv4: true,
            ipv6: false,
            received: None,
            sent: None,
        }
    }
    fn start(m: &mut Monitor) {
        let g = m.begin(Target::parse(Mode::Icmp, "1.1.1.1").unwrap());
        assert!(m.activate(g, "1.1.1.1".parse().unwrap()));
    }
    #[test]
    fn targets_and_legacy_preferences_validate_without_io() {
        for bad in [
            "",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "example.com",
        ] {
            assert!(Target::parse(Mode::Icmp, bad).is_err());
        }
        for bad in [
            "http://example.com",
            "https://user:pass@example.com",
            "https://example.com/#frag",
            "https://[::1]",
            "https://example.com\r\nX:bad",
        ] {
            assert!(Target::parse(Mode::Https, bad).is_err());
        }
        let t = Target::parse(Mode::Https, "example.com:8443/a?q=1").unwrap();
        assert_eq!(t.port, 8443);
        assert_eq!(t.request_path, "/a?q=1");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quality-settings.json");
        std::fs::write(&path, r#"{"Mode":1,"Target":"https://example.com/"}"#).unwrap();
        let settings = Settings::load(&path).unwrap();
        assert_eq!(settings.mode, Mode::Https);
        settings.save(&path).unwrap();
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(written["Mode"], 1);
        assert_eq!(Settings::load(&path).unwrap(), settings);
        std::fs::write(&path, vec![b'x'; 4097]).unwrap();
        assert!(Settings::load(&path).is_err());
    }
    #[test]
    fn classification_thresholds_are_mode_specific_and_need_five_attempts() {
        let mut w = Window::default();
        for n in 0..4 {
            w.add(Duration::from_secs(n), &success(80.));
        }
        assert_eq!(
            w.summarize(Duration::from_secs(4), Mode::Icmp).rating,
            Rating::Collecting
        );
        w.add(Duration::from_secs(4), &success(80.));
        assert_eq!(
            w.summarize(Duration::from_secs(4), Mode::Icmp).rating,
            Rating::Fair
        );
        assert_eq!(
            w.summarize(Duration::from_secs(4), Mode::Https).rating,
            Rating::Good
        );
        w.add(
            Duration::from_secs(5),
            &ProbeResult::new(Outcome::NoResponse, None, "filtered"),
        );
        assert_eq!(
            w.summarize(Duration::from_secs(5), Mode::Icmp).rating,
            Rating::Fair
        );
        w.add(
            Duration::from_secs(6),
            &ProbeResult::new(Outcome::NoResponse, None, "filtered"),
        );
        assert_eq!(
            w.summarize(Duration::from_secs(6), Mode::Icmp).rating,
            Rating::Poor
        );
        let mut none = Window::default();
        for n in 0..5 {
            none.add(
                Duration::from_secs(n),
                &ProbeResult::new(Outcome::NoResponse, None, "timeout"),
            );
        }
        assert_eq!(
            none.summarize(Duration::from_secs(4), Mode::Https).rating,
            Rating::NoResponse
        );
    }
    #[test]
    fn window_bounds_expiry_jitter_gaps_and_local_errors() {
        let mut w = Window::default();
        for (n, v) in [Some(10.), None, Some(100.), Some(110.)]
            .into_iter()
            .enumerate()
        {
            w.add(
                Duration::from_secs(n as u64),
                &v.map_or_else(
                    || ProbeResult::new(Outcome::NoResponse, None, "timeout"),
                    success,
                ),
            );
        }
        assert_eq!(
            w.summarize(Duration::from_secs(3), Mode::Icmp).jitter,
            Some(10.)
        );
        for n in 4..30 {
            w.add(Duration::from_secs(n), &success(20.));
        }
        assert_eq!(w.summarize(Duration::from_secs(30), Mode::Icmp).samples, 12);
        assert_eq!(w.summarize(Duration::from_secs(89), Mode::Icmp).samples, 0);
        w.add(Duration::from_secs(90), &success(20.));
        w.add(
            Duration::from_secs(91),
            &ProbeResult::new(Outcome::Error, None, "local"),
        );
        assert_eq!(w.summarize(Duration::from_secs(91), Mode::Icmp).samples, 0);
        w.add(Duration::from_secs(92), &success(f64::NAN));
        assert_eq!(w.summarize(Duration::from_secs(92), Mode::Icmp).samples, 0);
    }
    #[test]
    fn stopped_or_restarted_sessions_ignore_dns_and_samples_and_cancel_work() {
        let mut m = Monitor::default();
        let a = adapter();
        let selected = vec![a.id.clone()];
        start(&mut m);
        let request = m
            .prepare(std::slice::from_ref(&a), &selected, Duration::ZERO)
            .pop()
            .unwrap();
        let cancellation = request.cancel.clone();
        m.stop();
        assert!(cancellation.load(Ordering::Relaxed));
        start(&mut m);
        assert!(!m.complete(request, success(10.), Duration::from_secs(1)));
        assert!(!m.activate(0, "1.1.1.1".parse().unwrap()));
        assert!(m.rows.is_empty());
    }
    #[test]
    fn changed_ip_deselection_and_reselection_invalidate_inflight_and_limit_workers() {
        let mut m = Monitor::default();
        let mut a = adapter();
        let selected = vec![a.id.clone()];
        start(&mut m);
        let request = m
            .prepare(std::slice::from_ref(&a), &selected, Duration::ZERO)
            .pop()
            .unwrap();
        a.addresses = vec!["192.0.2.4".into()];
        m.sync(std::slice::from_ref(&a), &selected, Duration::from_secs(1));
        assert!(!m.complete(request, success(10.), Duration::from_secs(1)));
        let request = m
            .prepare(std::slice::from_ref(&a), &selected, Duration::from_secs(1))
            .pop()
            .unwrap();
        m.sync(std::slice::from_ref(&a), &[], Duration::from_secs(2));
        m.sync(std::slice::from_ref(&a), &selected, Duration::from_secs(2));
        assert!(!m.complete(request, success(10.), Duration::from_secs(2)));
        let many = (0..10).map(|_| adapter()).collect::<Vec<_>>();
        let all = many.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
        assert_eq!(
            m.prepare(&many, &all, Duration::from_secs(3)).len(),
            WORKERS
        );
        assert!(m.prepare(&many, &all, Duration::from_secs(4)).is_empty());
    }
    #[test]
    fn scheduling_is_fair_and_cancelled_workers_still_count_until_completion() {
        let mut m = Monitor::default();
        start(&mut m);
        let adapters = (0..10).map(|_| adapter()).collect::<Vec<_>>();
        let selected = adapters.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
        let first = m.prepare(&adapters, &selected, Duration::ZERO);
        assert_eq!(first.len(), 4);
        m.stop();
        start(&mut m);
        assert!(
            m.prepare(&adapters, &selected, Duration::from_secs(1))
                .is_empty()
        );
        for request in first {
            assert!(!m.complete(request, success(10.), Duration::from_secs(1)));
        }
        let first = m.prepare(&adapters, &selected, Duration::from_secs(1));
        for request in first {
            m.complete(request, success(10.), Duration::from_secs(4));
        }
        let second = m.prepare(&adapters, &selected, Duration::from_secs(4));
        assert!(
            second
                .iter()
                .all(|r| adapters[4..8].iter().any(|a| a.id == r.adapter.id))
        );
        for request in second {
            m.complete(request, success(10.), Duration::from_secs(7));
        }
        let third = m.prepare(&adapters, &selected, Duration::from_secs(7));
        assert!(
            adapters[8..]
                .iter()
                .all(|a| third.iter().any(|r| r.adapter.id == a.id))
        );
    }
}
