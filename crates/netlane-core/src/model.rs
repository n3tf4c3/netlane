use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum RouteMode {
    #[default]
    Automatic,
    Ethernet,
    WiFi,
    Blocked,
}
impl RouteMode {
    pub const ALL: [Self; 4] = [Self::Automatic, Self::Ethernet, Self::WiFi, Self::Blocked];
    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automático",
            Self::Ethernet => "Ethernet",
            Self::WiFi => "Wi-Fi",
            Self::Blocked => "Bloqueado",
        }
    }
}
impl<'de> Deserialize<'de> for RouteMode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        let text = match &value {
            Value::String(s) => s.to_ascii_lowercase(),
            Value::Number(n) => n.to_string(),
            _ => String::new(),
        };
        match text.as_str() {
            "automatic" | "0" => Ok(Self::Automatic),
            "ethernet" | "1" => Ok(Self::Ethernet),
            "wifi" | "2" => Ok(Self::WiFi),
            "blocked" | "3" => Ok(Self::Blocked),
            _ => Err(serde::de::Error::custom("Modo de conexão inválido.")),
        }
    }
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingPolicy {
    pub application_id: String,
    pub executable_path: Option<String>,
    pub interface_id: Option<String>,
    pub interface_type_hint: Option<String>,
    pub route_mode: RouteMode,
    pub fallback_interface_id: Option<String>,
    pub enabled: bool,
    pub include_related_executables: bool,
    #[serde(flatten)]
    pub additional_data: BTreeMap<String, Value>,
}
impl RoutingPolicy {
    pub fn new(path: &str) -> Self {
        Self {
            application_id: file_name(path).to_owned(),
            executable_path: Some(path.to_owned()),
            interface_id: None,
            interface_type_hint: None,
            route_mode: RouteMode::Automatic,
            fallback_interface_id: None,
            enabled: true,
            include_related_executables: true,
            additional_data: BTreeMap::new(),
        }
    }
}
impl<'de> Deserialize<'de> for RoutingPolicy {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            #[serde(default)]
            application_id: String,
            #[serde(default)]
            executable_path: Option<String>,
            #[serde(default)]
            interface_id: Option<String>,
            #[serde(default)]
            interface_type_hint: Option<String>,
            #[serde(default)]
            route_mode: RouteMode,
            #[serde(default)]
            fallback_interface_id: Option<String>,
            #[serde(default = "yes")]
            enabled: bool,
            #[serde(default = "yes")]
            include_related_executables: bool,
            #[serde(flatten)]
            additional_data: BTreeMap<String, Value>,
        }
        let mut value = Value::deserialize(d)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| serde::de::Error::custom("Regra inválida."))?;
        // JsonSerializerDefaults.Web is case insensitive; retain fields from newer versions.
        for canonical in [
            "applicationId",
            "executablePath",
            "interfaceId",
            "interfaceTypeHint",
            "routeMode",
            "fallbackInterfaceId",
            "enabled",
            "includeRelatedExecutables",
        ] {
            let keys: Vec<_> = object
                .keys()
                .filter(|k| k.eq_ignore_ascii_case(canonical))
                .cloned()
                .collect();
            if keys.len() > 1 {
                return Err(serde::de::Error::custom("Campo de regra duplicado."));
            }
            if let Some(key) = keys.first()
                && key != canonical
            {
                let v = object.remove(key).expect("existing key");
                object.insert(canonical.to_owned(), v);
            }
        }
        let w: Wire = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            application_id: w.application_id,
            executable_path: w.executable_path,
            interface_id: w.interface_id,
            interface_type_hint: w.interface_type_hint,
            route_mode: w.route_mode,
            fallback_interface_id: w.fallback_interface_id,
            enabled: w.enabled,
            include_related_executables: w.include_related_executables,
            additional_data: w.additional_data,
        })
    }
}
/// Rules per file; keeps a full session snapshot inside one control-channel frame.
pub const MAX_POLICIES: usize = 4096;
pub fn validate_policies(policies: &[RoutingPolicy]) -> Result<()> {
    if policies.len() > MAX_POLICIES {
        bail!("Limite de {MAX_POLICIES} regras excedido.");
    }
    let mut names = std::collections::HashSet::new();
    for p in policies {
        if p.application_id.trim().is_empty() {
            bail!("Toda regra deve identificar um aplicativo.");
        }
        if !names.insert(p.application_id.trim().to_lowercase()) {
            bail!("Há mais de uma regra para {}.", p.application_id);
        }
        if let Some(id) = &p.interface_id
            && !id.trim().is_empty()
            && Uuid::parse_str(id).is_err()
        {
            bail!(
                "Identificador de adaptador inválido para {}.",
                p.application_id
            );
        }
    }
    Ok(())
}
/// Local drive path (`C:\…`), decided by text before any filesystem access.
/// UNC (`\\host\…`), device (`\\?\`, `\\.\`) and relative paths are refused, so an
/// elevated caller never opens an SMB connection while checking existence.
pub fn is_local_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
        && !path.contains('\0')
}
pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}
pub fn same_interface(a: &str, b: &str) -> bool {
    match (Uuid::parse_str(a), Uuid::parse_str(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.trim().eq_ignore_ascii_case(b.trim()),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Adapter {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub addresses: Vec<String>,
    pub connected: bool,
    pub has_gateway: bool,
    pub luid: u64,
    pub ipv4: bool,
    pub ipv6: bool,
    pub received: Option<u64>,
    pub sent: Option<u64>,
}
impl Adapter {
    pub fn mode(&self) -> Option<RouteMode> {
        match self.kind.as_str() {
            "Wireless80211" => Some(RouteMode::WiFi),
            "Ethernet" | "Ethernet3Megabit" | "FastEthernetT" | "FastEthernetFx"
            | "GigabitEthernet" => Some(RouteMode::Ethernet),
            _ => None,
        }
    }
}
pub fn resolve_interface<'a>(
    policy: &RoutingPolicy,
    adapters: &'a [Adapter],
) -> Option<&'a Adapter> {
    let mut matching: Vec<_> = adapters
        .iter()
        .filter(|a| a.connected && a.mode() == Some(policy.route_mode))
        .collect();
    if let Some(id) = &policy.interface_id
        && !id.trim().is_empty()
    {
        return matching.into_iter().find(|a| same_interface(&a.id, id));
    }
    matching.sort_by_key(|a| a.name.to_lowercase());
    matching.first().copied()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Connection {
    pub pid: u32,
    pub name: String,
    pub executable: Option<String>,
    /// "TCP" (established) or "UDP" (bound socket, possibly without a fixed destination).
    #[serde(default)]
    pub protocol: String,
    pub local_address: String,
    pub local_port: u16,
    /// Empty with port 0 for UDP sockets that do not fix a destination.
    pub remote_address: String,
    pub remote_port: u16,
    pub interface: Option<String>,
}
impl Connection {
    /// `address:port`, with IPv6 between brackets.
    pub fn endpoint(address: &str, port: u16) -> String {
        if address.contains(':') {
            format!("[{address}]:{port}")
        } else {
            format!("{address}:{port}")
        }
    }
    pub fn local_endpoint(&self) -> String {
        Self::endpoint(&self.local_address, self.local_port)
    }
    /// None for UDP sockets without a fixed destination.
    pub fn remote_endpoint(&self) -> Option<String> {
        (self.remote_port != 0).then(|| Self::endpoint(&self.remote_address, self.remote_port))
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NetworkSnapshot {
    pub adapters: Vec<Adapter>,
    pub connections: Vec<Connection>,
    pub error: Option<String>,
}
