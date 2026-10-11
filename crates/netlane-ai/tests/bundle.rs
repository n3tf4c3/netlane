use netlane_ai::{
    AnalysisEvidence, Context, PacketEvidence,
    bundle::{self, Progress, Source},
    investigation::Notebook,
    report::{Draft, Response},
};
use netlane_capture::{
    analysis::{self, Scan},
    lookup, qa,
};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

fn scan(path: &Path) -> Scan {
    analysis::load_dump(
        path,
        &AtomicBool::new(false),
        &analysis::Progress::default(),
    )
    .unwrap()
}
fn context(scan: &Scan, number: u64, baseline: Option<String>) -> Context {
    let packet = lookup::load_packet(
        scan.source.as_ref().unwrap(),
        &lookup::PacketAnchor::new(&scan.capture, number).unwrap(),
        &AtomicBool::new(false),
        &analysis::Progress::default(),
    )
    .unwrap();
    let p = &packet.packet;
    Context {
        investigation_id: "bundle-fixture".into(),
        sha256: scan.capture.sha256.clone(),
        format: scan.capture.format.label().into(),
        total_packets: scan.capture.total_packets,
        inspected_packets: scan.capture.total_packets as usize,
        omitted_packets: 0,
        analysis: Some(AnalysisEvidence {
            summary: "Fixture local fictícia.".into(),
            analyzed_packets: scan.capture.total_packets,
            sample_packets: scan.capture.packets.len(),
            baseline_sha256: baseline,
            packet_references: vec![number],
        }),
        packets: vec![PacketEvidence {
            number,
            interface: packet.interface.name,
            section: packet.interface.section,
            link_type: packet.interface.link_type,
            unix_nanos: p.timestamp.map(|t| t.unix_nanos().to_string()),
            captured_bytes: p.captured_len as usize,
            original_bytes: p.original_len,
            protocol: packet.summary.protocol.into(),
            source: packet.summary.source,
            destination: packet.summary.destination,
            observation: packet.summary.info,
        }],
    }
}
fn notebook(c: &Context) -> Notebook {
    let mut book = Notebook::default();
    book.record(
        7,
        Draft {
            context: c.clone(),
            problem: "Intermitência fictícia".into(),
            question: "O que conferir?".into(),
            response: Some(Response {
                provider: "Codex simulado".into(),
                model: "fixture".into(),
                status: "Concluído; revisar".into(),
                text: format!(
                    "Hipótese a conferir [[packet:{}:{}]]",
                    c.sha256, c.packets[0].number
                ),
            }),
        },
    )
    .unwrap();
    book.add_note("Conferir no original", c.clone(), Some(1))
        .unwrap();
    book.cycle_note(1).unwrap();
    book
}
struct Fixture {
    root: tempfile::TempDir,
    current: Context,
    book: Notebook,
    sources: Vec<Source>,
}
fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    qa::generate(
        &root.path().join("originals"),
        qa::FixtureOptions {
            records: 2000,
            packet_bytes: 2112,
        },
    )
    .unwrap();
    let base = scan(&root.path().join("originals/baseline.pcap"));
    let deviation = scan(&root.path().join("originals/deviation.pcap"));
    let number = (1001..2000)
        .find(|n| !deviation.capture.packets.iter().any(|p| p.number == *n))
        .unwrap();
    let current = context(&deviation, number, Some(base.capture.sha256.clone()));
    let book = notebook(&current);
    let sources = vec![
        Source::new(base.source.clone().unwrap(), &base.capture),
        Source::new(deviation.source.clone().unwrap(), &deviation.capture),
    ];
    Fixture {
        root,
        current,
        book,
        sources,
    }
}
fn save(f: &Fixture, folder: &Path) -> bundle::Manifest {
    bundle::save(
        folder,
        &f.book,
        &f.current,
        &f.sources,
        &AtomicBool::new(false),
        &Progress::default(),
    )
    .unwrap()
}
#[test]
fn full_investigation_reopens_after_move_without_originals() {
    let f = fixture();
    let folder = f.root.path().join("saved");
    let manifest = save(&f, &folder);
    assert_eq!(manifest.captures.len(), 2);
    for (entry, source) in manifest.captures.iter().map(|e| {
        (
            e,
            f.sources
                .iter()
                .find(|s| s.sha256.eq_ignore_ascii_case(&e.sha256))
                .unwrap(),
        )
    }) {
        assert_eq!(
            fs::read(folder.join(&entry.file)).unwrap(),
            fs::read(&source.path).unwrap()
        );
    }
    fs::rename(
        f.root.path().join("originals"),
        f.root.path().join("originals-moved"),
    )
    .unwrap();
    let moved = f.root.path().join("moved-package");
    fs::rename(&folder, &moved).unwrap();
    let progress = Progress::default();
    let opened = bundle::load(&moved, &AtomicBool::new(false), &progress).unwrap();
    assert_eq!(
        serde_json::to_value(&opened.notebook).unwrap(),
        serde_json::to_value(&f.book).unwrap()
    );
    assert_eq!(opened.current, f.current);
    assert!(opened.sources.iter().all(|s| s.path.starts_with(&moved)));
    assert_eq!(opened.scan.capture.sha256, f.current.sha256);
    assert_eq!(opened.baseline.unwrap().packets, 1000);
    let recovered = opened.recovered.unwrap();
    assert_eq!(recovered.packet.number, f.current.packets[0].number);
    assert_eq!(recovered.packet.bytes.len(), 2048);
    assert_eq!(recovered.packet.captured_len, 2112);
    assert_eq!(
        progress.bytes.load(Ordering::Relaxed),
        progress.total.load(Ordering::Relaxed)
    );
    assert_eq!(progress.files.load(Ordering::Relaxed), 2);
    assert!(moved.join("reports/turn-0001/report.html").is_file());
    let manifest_text = fs::read_to_string(moved.join("bundle.json")).unwrap();
    assert!(!manifest_text.contains(&f.root.path().to_string_lossy().replace('\\', "\\\\")));
}
#[test]
fn missing_sources_and_existing_destinations_are_not_silently_accepted() {
    let f = fixture();
    let folder = f.root.path().join("saved");
    assert!(
        bundle::save(
            &folder,
            &f.book,
            &f.current,
            &f.sources[..1],
            &AtomicBool::new(false),
            &Progress::default()
        )
        .is_err()
    );
    assert!(!folder.exists());
    save(&f, &folder);
    let before = fs::read(folder.join("bundle.json")).unwrap();
    assert!(
        bundle::save(
            &folder,
            &f.book,
            &f.current,
            &f.sources,
            &AtomicBool::new(false),
            &Progress::default()
        )
        .is_err()
    );
    assert_eq!(fs::read(folder.join("bundle.json")).unwrap(), before);
}
#[test]
fn changed_source_preserves_partial_files_without_completion_manifest() {
    let f = fixture();
    let source = &f.sources[1];
    let mut bytes = fs::read(&source.path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&source.path, bytes).unwrap();
    let folder = f.root.path().join("incomplete");
    assert!(
        bundle::save(
            &folder,
            &f.book,
            &f.current,
            &f.sources,
            &AtomicBool::new(false),
            &Progress::default()
        )
        .is_err()
    );
    assert!(!folder.join("bundle.json").exists());
    assert!(folder.join("notebook/receipt.json").exists());
    assert!(bundle::load(&folder, &AtomicBool::new(false), &Progress::default()).is_err());
}
#[test]
fn malicious_manifest_paths_counts_versions_and_duplicates_are_refused() {
    let f = fixture();
    let folder = f.root.path().join("saved");
    save(&f, &folder);
    let path = folder.join("bundle.json");
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for case in 0..7 {
        let mut bad = original.clone();
        match case {
            0 => bad["captures"][0]["file"] = "../../outside.pcap".into(),
            1 => bad["captures"][0]["file"] = "captures/x.pcap:secret".into(),
            2 => bad["schema"] = 2.into(),
            3 => bad["captures"][0]["bytes"] = (bundle::MAX_CAPTURE_BYTES + 1).into(),
            4 => {
                let entry = bad["captures"][0].clone();
                bad["captures"].as_array_mut().unwrap().push(entry);
            }
            5 => bad["current"]["total_packets"] = 1.into(),
            _ => {
                let entry = bad["captures"][0].clone();
                bad["captures"] = serde_json::Value::Array(vec![entry; bundle::MAX_CAPTURES + 1]);
            }
        }
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(
            bundle::load(&folder, &AtomicBool::new(false), &Progress::default()).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn corrupted_or_missing_dump_and_changed_report_are_refused() {
    let f = fixture();
    let folder = f.root.path().join("saved");
    let manifest = save(&f, &folder);
    let path = folder.join(&manifest.captures[0].file);
    let original = fs::read(&path).unwrap();
    let mut bad = original.clone();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    fs::write(&path, bad).unwrap();
    assert!(bundle::load(&folder, &AtomicBool::new(false), &Progress::default()).is_err());
    fs::write(&path, &original).unwrap();
    let moved = path.with_extension("unavailable");
    fs::rename(&path, &moved).unwrap();
    assert!(bundle::load(&folder, &AtomicBool::new(false), &Progress::default()).is_err());
    fs::rename(&moved, &path).unwrap();
    fs::write(folder.join("reports/turn-0001/report.html"), b"altered").unwrap();
    assert!(bundle::load(&folder, &AtomicBool::new(false), &Progress::default()).is_err());
}
#[test]
fn cancellation_before_and_during_copy_does_not_publish_a_package() {
    let f = fixture();
    let folder = f.root.path().join("cancelled");
    assert!(
        bundle::save(
            &folder,
            &f.book,
            &f.current,
            &f.sources,
            &AtomicBool::new(true),
            &Progress::default()
        )
        .is_err()
    );
    assert!(!folder.exists());
    let cancel = AtomicBool::new(false);
    let done = AtomicBool::new(false);
    let progress = Progress::default();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while progress.bytes.load(Ordering::Relaxed) == 0 && !done.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            cancel.store(true, Ordering::Relaxed);
        });
        let result = bundle::save(&folder, &f.book, &f.current, &f.sources, &cancel, &progress);
        done.store(true, Ordering::Relaxed);
        assert!(result.is_err());
    });
    assert!(progress.bytes.load(Ordering::Relaxed) > 0);
    assert!(!folder.join("bundle.json").exists());
}
#[test]
fn pcapng_sources_and_untrusted_response_records_roundtrip() {
    let mut f = fixture();
    let ng = scan(&f.root.path().join("originals/pcapng/selected.pcapng"));
    f.current = context(&ng, 4.min(ng.capture.total_packets), None);
    f.book = notebook(&f.current);
    let mut invalid = f.book.turns()[0].draft.clone();
    invalid.response.as_mut().unwrap().text = format!(
        "Referência fora da seleção [[packet:{}:999999]]",
        f.current.sha256
    );
    f.book.record(7, invalid).unwrap();
    assert!(!f.book.turns()[1].references_valid);
    f.sources = vec![Source::new(ng.source.unwrap(), &ng.capture)];
    let folder = f.root.path().join("ng-package");
    save(&f, &folder);
    let opened = bundle::load(&folder, &AtomicBool::new(false), &Progress::default()).unwrap();
    assert_eq!(opened.scan.capture.format, netlane_capture::Format::PcapNg);
    assert_eq!(opened.current, f.current);
    assert_eq!(opened.scan.capture.total_packets, 3);
    assert_eq!(opened.notebook.turns().len(), 2);
    assert!(!opened.notebook.turns()[1].references_valid);
    assert!(folder.join("reports/turn-0001/report.md").exists());
    assert!(!folder.join("reports/turn-0002").exists());
}
#[test]
fn editable_resume_inputs_and_collection_provenance_are_preserved() {
    let mut f = fixture();
    f.sources[1].provenance =
        Some("Origem fictícia: perdas simuladas, investigar antes de concluir.".into());
    let inputs = bundle::Inputs {
        problem: "Problema editado depois da resposta".into(),
        question: String::new(),
    };
    let folder = f.root.path().join("with-inputs");
    bundle::save_with_inputs(
        &folder,
        &f.book,
        &f.current,
        &f.sources,
        &inputs,
        &AtomicBool::new(false),
        &Progress::default(),
    )
    .unwrap();
    let opened = bundle::load(&folder, &AtomicBool::new(false), &Progress::default()).unwrap();
    assert_eq!(opened.inputs.problem, inputs.problem);
    assert!(opened.inputs.question.is_empty());
    assert_eq!(
        opened.notebook.turns()[0].draft.problem,
        "Intermitência fictícia"
    );
    assert_eq!(
        opened
            .sources
            .iter()
            .find(|s| s.sha256 == f.current.sha256)
            .unwrap()
            .provenance,
        f.sources[1].provenance
    );
}
#[test]
fn oversized_resume_draft_is_refused_before_creating_output() {
    let f = fixture();
    let folder = f.root.path().join("too-big");
    let inputs = bundle::Inputs {
        problem: String::new(),
        question: "x".repeat(4097),
    };
    assert!(
        bundle::save_with_inputs(
            &folder,
            &f.book,
            &f.current,
            &f.sources,
            &inputs,
            &AtomicBool::new(false),
            &Progress::default()
        )
        .is_err()
    );
    assert!(!folder.exists());
}
#[test]
fn past_conversation_and_note_dumps_remain_available_beside_current_capture() {
    let mut f = fixture();
    let ng = scan(&f.root.path().join("originals/pcapng/selected.pcapng"));
    let other = context(&ng, 2, None);
    let mut turn = f.book.turns()[0].draft.clone();
    turn.context = other.clone();
    turn.response.as_mut().unwrap().text =
        format!("Outra evidência fictícia [[packet:{}:2]]", other.sha256);
    f.book.record(8, turn).unwrap();
    f.book
        .add_note("Outra coleta da mesma investigação", other.clone(), Some(2))
        .unwrap();
    f.sources.push(Source::new(ng.source.unwrap(), &ng.capture));
    let folder = f.root.path().join("multi-source");
    let manifest = save(&f, &folder);
    assert_eq!(manifest.captures.len(), 3);
    fs::rename(
        f.root.path().join("originals"),
        f.root.path().join("unavailable-originals"),
    )
    .unwrap();
    let opened = bundle::load(&folder, &AtomicBool::new(false), &Progress::default()).unwrap();
    assert_eq!(opened.current, f.current);
    assert_eq!(opened.notebook.turns()[1].draft.context, other);
    assert_eq!(opened.notebook.notes()[1].context, other);
    let historical = opened
        .sources
        .iter()
        .find(|s| s.sha256 == other.sha256)
        .unwrap();
    let scan =
        bundle::read_source(historical, &AtomicBool::new(false), &Progress::default()).unwrap();
    assert_eq!(scan.capture.format, netlane_capture::Format::PcapNg);
    assert_eq!(scan.capture.total_packets, other.total_packets);
    assert!(folder.join("reports/turn-0002/report.md").exists());
}
#[cfg(windows)]
#[test]
fn junction_in_source_parent_is_refused_without_touching_the_target() {
    let mut f = fixture();
    let link = f.root.path().join("junction");
    let original = f.root.path().join("originals");
    assert!(
        std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&original)
            .output()
            .unwrap()
            .status
            .success()
    );
    f.sources[1].path = link.join("deviation.pcap");
    let folder = f.root.path().join("rejected-link");
    assert!(
        bundle::save(
            &folder,
            &f.book,
            &f.current,
            &f.sources,
            &AtomicBool::new(false),
            &Progress::default()
        )
        .is_err()
    );
    assert!(!folder.join("bundle.json").exists());
    fs::remove_dir(&link).unwrap(); // Removes only the owned junction, never its target.
    assert!(original.join("deviation.pcap").is_file());
}
