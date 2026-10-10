use crate::*;
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, BufReader, Read},
    path::Path,
    sync::atomic::Ordering,
};

pub fn load(path: &Path, cancel: &AtomicBool) -> Result<CaptureFile> {
    let file = File::open(path).context("Não foi possível abrir a captura")?;
    let before = file.metadata()?;
    ensure!(before.is_file(), "Selecione um arquivo de captura comum.");
    ensure!(
        before.len() <= MAX_FILE_BYTES,
        "A captura excede o limite de 128 MiB."
    );
    let capture = read(BufReader::new(&file), cancel)?;
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len()
            && after.len() == capture.file_bytes
            && before.modified().ok() == after.modified().ok(),
        "O arquivo mudou durante a importação. Importe novamente."
    );
    Ok(capture)
}

struct Input<'a, R> {
    reader: R,
    hash: Sha256,
    count: u64,
    cancel: &'a AtomicBool,
    limits: Limits,
    sink: Option<&'a mut dyn PacketSink>,
}
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub bytes: u64,
    pub records: u64,
    pub retain: bool,
}
pub(crate) trait PacketSink {
    fn packet(&mut self, packet: &Packet, interface: &Interface) -> Result<()>;
    fn progress(&mut self, _bytes: u64) {}
}
impl<R: Read> Read for Input<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("Importação cancelada."));
        }
        let allowed = bytes
            .len()
            .min((self.limits.bytes - self.count + 1) as usize);
        let n = self.reader.read(&mut bytes[..allowed])?;
        if self.count + n as u64 > self.limits.bytes {
            return Err(io::Error::other(
                "A captura excede o limite de leitura configurado.",
            ));
        }
        self.hash.update(&bytes[..n]);
        self.count += n as u64;
        if let Some(sink) = &mut self.sink {
            sink.progress(self.count);
        }
        Ok(n)
    }
}
impl<R: Read> Input<'_, R> {
    fn exact<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0; N];
        self.read_exact(&mut bytes)
            .context("Arquivo de captura truncado")?;
        Ok(bytes)
    }
    fn next<const N: usize>(&mut self) -> Result<Option<[u8; N]>> {
        let mut bytes = [0; N];
        if self.read(&mut bytes[..1])? == 0 {
            return Ok(None);
        }
        self.read_exact(&mut bytes[1..])
            .context("Cabeçalho de captura truncado")?;
        Ok(Some(bytes))
    }
    fn bytes(&mut self, len: usize, limit: usize) -> Result<Vec<u8>> {
        ensure!(
            len <= limit,
            "Registro de captura excede o limite de leitura."
        );
        let mut bytes = vec![0; len];
        self.read_exact(&mut bytes)
            .context("Dados de captura truncados")?;
        Ok(bytes)
    }
}

#[derive(Clone, Copy)]
struct Endian(bool);
impl Endian {
    fn u16(self, b: &[u8]) -> u16 {
        let b = b[..2].try_into().unwrap();
        if self.0 {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        }
    }
    fn u32(self, b: &[u8]) -> u32 {
        let b = b[..4].try_into().unwrap();
        if self.0 {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        }
    }
    fn i64(self, b: &[u8]) -> i64 {
        let b = b[..8].try_into().unwrap();
        if self.0 {
            i64::from_le_bytes(b)
        } else {
            i64::from_be_bytes(b)
        }
    }
}

pub fn read<R: Read>(reader: R, cancel: &AtomicBool) -> Result<CaptureFile> {
    read_with(
        reader,
        cancel,
        Limits {
            bytes: MAX_FILE_BYTES,
            records: 1_000_000,
            retain: true,
        },
        None,
    )
}
pub(crate) fn read_with<'a, R: Read>(
    reader: R,
    cancel: &'a AtomicBool,
    limits: Limits,
    sink: Option<&'a mut dyn PacketSink>,
) -> Result<CaptureFile> {
    let mut input = Input {
        reader,
        hash: Sha256::new(),
        count: 0,
        cancel,
        limits,
        sink,
    };
    let magic = input.exact::<4>()?;
    let format = if magic == [0x0a, 0x0d, 0x0d, 0x0a] {
        Format::PcapNg
    } else {
        Format::Pcap
    };
    let mut capture = CaptureFile {
        format,
        sha256: String::new(),
        file_bytes: 0,
        interfaces: vec![],
        packets: vec![],
        total_packets: 0,
        omitted_packets: 0,
        truncated_packets: 0,
        auxiliary_blocks: 0,
        retained_bytes: 0,
    };
    match format {
        Format::Pcap => read_pcap(&mut input, magic, &mut capture)?,
        Format::PcapNg => read_ng(&mut input, magic, &mut capture)?,
    }
    capture.file_bytes = input.count;
    capture.sha256 = format!("{:X}", input.hash.finalize());
    Ok(capture)
}

fn retain(
    input: &mut Input<'_, impl Read>,
    capture: &mut CaptureFile,
    interface: usize,
    offset: u64,
    timestamp: Option<Timestamp>,
    original_len: u32,
    bytes: Vec<u8>,
) -> Result<()> {
    capture.total_packets += 1;
    ensure!(
        capture.total_packets <= input.limits.records,
        "O arquivo excede o limite de registros de pacote."
    );
    if bytes.len() < original_len as usize {
        capture.truncated_packets += 1;
    }
    let packet = Packet {
        number: capture.total_packets,
        file_offset: offset,
        interface,
        timestamp,
        original_len,
        captured_len: bytes.len() as u32,
        bytes,
    };
    if let Some(sink) = &mut input.sink {
        sink.packet(&packet, &capture.interfaces[interface])?;
    }
    // The legacy reader keeps a contiguous prefix; scans retain their own samples.
    if !input.limits.retain
        || capture.omitted_packets != 0
        || capture.packets.len() >= MAX_PACKETS
        || capture.retained_bytes + packet.bytes.len() > MAX_RETAINED_BYTES
    {
        capture.omitted_packets += 1;
    } else {
        capture.retained_bytes += packet.bytes.len();
        capture.packets.push(packet);
    }
    Ok(())
}

fn read_pcap<R: Read>(
    input: &mut Input<'_, R>,
    magic: [u8; 4],
    capture: &mut CaptureFile,
) -> Result<()> {
    let (endian, resolution) = match magic {
        [0xd4, 0xc3, 0xb2, 0xa1] => (Endian(true), 6),
        [0xa1, 0xb2, 0xc3, 0xd4] => (Endian(false), 6),
        [0x4d, 0x3c, 0xb2, 0xa1] => (Endian(true), 9),
        [0xa1, 0xb2, 0x3c, 0x4d] => (Endian(false), 9),
        _ => bail!("O arquivo não é PCAP clássico nem PCAPNG suportado."),
    };
    let header = input.exact::<20>()?;
    ensure!(
        endian.u16(&header) == 2 && endian.u16(&header[2..]) == 4,
        "Versão PCAP não suportada (esperado 2.4)."
    );
    let snaplen = endian.u32(&header[12..]);
    let link = endian.u32(&header[16..]);
    ensure!(snaplen > 0, "Snaplen PCAP inválido.");
    ensure!(
        link & 0x0bff0000 == 0,
        "Bits reservados no tipo de enlace PCAP."
    );
    capture.interfaces.push(Interface {
        section: 0,
        section_id: 0,
        link_type: link as u16,
        snaplen,
        name: "Interface do arquivo".into(),
        resolution,
        offset_seconds: 0,
    });
    loop {
        let offset = input.count;
        let Some(header) = input.next::<16>()? else {
            break;
        };
        let seconds = endian.u32(&header);
        let fraction = endian.u32(&header[4..]);
        let captured = endian.u32(&header[8..]);
        let original = endian.u32(&header[12..]);
        let units = if resolution == 6 {
            1_000_000
        } else {
            1_000_000_000
        };
        ensure!(
            fraction < units && captured <= original && captured <= snaplen,
            "Registro PCAP com horário/comprimento inválido."
        );
        let bytes = input.bytes(captured as usize, MAX_PACKET_BYTES)?;
        retain(
            input,
            capture,
            0,
            offset,
            Some(Timestamp {
                ticks: u64::from(seconds) * u64::from(units) + u64::from(fraction),
                resolution,
                offset_seconds: 0,
            }),
            original,
            bytes,
        )?;
    }
    Ok(())
}

fn options(
    mut data: &[u8],
    endian: Endian,
    mut visit: impl FnMut(u16, &[u8]) -> Result<()>,
) -> Result<()> {
    while !data.is_empty() {
        ensure!(data.len() >= 4, "Cabeçalho de opção PCAPNG truncado.");
        let code = endian.u16(data);
        let len = usize::from(endian.u16(&data[2..]));
        if code == 0 {
            ensure!(
                len == 0 && data[4..].iter().all(|b| *b == 0),
                "Fim de opções PCAPNG inválido."
            );
            return Ok(());
        }
        let padded = (len + 3) & !3;
        ensure!(data.len() >= 4 + padded, "Valor de opção PCAPNG truncado.");
        visit(code, &data[4..4 + len])?;
        data = &data[4 + padded..];
    }
    Ok(())
}

fn read_ng<R: Read>(
    input: &mut Input<'_, R>,
    first: [u8; 4],
    capture: &mut CaptureFile,
) -> Result<()> {
    let mut next = Some(first);
    let mut endian = Endian(true);
    let mut section = 0u32;
    let mut section_interfaces: Vec<usize> = vec![];
    let mut section_end = None;
    let mut blocks = 0u64;
    while let Some(kind) = next.take() {
        blocks += 1;
        ensure!(
            blocks <= input.limits.records,
            "O arquivo excede o limite de blocos."
        );
        let offset = input.count - 4;
        let prefix = input.exact::<8>()?;
        let shb = kind == [0x0a, 0x0d, 0x0d, 0x0a];
        if shb {
            if let Some(end) = section_end {
                ensure!(
                    offset == end,
                    "Comprimento declarado da seção PCAPNG não confere."
                );
            }
            endian = match prefix[4..] {
                [0x4d, 0x3c, 0x2b, 0x1a] => Endian(true),
                [0x1a, 0x2b, 0x3c, 0x4d] => Endian(false),
                _ => bail!("Byte order PCAPNG inválido."),
            };
        }
        let len = endian.u32(&prefix) as usize;
        ensure!(
            (12..=MAX_BLOCK_BYTES).contains(&len) && len.is_multiple_of(4),
            "Comprimento de bloco PCAPNG inválido ou acima de 4 MiB."
        );
        if !shb && let Some(end) = section_end {
            ensure!(
                offset + len as u64 <= end,
                "Bloco ultrapassa a seção PCAPNG."
            );
        }
        let rest = input.bytes(len - 12, MAX_BLOCK_BYTES)?;
        // Prefix contains the first four body bytes (or, for an empty block, its trailer).
        let trailer = if rest.is_empty() {
            &prefix[4..]
        } else {
            &rest[rest.len() - 4..]
        };
        ensure!(
            endian.u32(trailer) == len as u32,
            "Comprimentos inicial e final do bloco PCAPNG não conferem."
        );
        let mut body = Vec::with_capacity(len - 12);
        if !rest.is_empty() {
            body.extend_from_slice(&prefix[4..]);
            body.extend_from_slice(&rest[..rest.len() - 4]);
        }
        let block_type = endian.u32(&kind);
        match block_type {
            0x0a0d0d0a => {
                ensure!(
                    body.len() >= 16
                        && endian.u16(&body[4..]) == 1
                        && [0, 2].contains(&endian.u16(&body[6..])),
                    "Versão/cabeçalho PCAPNG não suportado."
                );
                let declared = endian.i64(&body[8..]);
                ensure!(
                    declared >= -1 && (declared == -1 || declared % 4 == 0),
                    "Comprimento de seção PCAPNG inválido."
                );
                section_end = if declared == -1 {
                    None
                } else {
                    Some(
                        input
                            .count
                            .checked_add(declared as u64)
                            .context("Seção PCAPNG grande demais")?,
                    )
                };
                options(&body[16..], endian, |_, _| Ok(()))?;
                section_interfaces.clear();
                section = section.checked_add(1).context("Seções PCAPNG demais")?;
            }
            1 => {
                ensure!(
                    body.len() >= 8 && capture.interfaces.len() < MAX_INTERFACES,
                    "Descrição de interface inválida ou mais de 1024 interfaces."
                );
                let mut interface = Interface {
                    section: section - 1,
                    section_id: section_interfaces.len() as u32,
                    link_type: endian.u16(&body),
                    snaplen: endian.u32(&body[4..]),
                    name: format!("Interface {}", section_interfaces.len() + 1),
                    resolution: 6,
                    offset_seconds: 0,
                };
                let mut seen = [false; 3];
                options(&body[8..], endian, |code, value| {
                    let index = match code {
                        2 => Some(0),
                        9 => Some(1),
                        14 => Some(2),
                        _ => None,
                    };
                    if let Some(index) = index {
                        ensure!(!seen[index], "Opção de interface PCAPNG repetida.");
                        seen[index] = true;
                    }
                    match code {
                        2 => interface.name = String::from_utf8_lossy(value)
                            .chars()
                            .take_while(|c| *c != '\0')
                            .filter(|c| {
                                !c.is_control()
                                    && !matches!(*c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
                            })
                            .take(128)
                            .collect(),
                        9 => {
                            ensure!(value.len() == 1, "Resolução PCAPNG inválida.");
                            interface.resolution = value[0];
                        }
                        14 => {
                            ensure!(value.len() == 8, "Offset PCAPNG inválido.");
                            interface.offset_seconds = endian.i64(value);
                        }
                        _ => {}
                    }
                    Ok(())
                })?;
                section_interfaces.push(capture.interfaces.len());
                capture.interfaces.push(interface);
            }
            6 | 2 => {
                ensure!(body.len() >= 20, "Cabeçalho de pacote PCAPNG truncado.");
                let local = if block_type == 6 {
                    endian.u32(&body)
                } else {
                    u32::from(endian.u16(&body))
                };
                let &global = section_interfaces
                    .get(local as usize)
                    .context("Pacote referencia interface PCAPNG inexistente")?;
                let iface = &capture.interfaces[global];
                let ticks =
                    (u64::from(endian.u32(&body[4..])) << 32) | u64::from(endian.u32(&body[8..]));
                let caplen = endian.u32(&body[12..]) as usize;
                let original = endian.u32(&body[16..]);
                ensure!(
                    caplen <= MAX_PACKET_BYTES
                        && caplen <= original as usize
                        && (iface.snaplen == 0 || caplen <= iface.snaplen as usize),
                    "Comprimento de pacote PCAPNG inválido."
                );
                let end = 20 + ((caplen + 3) & !3);
                ensure!(body.len() >= end, "Dados de pacote PCAPNG truncados.");
                options(&body[end..], endian, |_, _| Ok(()))?;
                let timestamp = Some(Timestamp {
                    ticks,
                    resolution: iface.resolution,
                    offset_seconds: iface.offset_seconds,
                });
                retain(
                    input,
                    capture,
                    global,
                    offset,
                    timestamp,
                    original,
                    body[20..20 + caplen].to_vec(),
                )?;
            }
            3 => {
                ensure!(body.len() >= 4, "Pacote simples PCAPNG truncado.");
                let &global = section_interfaces
                    .first()
                    .context("Pacote simples sem interface PCAPNG")?;
                let original = endian.u32(&body);
                let snap = capture.interfaces[global].snaplen;
                let len = if snap == 0 {
                    original
                } else {
                    original.min(snap)
                } as usize;
                ensure!(
                    len <= MAX_PACKET_BYTES && body.len() == 4 + ((len + 3) & !3),
                    "Comprimento do pacote simples PCAPNG inválido."
                );
                retain(
                    input,
                    capture,
                    global,
                    offset,
                    None,
                    original,
                    body[4..4 + len].to_vec(),
                )?;
            }
            _ => capture.auxiliary_blocks += 1,
        }
        next = input.next::<4>()?;
    }
    if let Some(end) = section_end {
        ensure!(input.count == end, "Seção PCAPNG truncada.");
    }
    Ok(())
}
