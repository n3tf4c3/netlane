use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Clone, Debug)]
pub struct Summary {
    pub protocol: &'static str,
    pub source: String,
    pub destination: String,
    pub info: String,
    pub service: &'static str,
    pub payload: Option<std::ops::Range<usize>>,
}
impl Summary {
    pub fn raw_preview(&self, bytes: &[u8]) -> String {
        self.payload
            .as_ref()
            .and_then(|r| bytes.get(r.start..r.end.min(bytes.len())))
            .unwrap_or_default()
            .iter()
            .take(96)
            .map(|b| {
                if (32..127).contains(b) {
                    char::from(*b)
                } else {
                    '.'
                }
            })
            .collect()
    }
}
fn service(port: u16) -> &'static str {
    match port {
        22 => "ssh",
        25 => "smtp",
        53 => "dns",
        67 | 68 => "dhcp",
        80 => "http",
        123 => "ntp",
        443 => "https",
        445 => "smb",
        3389 => "rdp",
        _ => "—",
    }
}
fn word(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}
pub(crate) fn summarize(bytes: &[u8], link: u16) -> Summary {
    let mut s = Summary {
        protocol: "OUTRO",
        source: "—".into(),
        destination: "—".into(),
        info: format!("Enlace {link} · sem decodificação"),
        service: "—",
        payload: None,
    };
    let (offset, kind) = match link {
        1 if bytes.len() >= 14 => {
            let mut at = 14;
            let mut kind = word(bytes, 12).unwrap();
            for _ in 0..4 {
                if ![0x8100, 0x88a8, 0x9100].contains(&kind) {
                    break;
                }
                let Some(next) = word(bytes, at + 2) else {
                    s.info = "VLAN truncada".into();
                    return s;
                };
                kind = next;
                at += 4;
            }
            (at, kind)
        }
        113 if bytes.len() >= 16 => (16, word(bytes, 14).unwrap()),
        276 if bytes.len() >= 20 => (20, word(bytes, 0).unwrap()),
        0 | 108 if bytes.len() >= 4 => {
            let array = bytes[..4].try_into().unwrap();
            let le = u32::from_le_bytes(array);
            let be = u32::from_be_bytes(array);
            let kind = if le == 2 || be == 2 {
                0x0800
            } else if [23, 24, 28, 30].contains(&le) || [23, 24, 28, 30].contains(&be) {
                0x86dd
            } else {
                0
            };
            (4, kind)
        }
        12 | 101 | 228 | 229 if !bytes.is_empty() => (
            0,
            match bytes[0] >> 4 {
                4 => 0x0800,
                6 => 0x86dd,
                _ => 0,
            },
        ),
        _ => return s,
    };
    let Some(ip) = bytes.get(offset..) else {
        return s;
    };
    if kind == 0x0806 {
        s.protocol = "ARP";
        s.info = "ARP".into();
        return s;
    }
    let (protocol, transport, ipv6, truncated, transport_offset) = match kind {
        0x0800 if ip.len() >= 20 && ip[0] >> 4 == 4 => {
            s.protocol = "IPv4";
            s.source = Ipv4Addr::new(ip[12], ip[13], ip[14], ip[15]).to_string();
            s.destination = Ipv4Addr::new(ip[16], ip[17], ip[18], ip[19]).to_string();
            let header = usize::from(ip[0] & 15) * 4;
            let total = usize::from(word(ip, 2).unwrap());
            if header < 20 || total < header || ip.len() < header {
                s.info = "Cabeçalho IPv4 inválido/truncado".into();
                return s;
            }
            if word(ip, 6).unwrap() & 0x3fff != 0 {
                s.info = "Fragmento IPv4 · sem remontagem".into();
                return s;
            }
            (
                ip[9],
                &ip[header..total.min(ip.len())],
                false,
                total > ip.len(),
                offset + header,
            )
        }
        0x86dd if ip.len() >= 40 && ip[0] >> 4 == 6 => {
            s.protocol = "IPv6";
            s.source = Ipv6Addr::from(<[u8; 16]>::try_from(&ip[8..24]).unwrap()).to_string();
            s.destination = Ipv6Addr::from(<[u8; 16]>::try_from(&ip[24..40]).unwrap()).to_string();
            let total = 40 + usize::from(word(ip, 4).unwrap());
            let end = total.min(ip.len());
            let mut next = ip[6];
            let mut at = 40;
            for _ in 0..8 {
                if next == 44 {
                    s.info = "Fragmento IPv6 · sem remontagem".into();
                    return s;
                }
                if ![0, 43, 60, 51].contains(&next) {
                    break;
                }
                if at + 2 > end {
                    s.info = "Extensão IPv6 truncada".into();
                    return s;
                }
                let len = if next == 51 {
                    (usize::from(ip[at + 1]) + 2) * 4
                } else {
                    (usize::from(ip[at + 1]) + 1) * 8
                };
                next = ip[at];
                at += len;
                if at > end {
                    s.info = "Extensão IPv6 truncada".into();
                    return s;
                }
            }
            if [0, 43, 60, 51, 44].contains(&next) {
                s.info = "Extensões IPv6 além do limite".into();
                return s;
            }
            (next, &ip[at..end], true, total > ip.len(), offset + at)
        }
        _ => return s,
    };
    s.protocol = match protocol {
        6 => "TCP",
        17 => "UDP",
        1 => "ICMP",
        58 => "ICMPv6",
        _ => s.protocol,
    };
    s.info = if truncated {
        "Pacote IP truncado".into()
    } else {
        format!("Protocolo IP {protocol}")
    };
    if ![6, 17].contains(&protocol) {
        return s;
    }
    let minimum = if protocol == 6 { 20 } else { 8 };
    if transport.len() < minimum {
        s.info = "Cabeçalho de transporte truncado".into();
        return s;
    }
    let source = word(transport, 0).unwrap();
    let destination = word(transport, 2).unwrap();
    s.service = service(destination);
    if s.service == "—" {
        s.service = service(source);
    }
    s.source = if ipv6 {
        format!("[{}]:{source}", s.source)
    } else {
        format!("{}:{source}", s.source)
    };
    s.destination = if ipv6 {
        format!("[{}]:{destination}", s.destination)
    } else {
        format!("{}:{destination}", s.destination)
    };
    if protocol == 6 {
        let header = usize::from(transport[12] >> 4) * 4;
        if header < 20 || header > transport.len() {
            s.info = "Cabeçalho TCP inválido/truncado".into();
            return s;
        }
        let flags = transport[13];
        s.payload = Some(transport_offset + header..transport_offset + transport.len());
        let names = [
            (0x02, "SYN"),
            (0x10, "ACK"),
            (0x01, "FIN"),
            (0x04, "RST"),
            (0x08, "PSH"),
            (0x20, "URG"),
            (0x40, "ECE"),
            (0x80, "CWR"),
        ];
        let labels: Vec<_> = names
            .into_iter()
            .filter(|(bit, _)| flags & bit != 0)
            .map(|(_, name)| name)
            .collect();
        s.info = format!(
            "{} · {} bytes de dados{}",
            labels.join(" "),
            transport.len() - header,
            if truncated { " · truncado" } else { "" }
        );
    } else {
        let len = usize::from(word(transport, 4).unwrap());
        if len >= 8 {
            s.payload = Some(transport_offset + 8..transport_offset + len.min(transport.len()));
        }
        s.info = if len < 8 {
            "Comprimento UDP inválido".into()
        } else {
            format!(
                "UDP · {} bytes{}",
                len - 8,
                if len > transport.len() || truncated {
                    " · truncado"
                } else {
                    ""
                }
            )
        };
        if source == 53 || destination == 53 {
            s.info.push_str(" · porta DNS (estimativa)");
        }
    }
    s
}
