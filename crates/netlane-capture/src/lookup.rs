//! Explicit recovery of one packet outside the distributed display samples.
//! The whole closed dump is parsed and hashed before returning any evidence.
//! Only one bounded preview is retained; this never writes or sends the source.
use crate::{
    CaptureFile, Interface, Packet, Summary,
    analysis::{MAX_DUMP_BYTES, MAX_DUMP_RECORDS, Progress, SAMPLE_BYTES},
    reader::{Limits, PacketSink, read_with},
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug)]
pub struct PacketAnchor {
    pub sha256: String,
    pub file_bytes: u64,
    pub total_packets: u64,
    pub number: u64,
}
impl PacketAnchor {
    pub fn new(capture: &CaptureFile, number: u64) -> Result<Self> {
        let anchor = Self {
            sha256: capture.sha256.clone(),
            file_bytes: capture.file_bytes,
            total_packets: capture.total_packets,
            number,
        };
        anchor.validate()?;
        Ok(anchor)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.sha256.len() == 64 && self.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "A referência precisa do SHA-256 do dump inteiro."
        );
        ensure!(
            self.file_bytes <= MAX_DUMP_BYTES && self.total_packets <= MAX_DUMP_RECORDS,
            "A referência excede os limites de leitura do dump."
        );
        ensure!(
            self.number > 0 && self.number <= self.total_packets,
            "Informe um número de pacote presente neste dump."
        );
        Ok(())
    }
}
#[derive(Debug)]
pub struct RecoveredPacket {
    pub sha256: String,
    pub packet: Packet,
    pub interface: Interface,
    pub summary: Summary,
}
struct Lookup<'a> {
    number: u64,
    progress: &'a Progress,
    recovered: Option<RecoveredPacket>,
}
impl PacketSink for Lookup<'_> {
    fn progress(&mut self, bytes: u64) {
        self.progress.bytes.store(bytes, Ordering::Relaxed);
    }
    fn packet(&mut self, packet: &Packet, interface: &Interface) -> Result<()> {
        self.progress
            .packets
            .store(packet.number, Ordering::Relaxed);
        if packet.number == self.number {
            // Decode using all captured bytes, as the full-dump analyzer does.
            let summary = packet.summary(interface.link_type);
            let retained = Packet {
                number: packet.number,
                file_offset: packet.file_offset,
                interface: packet.interface,
                timestamp: packet.timestamp,
                original_len: packet.original_len,
                captured_len: packet.captured_len,
                bytes: packet.bytes[..packet.bytes.len().min(SAMPLE_BYTES)].to_vec(),
            };
            self.recovered = Some(RecoveredPacket {
                sha256: String::new(),
                packet: retained,
                interface: interface.clone(),
                summary,
            });
        }
        Ok(())
    }
}
pub fn read_packet<R: Read>(
    reader: R,
    anchor: &PacketAnchor,
    cancel: &AtomicBool,
    progress: &Progress,
) -> Result<RecoveredPacket> {
    anchor.validate()?;
    let mut lookup = Lookup {
        number: anchor.number,
        progress,
        recovered: None,
    };
    let capture = read_with(
        reader,
        cancel,
        Limits {
            bytes: MAX_DUMP_BYTES,
            records: MAX_DUMP_RECORDS,
            retain: false,
        },
        Some(&mut lookup),
    )?;
    ensure!(
        capture.sha256.eq_ignore_ascii_case(&anchor.sha256)
            && capture.file_bytes == anchor.file_bytes
            && capture.total_packets == anchor.total_packets,
        "O dump difere da referência. Nenhum pacote foi recuperado; importe o arquivo novamente."
    );
    ensure!(!cancel.load(Ordering::Relaxed), "Leitura cancelada.");
    let mut recovered = lookup.recovered.context("Pacote não encontrado no dump.")?;
    recovered.sha256 = capture.sha256;
    Ok(recovered)
}
pub fn load_packet(
    path: &Path,
    anchor: &PacketAnchor,
    cancel: &AtomicBool,
    progress: &Progress,
) -> Result<RecoveredPacket> {
    anchor.validate()?;
    let file = File::open(path).context("Não foi possível abrir o dump original")?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() == anchor.file_bytes && before.len() <= MAX_DUMP_BYTES,
        "O dump original está indisponível ou mudou de tamanho. Importe novamente."
    );
    let recovered = read_packet(BufReader::new(&file), anchor, cancel, progress)?;
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len() && before.modified().ok() == after.modified().ok(),
        "O dump mudou durante a leitura. Use uma coleta fechada."
    );
    Ok(recovered)
}
