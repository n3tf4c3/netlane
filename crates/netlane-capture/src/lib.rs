//! Offline capture files are untrusted input. All reads, allocations and retained
//! packets are bounded. Import never writes the source, resolves names or sends data.
#![forbid(unsafe_code)]

pub mod analysis;
pub mod collection;
mod decode;
mod reader;
pub use decode::Summary;
pub use reader::{load, read};
use std::{io::Cursor, sync::atomic::AtomicBool};

pub const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_PACKET_BYTES: usize = 1024 * 1024;
pub const MAX_BLOCK_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PACKETS: usize = 50_000;
pub const MAX_INTERFACES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pcap,
    PcapNg,
}
impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pcap => "PCAP",
            Self::PcapNg => "PCAPNG",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Interface {
    pub section: u32,
    pub section_id: u32,
    pub link_type: u16,
    pub snaplen: u32,
    pub name: String,
    pub resolution: u8,
    pub offset_seconds: i64,
}

/// Raw timestamp units are preserved; conversion floors fractions of a nanosecond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timestamp {
    pub ticks: u64,
    pub resolution: u8,
    pub offset_seconds: i64,
}
impl Timestamp {
    pub fn unix_nanos(self) -> i128 {
        let exponent = u32::from(self.resolution & 0x7f);
        let denominator = if self.resolution & 0x80 == 0 {
            10u128.checked_pow(exponent)
        } else {
            1u128.checked_shl(exponent)
        };
        // A decimal denominator > 10^38 makes even u64::MAX less than 1 ns.
        let nanos = denominator.map_or(0, |d| u128::from(self.ticks) * 1_000_000_000 / d);
        i128::from(self.offset_seconds) * 1_000_000_000 + nanos as i128
    }
    pub fn clock_label(self) -> String {
        let nanos = self.unix_nanos();
        let seconds = nanos.div_euclid(1_000_000_000).rem_euclid(86_400);
        format!(
            "{:02}:{:02}:{:02}.{:06}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
            nanos.rem_euclid(1_000_000_000) / 1000
        )
    }
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub number: u64,
    pub file_offset: u64,
    pub interface: usize,
    pub timestamp: Option<Timestamp>,
    pub original_len: u32,
    pub captured_len: u32,
    pub bytes: Vec<u8>,
}
impl Packet {
    pub fn summary(&self, link_type: u16) -> Summary {
        decode::summarize(&self.bytes, link_type)
    }
    pub fn hex_preview(&self) -> String {
        let mut out = String::new();
        for (row, bytes) in self.bytes[..self.bytes.len().min(256)]
            .chunks(16)
            .enumerate()
        {
            use std::fmt::Write;
            let _ = write!(out, "{:04x}  ", row * 16);
            for i in 0..16 {
                if let Some(byte) = bytes.get(i) {
                    let _ = write!(out, "{byte:02x} ");
                } else {
                    out.push_str("   ");
                }
            }
            out.push(' ');
            for &byte in bytes {
                out.push(if (32..127).contains(&byte) {
                    char::from(byte)
                } else {
                    '.'
                });
            }
            out.push('\n');
        }
        if self.bytes.len() > 256 {
            out.push_str("… prévia limitada a 256 bytes\n");
        }
        out
    }
}

#[derive(Debug)]
pub struct CaptureFile {
    pub format: Format,
    pub sha256: String,
    pub file_bytes: u64,
    pub interfaces: Vec<Interface>,
    pub packets: Vec<Packet>,
    pub total_packets: u64,
    pub omitted_packets: u64,
    pub truncated_packets: u64,
    pub auxiliary_blocks: u64,
    pub retained_bytes: usize,
}
impl CaptureFile {
    pub fn limited(&self) -> bool {
        self.omitted_packets != 0
    }
}

/// A visibly synthetic, memory-only demonstration. Never opens a socket or a file.
pub fn example() -> CaptureFile {
    read(Cursor::new(example_bytes()), &AtomicBool::new(false)).expect("built-in synthetic PCAP")
}
pub fn example_scan() -> analysis::Scan {
    analysis::read_dump(
        Cursor::new(example_bytes()),
        &AtomicBool::new(false),
        &analysis::Progress::default(),
    )
    .expect("built-in synthetic scan")
}
/// Synthetic baseline/deviation pair for visual QA. No files or network.
pub fn comparison_example(reference: bool) -> analysis::Scan {
    let original = example_bytes();
    let mut file = original[..24].to_vec();
    let count = if reference { 120 } else { 10_000 };
    for i in 0..count {
        let deviates = !reference && i >= count / 2;
        let mut bytes = original[40..82].to_vec();
        if deviates {
            bytes.resize(54, 0);
            bytes[16..18].copy_from_slice(&40u16.to_be_bytes());
            bytes[23] = 6;
            bytes[33] = 99;
            bytes[36..38].copy_from_slice(&443u16.to_be_bytes());
            bytes[46] = 0x50;
            bytes[47] = 4;
        }
        for value in [1_791_590_400 + i, 0, bytes.len() as u32, bytes.len() as u32] {
            file.extend(value.to_le_bytes());
        }
        file.extend(bytes);
    }
    analysis::read_dump(
        Cursor::new(file),
        &AtomicBool::new(false),
        &analysis::Progress::default(),
    )
    .expect("synthetic comparison")
}
fn example_bytes() -> Vec<u8> {
    let mut file = Vec::new();
    for value in [0xa1b2c3d4u32, 0x00040002, 0, 0, 65535, 1] {
        file.extend(value.to_le_bytes());
    }
    for i in 0..6u32 {
        let mut bytes = vec![0; 14 + 20 + 8];
        bytes[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
        bytes[14] = 0x45;
        bytes[16..18].copy_from_slice(&28u16.to_be_bytes());
        bytes[22] = 64;
        bytes[23] = 17;
        bytes[26..30].copy_from_slice(&[192, 0, 2, 10]);
        bytes[30..34].copy_from_slice(&[198, 51, 100, 20]);
        bytes[34..36].copy_from_slice(&(50000 + i as u16).to_be_bytes());
        bytes[36..38].copy_from_slice(&53u16.to_be_bytes());
        bytes[38..40].copy_from_slice(&8u16.to_be_bytes());
        for value in [
            1_791_590_400,
            i * 150_000,
            bytes.len() as u32,
            bytes.len() as u32,
        ] {
            file.extend(value.to_le_bytes());
        }
        file.extend(bytes);
    }
    file
}
