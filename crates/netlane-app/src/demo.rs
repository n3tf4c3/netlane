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
