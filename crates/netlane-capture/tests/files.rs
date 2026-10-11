use netlane_capture::*;
use std::{
    io::{Cursor, Read},
    sync::atomic::{AtomicBool, Ordering},
};

fn u16s(out: &mut Vec<u8>, v: u16, le: bool) {
    out.extend(if le { v.to_le_bytes() } else { v.to_be_bytes() });
}
fn u32s(out: &mut Vec<u8>, v: u32, le: bool) {
    out.extend(if le { v.to_le_bytes() } else { v.to_be_bytes() });
}
fn u64s(out: &mut Vec<u8>, v: u64, le: bool) {
    out.extend(if le { v.to_le_bytes() } else { v.to_be_bytes() });
}
fn pcap(le: bool, nano: bool, data: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    u32s(&mut out, if nano { 0xa1b23c4d } else { 0xa1b2c3d4 }, le);
    u16s(&mut out, 2, le);
    u16s(&mut out, 4, le);
    for value in [0, 0, 65535, 101] {
        u32s(&mut out, value, le);
    }
    for value in [2, 123456, data.len() as u32, data.len() as u32] {
        u32s(&mut out, value, le);
    }
    out.extend(data);
    out
}
fn block(kind: u32, body: &[u8], le: bool) -> Vec<u8> {
    assert_eq!(body.len() % 4, 0);
    let mut out = vec![];
    let len = 12 + body.len() as u32;
    u32s(&mut out, kind, le);
    u32s(&mut out, len, le);
    out.extend(body);
    u32s(&mut out, len, le);
    out
}
fn option(out: &mut Vec<u8>, code: u16, value: &[u8], le: bool) {
    u16s(out, code, le);
    u16s(out, value.len() as u16, le);
    out.extend(value);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}
fn section(le: bool) -> Vec<u8> {
    let mut body = vec![];
    u32s(&mut body, 0x1a2b3c4d, le);
    u16s(&mut body, 1, le);
    u16s(&mut body, 0, le);
    u64s(&mut body, u64::MAX, le);
    block(0x0a0d0d0a, &body, le)
}
fn interface(le: bool, resolution: u8, offset: i64, name: &str) -> Vec<u8> {
    let mut body = vec![];
    u16s(&mut body, 101, le);
    u16s(&mut body, 0, le);
    u32s(&mut body, 65535, le);
    option(&mut body, 2, name.as_bytes(), le);
    option(&mut body, 9, &[resolution], le);
    option(
        &mut body,
        14,
        &if le {
            offset.to_le_bytes()
        } else {
            offset.to_be_bytes()
        },
        le,
    );
    option(&mut body, 0, &[], le);
    block(1, &body, le)
}
fn enhanced(le: bool, id: u32, ticks: u64, data: &[u8]) -> Vec<u8> {
    let mut body = vec![];
    for value in [
        id,
        (ticks >> 32) as u32,
        ticks as u32,
        data.len() as u32,
        data.len() as u32,
    ] {
        u32s(&mut body, value, le);
    }
    body.extend(data);
    while !body.len().is_multiple_of(4) {
        body.push(0);
    }
    block(6, &body, le)
}
fn parse(bytes: &[u8]) -> anyhow::Result<CaptureFile> {
    read(Cursor::new(bytes), &AtomicBool::new(false))
}

#[test]
fn classic_endian_and_precision_are_preserved() {
    for le in [true, false] {
        for nano in [true, false] {
            let c = parse(&pcap(le, nano, &[0x45, 0, 0, 20])).unwrap();
            assert_eq!(c.format, Format::Pcap);
            assert_eq!(c.packets[0].bytes, [0x45, 0, 0, 20]);
            let timestamp = c.packets[0].timestamp.unwrap();
            assert_eq!(
                timestamp.unix_nanos(),
                2_000_000_000 + if nano { 123456 } else { 123456000 }
            );
            assert_eq!(c.packets[0].file_offset, 24);
            let recovered = lookup::read_packet(
                Cursor::new(pcap(le, nano, &[0x45, 0, 0, 20])),
                &lookup::PacketAnchor::new(&c, 1).unwrap(),
                &AtomicBool::new(false),
                &analysis::Progress::default(),
            )
            .unwrap();
            assert_eq!(recovered.packet.timestamp, c.packets[0].timestamp);
            assert_eq!(recovered.packet.bytes, c.packets[0].bytes);
            assert_eq!(recovered.interface.link_type, 101);
        }
    }
}
#[test]
fn multiple_interfaces_sections_endian_offsets_and_simple_packets() {
    let mut bytes = section(true);
    bytes.extend(interface(true, 9, -1, "first"));
    bytes.extend(interface(true, 0x8a, 5, "second"));
    bytes.extend(enhanced(true, 1, 1536, &[1, 2, 3]));
    let mut body = vec![];
    u32s(&mut body, 3, true);
    body.extend([4, 5, 6, 0]);
    bytes.extend(block(3, &body, true));
    bytes.extend(block(0xdeadbeef, &[], true));
    bytes.extend(section(false));
    bytes.extend(interface(false, 6, 0, "third"));
    bytes.extend(enhanced(false, 0, 1234567, &[7]));
    let c = parse(&bytes).unwrap();
    assert_eq!(c.interfaces.len(), 3);
    assert_eq!(c.total_packets, 3);
    assert_eq!(c.auxiliary_blocks, 1);
    assert_eq!(c.packets[0].interface, 1);
    assert_eq!(c.packets[0].timestamp.unwrap().unix_nanos(), 6_500_000_000);
    assert_eq!(c.packets[1].timestamp, None);
    assert_eq!(c.packets[1].bytes, [4, 5, 6]);
    assert_eq!(c.packets[2].interface, 2);
    assert_eq!(c.interfaces[2].section, 1);
    assert_eq!(c.packets[2].timestamp.unwrap().ticks, 1234567);
    assert_eq!(c.file_bytes, bytes.len() as u64);
    for original in &c.packets {
        let recovered = lookup::read_packet(
            Cursor::new(&bytes),
            &lookup::PacketAnchor::new(&c, original.number).unwrap(),
            &AtomicBool::new(false),
            &analysis::Progress::default(),
        )
        .unwrap();
        let interface = &c.interfaces[original.interface];
        assert_eq!(recovered.packet.bytes, original.bytes);
        assert_eq!(recovered.packet.timestamp, original.timestamp);
        assert_eq!(recovered.packet.file_offset, original.file_offset);
        assert_eq!(recovered.packet.interface, original.interface);
        assert_eq!(recovered.interface.section, interface.section);
        assert_eq!(recovered.interface.resolution, interface.resolution);
        assert_eq!(recovered.interface.offset_seconds, interface.offset_seconds);
    }
}
#[test]
fn options_and_headers_cannot_hide_truncated_or_foreign_data() {
    let mut bytes = section(true);
    bytes.extend(interface(true, 6, 0, "synthetic"));
    let epb = enhanced(true, 0, 1, &[1, 2, 3]);
    let mut bad = bytes.clone();
    bad.extend(enhanced(true, 1, 1, &[1]));
    assert!(parse(&bad).is_err());
    for trim in [1, 2, 3, 5] {
        let mut bad = bytes.clone();
        bad.extend(&epb[..epb.len() - trim]);
        assert!(parse(&bad).is_err());
    }
    let mut bad = bytes.clone();
    let mut epb_bad = epb.clone();
    let end = epb_bad.len() - 4;
    epb_bad[end] = 0;
    bad.extend(epb_bad);
    assert!(parse(&bad).is_err());
    let mut bad = bytes.clone();
    let mut epb_bad = epb.clone();
    epb_bad[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    bad.extend(epb_bad);
    assert!(parse(&bad).is_err());
    let mut bad = section(true);
    let mut body = vec![0; 8];
    option(&mut body, 9, &[6], true);
    option(&mut body, 9, &[9], true);
    bad.extend(block(1, &body, true));
    assert!(parse(&bad).is_err());
    let mut bad = bytes.clone();
    bad.extend([6, 0, 0, 0, 0xff, 0xff, 0xff, 0x7f, 0, 0, 0, 0]);
    assert!(parse(&bad).is_err());
    assert!(parse(b"not a capture").is_err());
}
#[test]
fn classic_lengths_fraction_and_trailing_header_are_checked() {
    let bytes = pcap(true, false, &[1, 2, 3]);
    for index in [28, 32] {
        let mut bad = bytes.clone();
        bad[index..index + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&bad).is_err());
    }
    let mut bad = bytes.clone();
    bad[36..40].copy_from_slice(&2u32.to_le_bytes());
    assert!(parse(&bad).is_err());
    assert!(parse(&bytes[..bytes.len() - 1]).is_err());
    let mut bad = bytes.clone();
    bad.extend([0]);
    assert!(parse(&bad).is_err());
    let mut truncated = bytes.clone();
    truncated[36..40].copy_from_slice(&4u32.to_le_bytes());
    assert_eq!(parse(&truncated).unwrap().truncated_packets, 1);
}
#[test]
fn empty_files_and_header_only_captures_are_distinct() {
    assert!(parse(&[]).is_err());
    assert_eq!(
        parse(&pcap(true, false, &[])[..24]).unwrap().total_packets,
        0
    );
    assert_eq!(parse(&section(true)).unwrap().total_packets, 0);
}
#[test]
fn retained_prefix_is_bounded_but_entire_file_is_validated_and_hashed() {
    let header = pcap(true, false, &[]);
    let mut bytes = header[..24].to_vec();
    for _ in 0..MAX_PACKETS + 2 {
        bytes.extend(&header[24..]);
    }
    let c = parse(&bytes).unwrap();
    assert_eq!(c.packets.len(), MAX_PACKETS);
    assert_eq!(c.omitted_packets, 2);
    assert_eq!(c.total_packets, MAX_PACKETS as u64 + 2);
    assert!(c.limited());
    let previous_hash = c.sha256;
    let at = bytes.len() - 16;
    bytes[at..at + 4].copy_from_slice(&9u32.to_le_bytes());
    assert_ne!(parse(&bytes).unwrap().sha256, previous_hash);
    bytes.push(1);
    assert!(parse(&bytes).is_err());
}
#[test]
fn byte_budget_retains_a_contiguous_prefix_and_checks_omitted_packets() {
    let header = pcap(true, false, &[]);
    let mut bytes = header[..24].to_vec();
    bytes[16..20].copy_from_slice(&(MAX_PACKET_BYTES as u32).to_le_bytes());
    let packet = vec![0; MAX_PACKET_BYTES];
    for _ in 0..33 {
        for value in [0, 0, MAX_PACKET_BYTES as u32, MAX_PACKET_BYTES as u32] {
            u32s(&mut bytes, value, true);
        }
        bytes.extend(&packet);
    }
    // A smaller record after the first omission must also stay omitted.
    bytes.extend(&header[24..]);
    let capture = parse(&bytes).unwrap();
    assert_eq!(capture.retained_bytes, MAX_RETAINED_BYTES);
    assert_eq!(capture.packets.len(), 32);
    assert_eq!(capture.total_packets, 34);
    assert_eq!(capture.omitted_packets, 2);
    bytes.push(1);
    assert!(parse(&bytes).is_err());
}
#[test]
fn interface_names_stop_at_nul_and_options_need_no_explicit_terminator() {
    let mut bytes = section(true);
    let mut body = vec![0; 8];
    option(&mut body, 2, "Ethernet\0hidden".as_bytes(), true);
    bytes.extend(block(1, &body, true));
    let capture = parse(&bytes).unwrap();
    assert_eq!(capture.interfaces[0].name, "Ethernet");
}
#[test]
fn declared_section_lengths_are_checked_at_eof_and_between_sections() {
    let iface = interface(true, 6, 0, "synthetic");
    let mut bytes = section(true);
    bytes[16..24].copy_from_slice(&(iface.len() as i64).to_le_bytes());
    bytes.extend(iface);
    assert!(parse(&bytes).is_ok());
    let mut short = bytes.clone();
    short[16..24].copy_from_slice(&0i64.to_le_bytes());
    assert!(parse(&short).is_err());
    let mut long = bytes.clone();
    long[16..24].copy_from_slice(&4096i64.to_le_bytes());
    assert!(parse(&long).is_err());
    bytes.extend(section(false));
    assert!(parse(&bytes).is_ok());
    long.extend(section(false));
    assert!(parse(&long).is_err());
}
#[test]
fn cancellation_interrupts_mid_import_instead_of_retrying_forever() {
    struct Cancelling<'a> {
        reader: Cursor<Vec<u8>>,
        cancel: &'a AtomicBool,
        reads: usize,
    }
    impl Read for Cancelling<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let n = self.reader.read(out)?;
            self.reads += 1;
            if self.reads == 2 {
                self.cancel.store(true, Ordering::Relaxed);
            }
            Ok(n)
        }
    }
    let cancel = AtomicBool::new(false);
    let input = Cancelling {
        reader: Cursor::new(pcap(true, false, &[1])),
        cancel: &cancel,
        reads: 0,
    };
    assert!(format!("{:#}", read(input, &cancel).unwrap_err()).contains("cancelada"));
}
#[test]
fn load_preserves_source_and_does_not_depend_on_extension() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture.data");
    let data = pcap(false, true, &[1, 2]);
    std::fs::write(&path, &data).unwrap();
    let c = load(&path, &AtomicBool::new(false)).unwrap();
    assert_eq!(c.file_bytes, data.len() as u64);
    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert!(load(dir.path(), &AtomicBool::new(false)).is_err());
}
#[test]
fn native_link_types_and_fragmentation_remain_honest() {
    let demo = example();
    let p = &demo.packets[0];
    let s = p.summary(1);
    assert_eq!(s.protocol, "UDP");
    assert_eq!(s.source, "192.0.2.10:50000");
    assert!(s.info.contains("estimativa"));
    let mut bytes = p.bytes.clone();
    bytes[20..22].copy_from_slice(&0x2000u16.to_be_bytes());
    let p = Packet { bytes, ..p.clone() };
    let s = p.summary(1);
    assert_eq!(s.protocol, "IPv4");
    assert!(s.info.contains("Fragmento"));
    assert_eq!(p.summary(999).protocol, "OUTRO");
    for len in 0..p.bytes.len() {
        let truncated = Packet {
            bytes: p.bytes[..len].to_vec(),
            ..p.clone()
        };
        let _ = truncated.summary(1);
    }
}
#[test]
fn ipv6_extensions_tcp_and_linux_cooked_are_decoded_without_payload_inspection() {
    let mut ipv6 = vec![0; 60];
    ipv6[0] = 0x60;
    ipv6[4..6].copy_from_slice(&20u16.to_be_bytes());
    ipv6[6] = 6;
    ipv6[8..24].copy_from_slice(
        &"2001:db8::1"
            .parse::<std::net::Ipv6Addr>()
            .unwrap()
            .octets(),
    );
    ipv6[24..40].copy_from_slice(
        &"2001:db8::2"
            .parse::<std::net::Ipv6Addr>()
            .unwrap()
            .octets(),
    );
    ipv6[40..42].copy_from_slice(&1000u16.to_be_bytes());
    ipv6[42..44].copy_from_slice(&443u16.to_be_bytes());
    ipv6[52] = 0x50;
    ipv6[53] = 0x12;
    let p = Packet {
        number: 1,
        file_offset: 0,
        interface: 0,
        timestamp: None,
        original_len: 60,
        captured_len: ipv6.len() as u32,
        bytes: ipv6.clone(),
    };
    let s = p.summary(101);
    assert_eq!(s.protocol, "TCP");
    assert_eq!(s.destination, "[2001:db8::2]:443");
    assert!(s.info.starts_with("SYN ACK"));
    let mut cooked = vec![0; 20];
    cooked[..2].copy_from_slice(&0x86ddu16.to_be_bytes());
    cooked.extend(ipv6);
    let p = Packet { bytes: cooked, ..p };
    assert_eq!(p.summary(276).protocol, "TCP");
    let mut ip = p.bytes[20..].to_vec();
    ip[6] = 0;
    ip[40] = 44;
    ip[41] = 0;
    let p = Packet { bytes: ip, ..p };
    assert!(p.summary(101).info.contains("Fragmento IPv6"));
}
#[test]
fn timestamp_negative_offset_binary_resolution_and_hex_preview_are_bounded() {
    assert_eq!(
        Timestamp {
            ticks: 0,
            resolution: 6,
            offset_seconds: -1
        }
        .clock_label(),
        "23:59:59.000000"
    );
    assert_eq!(
        Timestamp {
            ticks: 1024,
            resolution: 0x8a,
            offset_seconds: -2
        }
        .unix_nanos(),
        -1_000_000_000
    );
    assert_eq!(
        Timestamp {
            ticks: u64::MAX,
            resolution: 127,
            offset_seconds: 0
        }
        .unix_nanos(),
        0
    );
    let p = Packet {
        number: 1,
        file_offset: 0,
        interface: 0,
        timestamp: None,
        original_len: 4096,
        captured_len: 4096,
        bytes: vec![b'A'; 4096],
    };
    let preview = p.hex_preview();
    assert_eq!(preview.lines().count(), 17);
    assert!(preview.len() < 1400);
}
