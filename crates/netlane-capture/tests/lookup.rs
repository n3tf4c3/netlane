use netlane_capture::{
    analysis::{Progress, SAMPLE_BYTES, read_dump},
    lookup::{PacketAnchor, load_packet, read_packet},
};
use std::{
    io::{self, Cursor, Read},
    sync::atomic::{AtomicBool, Ordering},
};

fn fixture(count: u32, size: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for word in [0xa1b2c3d4u32, 0x00040002, 0, 0, 65535, 1] {
        bytes.extend(word.to_le_bytes());
    }
    for number in 1..=count {
        for word in [1_700_000_000 + number, 123456, size, size + 10] {
            bytes.extend(word.to_le_bytes());
        }
        let mut frame = vec![0; size as usize];
        frame[42..].fill(number as u8);
        frame[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
        frame[14] = 0x45;
        frame[16..18].copy_from_slice(&((size - 14) as u16).to_be_bytes());
        frame[23] = 17;
        frame[26..30].copy_from_slice(&[192, 0, 2, 10]);
        frame[30..34].copy_from_slice(&[198, 51, 100, 20]);
        frame[34..36].copy_from_slice(&50000u16.to_be_bytes());
        frame[36..38].copy_from_slice(&53u16.to_be_bytes());
        frame[38..40].copy_from_slice(&((size - 34) as u16).to_be_bytes());
        bytes.extend(frame);
    }
    bytes
}
#[test]
fn recovers_an_unsampled_packet_and_verifies_the_entire_dump() {
    let bytes = fixture(3000, 4096);
    let stop = AtomicBool::new(false);
    let scan = read_dump(Cursor::new(&bytes), &stop, &Progress::default()).unwrap();
    let number = (2..3000)
        .find(|n| !scan.capture.packets.iter().any(|p| p.number == *n))
        .unwrap();
    let anchor = PacketAnchor::new(&scan.capture, number).unwrap();
    let progress = Progress::default();
    let packet = read_packet(Cursor::new(&bytes), &anchor, &stop, &progress).unwrap();
    assert_eq!(packet.sha256, scan.capture.sha256);
    assert_eq!(packet.packet.number, number);
    assert_eq!(packet.packet.file_offset, 24 + (number - 1) * (16 + 4096));
    assert_eq!(packet.packet.bytes.len(), SAMPLE_BYTES);
    assert_eq!(packet.packet.captured_len, 4096);
    assert_eq!(packet.packet.original_len, 4106);
    assert_eq!(
        packet.packet.timestamp.unwrap().ticks,
        (1_700_000_000 + number) * 1_000_000 + 123456
    );
    assert_eq!(packet.summary.protocol, "UDP");
    assert_eq!(packet.summary.destination, "198.51.100.20:53");
    let offset = packet.packet.file_offset as usize + 16;
    assert_eq!(packet.packet.bytes, bytes[offset..offset + SAMPLE_BYTES]);
    assert_eq!(progress.bytes.load(Ordering::Relaxed), bytes.len() as u64);
    assert_eq!(progress.packets.load(Ordering::Relaxed), 3000);
}
#[test]
fn mutation_after_the_requested_packet_cannot_pass_the_anchor() {
    let mut bytes = fixture(20, 128);
    let stop = AtomicBool::new(false);
    let scan = read_dump(Cursor::new(&bytes), &stop, &Progress::default()).unwrap();
    let anchor = PacketAnchor::new(&scan.capture, 1).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1; // Same length, valid records and identical requested packet.
    let error = read_packet(Cursor::new(&bytes), &anchor, &stop, &Progress::default()).unwrap_err();
    assert!(error.to_string().contains("difere da referência"));
    bytes.pop();
    assert!(read_packet(Cursor::new(&bytes), &anchor, &stop, &Progress::default()).is_err());
}
#[test]
fn invalid_numbers_and_inconsistent_totals_are_refused() {
    let bytes = fixture(20, 128);
    let stop = AtomicBool::new(false);
    let scan = read_dump(Cursor::new(&bytes), &stop, &Progress::default()).unwrap();
    assert!(PacketAnchor::new(&scan.capture, 0).is_err());
    assert!(PacketAnchor::new(&scan.capture, 21).is_err());
    let mut anchor = PacketAnchor::new(&scan.capture, 1).unwrap();
    anchor.total_packets += 1;
    assert!(read_packet(Cursor::new(&bytes), &anchor, &stop, &Progress::default()).is_err());
    anchor.total_packets -= 1;
    anchor.sha256.make_ascii_uppercase();
    assert!(read_packet(Cursor::new(&bytes), &anchor, &stop, &Progress::default()).is_ok());
    anchor.sha256 = "invalid".into();
    assert!(read_packet(Cursor::new(&bytes), &anchor, &stop, &Progress::default()).is_err());
}
struct CancelAfter<'a> {
    reader: Cursor<&'a [u8]>,
    cancel: &'a AtomicBool,
    after: u64,
}
impl Read for CancelAfter<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let read = self.reader.read(out)?;
        if self.reader.position() >= self.after {
            self.cancel.store(true, Ordering::Relaxed);
        }
        Ok(read)
    }
}
#[test]
fn cancellation_never_returns_evidence_even_after_finding_the_packet() {
    let bytes = fixture(20, 128);
    let stop = AtomicBool::new(false);
    let scan = read_dump(Cursor::new(&bytes), &stop, &Progress::default()).unwrap();
    let anchor = PacketAnchor::new(&scan.capture, 1).unwrap();
    for after in [24 + 16 + 128, bytes.len() as u64] {
        let cancel = AtomicBool::new(false);
        let progress = Progress::default();
        let reader = CancelAfter {
            reader: Cursor::new(bytes.as_slice()),
            cancel: &cancel,
            after,
        };
        assert!(read_packet(reader, &anchor, &cancel, &progress).is_err());
        assert!(progress.packets.load(Ordering::Relaxed) >= 1);
    }
}
#[test]
fn local_source_path_is_preserved_and_changed_files_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("closed.pcap");
    let mut bytes = fixture(20, 128);
    std::fs::write(&path, &bytes).unwrap();
    let stop = AtomicBool::new(false);
    let scan = netlane_capture::analysis::load_dump(&path, &stop, &Progress::default()).unwrap();
    assert_eq!(scan.source.as_deref(), Some(path.as_path()));
    let anchor = PacketAnchor::new(&scan.capture, 10).unwrap();
    let recovered = load_packet(&path, &anchor, &stop, &Progress::default()).unwrap();
    assert_eq!(recovered.packet.number, 10);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    bytes.pop();
    std::fs::write(&path, &bytes).unwrap();
    assert!(load_packet(&path, &anchor, &stop, &Progress::default()).is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(load_packet(&path, &anchor, &stop, &Progress::default()).is_err());
}
