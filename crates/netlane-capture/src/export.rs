//! Derived exports contain only the retained bytes. Originals are never reopened.
//! PCAPNG follows SHB/IDB/EPB layouts in draft-ietf-opsawg-pcapng-05.
use crate::{CaptureFile, Format, Interface, Packet};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Write, path::Path};

#[derive(Clone)]
pub struct Selection {
    source_sha256: String,
    source_packets: u64,
    interfaces: Vec<Interface>,
    packets: Vec<Packet>,
}
#[derive(Debug, Serialize)]
pub struct Mapping {
    pub output_number: usize,
    pub source_number: u64,
    pub source_interface: usize,
    pub source_section: u32,
    pub source_section_interface: u32,
    pub source_link_type: u16,
    pub source_snaplen: u32,
    pub source_offset: u64,
    pub source_captured_bytes: u32,
    pub exported_bytes: usize,
    pub original_bytes: u32,
}
#[derive(Debug, Serialize)]
pub struct Receipt {
    pub schema: u32,
    pub source_sha256: String,
    pub source_packets: u64,
    pub file: String,
    pub sha256: String,
    pub bytes: usize,
    pub packets: Vec<Mapping>,
    pub clipped_packets: usize,
    pub timestamp_policy: String,
    pub limitations: String,
}
impl Selection {
    pub fn new(capture: &CaptureFile, indexes: &[usize]) -> Result<Self> {
        ensure!(
            !indexes.is_empty() && indexes.len() <= 512,
            "Selecione de 1 a 512 amostras."
        );
        ensure!(
            capture.interfaces.len() <= crate::MAX_INTERFACES,
            "Interfaces demais."
        );
        ensure!(
            capture.sha256.len() == 64 && capture.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Hash inválido."
        );
        let mut seen = BTreeSet::new();
        let mut packets = Vec::with_capacity(indexes.len());
        let mut bytes = 0;
        for &index in indexes {
            let p = capture.packets.get(index).context("Amostra inexistente")?;
            let i = capture
                .interfaces
                .get(p.interface)
                .context("Interface inexistente")?;
            ensure!(
                seen.insert(p.number) && p.number > 0 && p.number <= capture.total_packets,
                "Número repetido ou inválido."
            );
            ensure!(
                p.bytes.len() <= p.captured_len as usize
                    && p.captured_len <= p.original_len
                    && (i.snaplen == 0 || p.captured_len <= i.snaplen),
                "Comprimentos inválidos."
            );
            bytes += p.bytes.len();
            ensure!(
                bytes <= 1024 * 1024,
                "Exportação excede 1 MiB de bytes retidos."
            );
            packets.push(p.clone());
        }
        packets.sort_by_key(|p| p.number);
        Ok(Self {
            source_sha256: capture.sha256.clone(),
            source_packets: capture.total_packets,
            interfaces: capture.interfaces.clone(),
            packets,
        })
    }
    pub fn len(&self) -> usize {
        self.packets.len()
    }
    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }
    fn encode(&self, format: Format) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        match format {
            Format::Pcap => {
                let first = self.packets[0].interface;
                ensure!(
                    self.packets.iter().all(|p| p.interface == first),
                    "PCAP exige uma única interface. Use PCAPNG."
                );
                let snap = self
                    .packets
                    .iter()
                    .map(|p| p.bytes.len())
                    .max()
                    .unwrap_or(1)
                    .max(1) as u32;
                for word in [
                    0xa1b23c4du32,
                    0x00040002,
                    0,
                    0,
                    snap,
                    u32::from(self.interfaces[first].link_type),
                ] {
                    out.extend(word.to_le_bytes());
                }
                for p in &self.packets {
                    let ns = p
                        .timestamp
                        .context("PCAP não representa pacotes sem horário. Use PCAPNG.")?
                        .unix_nanos();
                    ensure!(
                        (0..=(i128::from(u32::MAX) * 1_000_000_000 + 999_999_999)).contains(&ns),
                        "Horário fora do intervalo PCAP. Use PCAPNG."
                    );
                    for word in [
                        (ns / 1_000_000_000) as u32,
                        (ns % 1_000_000_000) as u32,
                        p.bytes.len() as u32,
                        p.original_len,
                    ] {
                        out.extend(word.to_le_bytes());
                    }
                    out.extend(&p.bytes);
                }
            }
            Format::PcapNg => {
                // One section per selected packet also represents SPB without inventing a timestamp
                // and accommodates different source sections/resolutions/offsets.
                for p in &self.packets {
                    let i = &self.interfaces[p.interface];
                    let mut shb = Vec::new();
                    shb.extend(0x1a2b3c4du32.to_le_bytes());
                    shb.extend(1u16.to_le_bytes());
                    shb.extend(0u16.to_le_bytes());
                    shb.extend((-1i64).to_le_bytes());
                    block(&mut out, 0x0a0d0d0a, shb);
                    let mut idb = Vec::new();
                    idb.extend(i.link_type.to_le_bytes());
                    idb.extend(0u16.to_le_bytes());
                    // Derived snaplen describes these retained bytes, not the source's larger snaplen.
                    let snap = p.bytes.len() as u32;
                    ensure!(
                        p.timestamp.is_some() || !p.bytes.is_empty() || p.original_len == 0,
                        "PCAPNG não representa amostra vazia de pacote sem horário."
                    );
                    idb.extend(snap.to_le_bytes());
                    let name: String = i
                        .name
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(128)
                        .collect();
                    option(&mut idb, 2, name.as_bytes());
                    let (resolution, offset) =
                        p.timestamp.map_or((i.resolution, i.offset_seconds), |t| {
                            (t.resolution, t.offset_seconds)
                        });
                    option(&mut idb, 9, &[resolution]);
                    option(&mut idb, 14, &offset.to_le_bytes());
                    idb.extend([0u8; 4]);
                    block(&mut out, 1, idb);
                    let mut body = Vec::new();
                    if let Some(t) = p.timestamp {
                        for word in [
                            0,
                            (t.ticks >> 32) as u32,
                            t.ticks as u32,
                            p.bytes.len() as u32,
                            p.original_len,
                        ] {
                            body.extend(word.to_le_bytes());
                        }
                        body.extend(&p.bytes);
                        pad(&mut body);
                        block(&mut out, 6, body);
                    } else {
                        body.extend(p.original_len.to_le_bytes());
                        body.extend(&p.bytes);
                        pad(&mut body);
                        block(&mut out, 3, body);
                    }
                }
            }
        }
        Ok(out)
    }
    /// A new directory is the bundle boundary. Errors leave recoverable .part files;
    /// receipt.json is published last and is the completion marker.
    pub fn save(&self, directory: &Path, format: Format) -> Result<Receipt> {
        ensure!(directory.is_absolute(), "Destino deve ser absoluto.");
        let data = self.encode(format)?;
        let file = if format == Format::Pcap {
            "selected.pcap"
        } else {
            "selected.pcapng"
        };
        let receipt = Receipt { schema: 1, source_sha256: self.source_sha256.clone(), source_packets: self.source_packets,
            file: file.into(), sha256: format!("{:X}", Sha256::digest(&data)), bytes: data.len(),
            clipped_packets: self.packets.iter().filter(|p| p.bytes.len() < p.captured_len as usize).count(),
            timestamp_policy: if format == Format::Pcap { "Nanossegundos UTC; frações menores descartadas." } else { "Unidades/resolução/offset originais; ausentes continuam ausentes." }.into(),
            limitations: "Exportação DERIVADA das amostras retidas, até 2 KiB por amostra na análise de dumps. Não é cópia do dump inteiro nem filtro do arquivo inteiro. Caplen gravado corresponde aos bytes exportados. Blocos/opções auxiliares, comentários, estatísticas, segredos, nomes resolvidos e metadados não modelados não são copiados. PCAP perde nomes e estrutura de interfaces. PCAPNG reagrupa cada pacote em seção própria; consulte o mapeamento de origem.".into(),
            packets: self.packets.iter().enumerate().map(|(n,p)| Mapping { output_number:n+1,source_number:p.number,
                source_interface:p.interface,source_section:self.interfaces[p.interface].section,
                source_section_interface:self.interfaces[p.interface].section_id,
                source_link_type:self.interfaces[p.interface].link_type,source_snaplen:self.interfaces[p.interface].snaplen,
                source_offset:p.file_offset,source_captured_bytes:p.captured_len,
                exported_bytes:p.bytes.len(),original_bytes:p.original_len }).collect() };
        // Round-trip checks the actual encoded bytes with the existing bounded reader.
        let reopened = crate::read(
            std::io::Cursor::new(&data),
            &std::sync::atomic::AtomicBool::new(false),
        )?;
        ensure!(
            reopened.total_packets == self.packets.len() as u64
                && reopened.sha256 == receipt.sha256,
            "Verificação da exportação falhou."
        );
        fs::create_dir(directory)
            .context("Escolha uma pasta nova; exportações existentes são preservadas")?;
        publish(directory, file, &data)?;
        let saved = crate::load(
            &directory.join(file),
            &std::sync::atomic::AtomicBool::new(false),
        )?;
        ensure!(
            saved.sha256 == receipt.sha256,
            "Hash do arquivo salvo não confere; recibo não publicado."
        );
        publish(
            directory,
            "receipt.json",
            &serde_json::to_vec_pretty(&receipt)?,
        )?;
        Ok(receipt)
    }
}
fn pad(data: &mut Vec<u8>) {
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
}
fn option(out: &mut Vec<u8>, code: u16, value: &[u8]) {
    out.extend(code.to_le_bytes());
    out.extend((value.len() as u16).to_le_bytes());
    out.extend(value);
    pad(out);
}
fn block(out: &mut Vec<u8>, kind: u32, body: Vec<u8>) {
    let length = (body.len() + 12) as u32;
    out.extend(kind.to_le_bytes());
    out.extend(length.to_le_bytes());
    out.extend(body);
    out.extend(length.to_le_bytes());
}
fn publish(directory: &Path, name: &str, data: &[u8]) -> Result<()> {
    let part = directory.join(format!("{name}.part"));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)?;
    f.write_all(data)?;
    f.sync_all()?;
    drop(f);
    fs::rename(part, directory.join(name))?;
    Ok(())
}
