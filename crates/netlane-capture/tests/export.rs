use netlane_capture::{Format, Timestamp, export::Selection};
use std::sync::atomic::AtomicBool;

#[test]
fn derived_formats_preserve_bytes_lengths_times_and_source_mapping_without_overwrite() {
    let source = netlane_capture::example();
    let temp = tempfile::tempdir().unwrap();
    for (format, name) in [(Format::Pcap, "pcap"), (Format::PcapNg, "ng")] {
        let dir = temp.path().join(name);
        let selection = Selection::new(&source, &[4, 1]).unwrap();
        let receipt = selection.save(&dir, format).unwrap();
        let reopened =
            netlane_capture::load(&dir.join(&receipt.file), &AtomicBool::new(false)).unwrap();
        assert_eq!(receipt.sha256, reopened.sha256);
        assert_eq!(receipt.source_sha256, source.sha256);
        assert_eq!(receipt.source_packets, 6);
        assert_eq!(receipt.clipped_packets, 0);
        for (p, index) in reopened.packets.iter().zip([1, 4]) {
            assert_eq!(p.bytes, source.packets[index].bytes);
            assert_eq!(
                p.timestamp.unwrap().unix_nanos(),
                source.packets[index].timestamp.unwrap().unix_nanos()
            );
        }
        assert_eq!(receipt.packets[0].source_number, 2);
        assert_eq!(receipt.packets[1].source_number, 5);
        let original = std::fs::read(dir.join(&receipt.file)).unwrap();
        assert!(selection.save(&dir, format).is_err());
        assert_eq!(std::fs::read(dir.join(&receipt.file)).unwrap(), original);
        assert!(dir.join("receipt.json").exists());
        assert!(!dir.join("receipt.json.part").exists());
    }
}
#[test]
fn sample_clipping_is_explicit_in_file_and_receipt() {
    let mut source = netlane_capture::example();
    source.packets[0].bytes.truncate(17);
    let temp = tempfile::tempdir().unwrap();
    for (format, name) in [(Format::Pcap, "pcap"), (Format::PcapNg, "ng")] {
        let dir = temp.path().join(name);
        let r = Selection::new(&source, &[0])
            .unwrap()
            .save(&dir, format)
            .unwrap();
        let c = netlane_capture::load(&dir.join(&r.file), &AtomicBool::new(false)).unwrap();
        assert_eq!(r.clipped_packets, 1);
        assert_eq!(r.packets[0].source_captured_bytes, 42);
        assert_eq!(r.packets[0].exported_bytes, 17);
        assert_eq!(c.packets[0].captured_len, 17);
        assert_eq!(c.packets[0].original_len, 42);
        assert_eq!(c.packets[0].bytes, source.packets[0].bytes);
        assert!(r.limitations.contains("DERIVADA"));
    }
}
#[test]
fn pcapng_preserves_mixed_interfaces_binary_clock_negative_offset_and_missing_time() {
    let mut source = netlane_capture::example();
    let mut iface = source.interfaces[0].clone();
    iface.link_type = 101;
    iface.name = "Lab & Ethernet".into();
    source.interfaces.push(iface);
    source.packets[1].interface = 1;
    source.packets[1].timestamp = Some(Timestamp {
        ticks: 123456789,
        resolution: 0x80 | 10,
        offset_seconds: -200000,
    });
    source.packets[2].timestamp = None;
    source.packets[2].bytes.truncate(21);
    let s = Selection::new(&source, &[0, 1, 2]).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("pcap");
    assert!(s.save(&dir, Format::Pcap).is_err());
    assert!(!dir.exists());
    let dir = temp.path().join("ng");
    let r = s.save(&dir, Format::PcapNg).unwrap();
    let c = netlane_capture::load(&dir.join(r.file), &AtomicBool::new(false)).unwrap();
    assert_eq!(c.interfaces[c.packets[1].interface].link_type, 101);
    assert_eq!(c.interfaces[c.packets[1].interface].name, "Lab & Ethernet");
    assert_eq!(c.packets[1].timestamp, source.packets[1].timestamp);
    assert!(c.packets[2].timestamp.is_none());
    assert_eq!(c.packets[2].bytes.len(), 21);
    assert_eq!(c.packets[2].original_len, 42);
}
#[test]
fn invalid_selection_and_unrepresentable_classic_timestamps_fail_before_writes() {
    let mut c = netlane_capture::example();
    assert!(Selection::new(&c, &[]).is_err());
    assert!(Selection::new(&c, &[0, 0]).is_err());
    assert!(Selection::new(&c, &[6]).is_err());
    let temp = tempfile::tempdir().unwrap();
    for (n, ts) in [
        None,
        Some(Timestamp {
            ticks: 0,
            resolution: 6,
            offset_seconds: -1,
        }),
        Some(Timestamp {
            ticks: u64::MAX,
            resolution: 0,
            offset_seconds: 0,
        }),
    ]
    .into_iter()
    .enumerate()
    {
        c.packets[0].timestamp = ts;
        let dir = temp.path().join(n.to_string());
        assert!(
            Selection::new(&c, &[0])
                .unwrap()
                .save(&dir, Format::Pcap)
                .is_err()
        );
        assert!(!dir.exists());
    }
    assert!(
        Selection::new(&c, &[0])
            .unwrap()
            .save(std::path::Path::new("relative"), Format::PcapNg)
            .is_err()
    );
}
