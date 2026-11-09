use netlane_core::{Adapter, Connection, NetworkSnapshot, RouteMode, RoutingPolicy};
pub fn snapshot() -> NetworkSnapshot {
    NetworkSnapshot {
        adapters: vec![
            Adapter {
                id: "aa6f3b88-072a-45b6-b0cb-8afe9567e62f".to_owned(),
                name: "Ethernet".to_owned(),
                kind: "Ethernet".to_owned(),
                addresses: vec!["192.0.2.10".to_owned()],
                connected: true,
                has_gateway: true,
                luid: 1,
                ipv4: true,
                received: Some(245_000_000),
                sent: Some(34_000_000),
                ..Default::default()
            },
            Adapter {
                id: "d3ae43d2-8203-41d6-9d7f-0b6fa59518fd".to_owned(),
                name: "Wi-Fi".to_owned(),
                kind: "Wireless80211".to_owned(),
                addresses: vec!["198.51.100.20".to_owned()],
                connected: true,
                has_gateway: true,
                luid: 2,
                ipv4: true,
                received: Some(520_000_000),
                sent: Some(72_000_000),
                ..Default::default()
            },
        ],
        connections: vec![
            Connection {
                pid: 4100,
                name: "Navegador.exe".to_owned(),
                executable: Some("C:\\Demo\\Navegador.exe".to_owned()),
                protocol: "TCP".to_owned(),
                local_address: "192.0.2.10".to_owned(),
                local_port: 50120,
                remote_address: "203.0.113.10".to_owned(),
                remote_port: 443,
                interface: Some("Ethernet".to_owned()),
            },
            Connection {
                pid: 6200,
                name: "Sincronização.exe".to_owned(),
                executable: Some("C:\\Demo\\Sincronização.exe".to_owned()),
                protocol: "TCP".to_owned(),
                local_address: "198.51.100.20".to_owned(),
                local_port: 50130,
                remote_address: "203.0.113.20".to_owned(),
                remote_port: 443,
                interface: Some("Wi-Fi".to_owned()),
            },
        ],
        error: None,
    }
}
pub fn policies() -> Vec<RoutingPolicy> {
    let mut ethernet = RoutingPolicy::new("C:\\Demo\\Navegador.exe");
    ethernet.route_mode = RouteMode::Ethernet;
    ethernet.interface_id = Some("aa6f3b88-072a-45b6-b0cb-8afe9567e62f".to_owned());
    let mut wifi = RoutingPolicy::new("C:\\Demo\\Sincronização.exe");
    wifi.route_mode = RouteMode::WiFi;
    wifi.interface_id = Some("d3ae43d2-8203-41d6-9d7f-0b6fa59518fd".to_owned());
    vec![ethernet, wifi]
}
/// Two fictitious profiles: the demo rules and a copy where everything is Automático.
pub fn profiles(current: &[RoutingPolicy]) -> netlane_core::profiles::Profiles {
    use netlane_core::profiles::{DEFAULT_PROFILE, Profile, Profiles};
    let automatic = current
        .iter()
        .cloned()
        .map(|mut p| {
            p.route_mode = RouteMode::Automatic;
            p.interface_id = None;
            p
        })
        .collect();
    Profiles {
        active: DEFAULT_PROFILE.to_owned(),
        profiles: vec![
            Profile {
                name: DEFAULT_PROFILE.to_owned(),
                policies: current.to_vec(),
            },
            Profile {
                name: "Tudo automático".to_owned(),
                policies: automatic,
            },
        ],
    }
}
/// Fictitious data usage; larger periods scale the same mix.
pub fn usage(period: netlane_core::usage::Period) -> Vec<netlane_core::usage::NetworkUsage> {
    use netlane_core::usage::{AppBytes, NetworkKind, NetworkUsage, Period};
    let scale = match period {
        Period::Today => 1,
        Period::Week => 6,
        Period::Month => 23,
    };
    let app = |id: &str, path: Option<&str>, received: u64, sent: u64| AppBytes {
        id: id.to_owned(),
        path: path.map(str::to_owned),
        received: received * scale,
        sent: sent * scale,
    };
    vec![
        NetworkUsage {
            name: "Rede cabeada".to_owned(),
            kind: NetworkKind::Wired,
            apps: vec![
                app(
                    r"\device\harddiskvolume3\demo\navegador.exe",
                    Some(r"C:\Demo\Navegador.exe"),
                    3_840_000_000,
                    212_000_000,
                ),
                app(
                    r"\device\harddiskvolume3\demo\jogos.exe",
                    Some(r"C:\Demo\Jogos.exe"),
                    11_200_000_000,
                    96_000_000,
                ),
                app("System", None, 42_000_000, 18_000_000),
                app("", None, 156_000_000, 21_000_000),
            ],
        },
        NetworkUsage {
            name: "Casa 5G".to_owned(),
            kind: NetworkKind::WiFi,
            apps: vec![
                app(
                    r"\device\harddiskvolume3\demo\sincronização.exe",
                    Some(r"C:\Demo\Sincronização.exe"),
                    1_260_000_000,
                    2_480_000_000,
                ),
                app(
                    r"\device\harddiskvolume3\demo\navegador.exe",
                    Some(r"C:\Demo\Navegador.exe"),
                    310_000_000,
                    24_000_000,
                ),
                app("Demo.Mensagens_0demo0000000", None, 188_000_000, 61_000_000),
            ],
        },
    ]
}
