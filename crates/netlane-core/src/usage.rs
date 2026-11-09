//! Data usage per network and per app as recorded by Windows (the same source as
//! Settings → Network & internet → Data usage). Windows keeps the history, so the
//! totals cover the whole period even when NetLane was closed.

/// Network type of a Windows connection profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetworkKind {
    Wired,
    WiFi,
    Mobile,
}
impl NetworkKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Wired => "Ethernet",
            Self::WiFi => "Wi-Fi",
            Self::Mobile => "Celular",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Period {
    #[default]
    Today,
    Week,
    Month,
}
impl Period {
    pub const ALL: [Self; 3] = [Self::Today, Self::Week, Self::Month];
    /// Calendar days covered, ending today.
    pub fn days(self) -> u32 {
        match self {
            Self::Today => 1,
            Self::Week => 7,
            Self::Month => 30,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Hoje",
            Self::Week => "7 dias",
            Self::Month => "30 dias",
        }
    }
}

/// One app as attributed by Windows: an NT image path for desktop programs, a
/// package family name for Store apps, `System…` for the kernel or empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppBytes {
    pub id: String,
    /// Drive-letter path of a desktop program, when the volume could be mapped.
    pub path: Option<String>,
    pub received: u64,
    pub sent: u64,
}

/// One Windows connection profile (an Ethernet network, a Wi-Fi SSID…).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkUsage {
    pub name: String,
    pub kind: NetworkKind,
    pub apps: Vec<AppBytes>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bytes {
    pub received: u64,
    pub sent: u64,
}
impl Bytes {
    pub fn total(self) -> u64 {
        self.received.saturating_add(self.sent)
    }
    fn add(&mut self, received: u64, sent: u64) {
        self.received = self.received.saturating_add(received);
        self.sent = self.sent.saturating_add(sent);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkTotal {
    pub name: String,
    pub kind: NetworkKind,
    pub bytes: Bytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppTotal {
    pub name: String,
    /// Full path or package name, for the tooltip/detail line.
    pub detail: String,
    pub path: Option<String>,
    pub bytes: Bytes,
    /// Total bytes per network type, largest first.
    pub by_kind: Vec<(NetworkKind, u64)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub total: Bytes,
    /// Largest first.
    pub kinds: Vec<(NetworkKind, Bytes)>,
    /// Largest first; networks without traffic in the period are left out.
    pub networks: Vec<NetworkTotal>,
    /// Largest first; the same app on several networks is one entry.
    pub apps: Vec<AppTotal>,
}

const SYSTEM: &str = "system";

/// Merge key: Windows reports the kernel under `System` and `System\<detail>`.
fn app_key(id: &str) -> String {
    let id = id.trim().to_lowercase();
    if id == SYSTEM || id.starts_with("system\\") {
        SYSTEM.into()
    } else {
        id
    }
}

/// Short name shown in the list.
pub fn app_name(id: &str) -> String {
    let id = id.trim();
    match app_key(id).as_str() {
        "" => "Não identificado".into(),
        SYSTEM => "Windows (sistema)".into(),
        _ if id.contains('\\') => id.rsplit('\\').next().unwrap_or(id).to_owned(),
        // Store apps: package family name without the publisher hash.
        _ => id.rsplit_once('_').map_or(id, |(name, _)| name).to_owned(),
    }
}

pub fn summarize(networks: &[NetworkUsage]) -> Summary {
    let mut summary = Summary::default();
    // (key, total, per-kind)
    let mut apps: Vec<(String, AppTotal)> = Vec::new();
    for network in networks {
        let mut bytes = Bytes::default();
        for app in &network.apps {
            bytes.add(app.received, app.sent);
            let key = app_key(&app.id);
            let index = match apps.iter().position(|(k, _)| *k == key) {
                Some(index) => index,
                None => {
                    let detail = match key.as_str() {
                        "" => "Tráfego que o Windows não associou a um programa.".into(),
                        SYSTEM => "Núcleo do Windows (System).".into(),
                        _ => app.path.clone().unwrap_or_else(|| app.id.trim().to_owned()),
                    };
                    apps.push((
                        key.clone(),
                        AppTotal {
                            name: app_name(&app.id),
                            detail,
                            path: app.path.clone(),
                            bytes: Bytes::default(),
                            by_kind: Vec::new(),
                        },
                    ));
                    apps.len() - 1
                }
            };
            let total = &mut apps[index].1;
            total.bytes.add(app.received, app.sent);
            let amount = app.received.saturating_add(app.sent);
            match total.by_kind.iter_mut().find(|(k, _)| *k == network.kind) {
                Some((_, value)) => *value = value.saturating_add(amount),
                None => total.by_kind.push((network.kind, amount)),
            }
        }
        if bytes.total() == 0 {
            continue;
        }
        summary.total.add(bytes.received, bytes.sent);
        match summary.kinds.iter_mut().find(|(k, _)| *k == network.kind) {
            Some((_, value)) => value.add(bytes.received, bytes.sent),
            None => summary.kinds.push((network.kind, bytes)),
        }
        summary.networks.push(NetworkTotal {
            name: network.name.clone(),
            kind: network.kind,
            bytes,
        });
    }
    summary
        .kinds
        .sort_by(|a, b| b.1.total().cmp(&a.1.total()).then(a.0.cmp(&b.0)));
    summary.networks.sort_by(|a, b| {
        b.bytes
            .total()
            .cmp(&a.bytes.total())
            .then(a.name.cmp(&b.name))
    });
    summary.apps = apps
        .into_iter()
        .map(|(_, mut app)| {
            app.by_kind.retain(|(_, v)| *v > 0);
            app.by_kind
                .sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            app
        })
        .filter(|app| app.bytes.total() > 0)
        .collect();
    summary.apps.sort_by(|a, b| {
        b.bytes
            .total()
            .cmp(&a.bytes.total())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    summary
}

/// Decimal units as Windows shows them, with a Brazilian decimal comma.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000. && unit < UNITS.len() - 1 {
        value /= 1000.;
        unit += 1;
    }
    // 999.96 would print as "1000,0": promote to the next unit instead.
    if value >= 999.95 && unit < UNITS.len() - 1 {
        value /= 1000.;
        unit += 1;
    }
    let text = if value >= 100. {
        format!("{value:.0}")
    } else {
        format!("{value:.1}").replace('.', ",")
    };
    format!("{text} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, received: u64, sent: u64) -> AppBytes {
        AppBytes {
            id: id.into(),
            path: None,
            received,
            sent,
        }
    }

    #[test]
    fn apps_merge_across_networks_and_keep_the_split_per_network_type() {
        let steam = "\\device\\harddiskvolume3\\games\\steam\\steam.exe";
        let networks = [
            NetworkUsage {
                name: "Rede 2".into(),
                kind: NetworkKind::Wired,
                apps: vec![
                    AppBytes {
                        path: Some("D:\\games\\steam\\steam.exe".into()),
                        ..app(steam, 20_000, 1_000)
                    },
                    app("System", 10, 5),
                    app("System\\IPv6 Control Message", 0, 5),
                    app("", 300, 0),
                ],
            },
            NetworkUsage {
                name: "Casa 5G".into(),
                kind: NetworkKind::WiFi,
                apps: vec![
                    // Same program, different letter case from another network.
                    app(&steam.to_uppercase(), 4_000, 0),
                    app("Microsoft.WindowsStore_8wekyb3d8bbwe", 7_000, 100),
                ],
            },
            NetworkUsage {
                name: "Sem uso".into(),
                kind: NetworkKind::Mobile,
                apps: vec![app("x.exe", 0, 0)],
            },
        ];
        let summary = summarize(&networks);
        assert_eq!(summary.total.total(), 32_420);
        assert_eq!(
            summary
                .networks
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            ["Rede 2", "Casa 5G"],
            "Redes sem tráfego ficam de fora."
        );
        assert_eq!(
            summary.kinds,
            [
                (
                    NetworkKind::Wired,
                    Bytes {
                        received: 20_310,
                        sent: 1_010
                    }
                ),
                (
                    NetworkKind::WiFi,
                    Bytes {
                        received: 11_000,
                        sent: 100
                    }
                )
            ]
        );
        let names = summary
            .apps
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "steam.exe",
                "Microsoft.WindowsStore",
                "Não identificado",
                "Windows (sistema)"
            ]
        );
        let steam = &summary.apps[0];
        assert_eq!(steam.bytes.total(), 25_000);
        assert_eq!(steam.detail, "D:\\games\\steam\\steam.exe");
        assert_eq!(
            steam.by_kind,
            [(NetworkKind::Wired, 21_000), (NetworkKind::WiFi, 4_000)]
        );
        assert_eq!(
            summary.apps[3].bytes.total(),
            20,
            "System e System\\… somam."
        );
    }

    #[test]
    fn sizes_use_decimal_units_and_a_decimal_comma() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1_000), "1,0 KB");
        assert_eq!(format_bytes(42_300_000_000), "42,3 GB");
        assert_eq!(format_bytes(523_400_000), "523 MB");
        assert_eq!(format_bytes(999_960), "1,0 MB");
        assert_eq!(format_bytes(u64::MAX), "18446744 TB");
    }
}
