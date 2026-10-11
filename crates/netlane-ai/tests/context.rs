use netlane_ai::*;
fn fixture() -> Context {
    Context {
        investigation_id: "case-1".into(),
        sha256: "A".repeat(64),
        format: "PCAPNG".into(),
        total_packets: 5,
        inspected_packets: 4,
        omitted_packets: 1,
        analysis: None,
        packets: vec![PacketEvidence {
            number: 2,
            interface: "Ethernet".into(),
            section: 0,
            link_type: 1,
            unix_nanos: Some("123456789".into()),
            captured_bytes: 42,
            original_bytes: 60,
            protocol: "UDP".into(),
            source: "192.0.2.10:50000".into(),
            destination: "198.51.100.20:53".into(),
            observation: "Porta DNS (estimativa)".into(),
        }],
    }
}
#[test]
fn saturated_selection_can_be_replaced_without_changing_prior_snapshot() {
    let mut previous = fixture();
    previous.total_packets = 20;
    previous.inspected_packets = 20;
    previous.omitted_packets = 0;
    previous.packets = (1..=16)
        .map(|number| PacketEvidence {
            number,
            ..fixture().packets.remove(0)
        })
        .collect();
    let original = previous.clone();
    let mut next = previous.clone();
    next.investigation_id = "next".into();
    next.packets = vec![PacketEvidence {
        number: 17,
        ..fixture().packets.remove(0)
    }];
    assert!(next.clone().select(Some(&previous), false).is_err());
    let replaced = next.select(Some(&previous), true).unwrap();
    assert_eq!(replaced.packets.len(), 1);
    assert_eq!(replaced.packets[0].number, 17);
    assert_eq!(replaced.investigation_id, previous.investigation_id);
    assert_eq!(previous, original);
}
#[test]
fn context_has_traceable_selected_metadata_and_no_raw_bytes_or_paths() {
    let c = fixture();
    let prompt = c.prompt("falha de rede", "O que foi observado?").unwrap();
    assert!(prompt.contains("\"investigation_id\": \"case-1\""));
    assert!(prompt.contains("192.0.2.10:50000"));
    assert!(prompt.contains("\"omitted_packets\": 1"));
    let json = serde_json::to_value(c).unwrap();
    assert!(json.get("path").is_none());
    assert!(json["packets"][0].get("bytes").is_none());
}
#[test]
fn context_rejects_foreign_packets_duplicates_overflow_and_invalid_identity() {
    let mut c = fixture();
    c.packets[0].number = 5;
    assert!(c.validate().is_err());
    let mut c = fixture();
    c.packets.push(c.packets[0].clone());
    assert!(c.validate().is_err());
    let mut c = fixture();
    c.omitted_packets = u64::MAX;
    assert!(c.validate().is_err());
    let mut c = fixture();
    c.sha256 = "bad".into();
    assert!(c.validate().is_err());
    let mut c = fixture();
    c.investigation_id = "../another".into();
    assert!(c.validate().is_err());
    let mut c = fixture();
    c.packets[0].interface = "bad\nname".into();
    assert!(c.validate().is_err());
    let mut c = fixture();
    c.packets[0].unix_nanos = Some("invented".into());
    assert!(c.validate().is_err());
}
#[test]
fn prompts_are_bounded_and_empty_questions_are_not_sent() {
    let c = fixture();
    assert!(c.prompt("", " ").is_err());
    assert!(c.prompt(&"x".repeat(MAX_PROBLEM_BYTES + 1), "q").is_err());
    let mut c = fixture();
    c.packets[0].observation = "x".repeat(513);
    assert!(c.validate().is_err());
    let mut c = fixture();
    c.packets.clear();
    assert!(c.validate().is_err());
    let mut c = fixture();
    c.total_packets = 100;
    c.inspected_packets = 99;
    c.packets = (1..=MAX_PACKETS + 1)
        .map(|i| PacketEvidence {
            number: i as u64,
            ..fixture().packets.remove(0)
        })
        .collect();
    assert!(c.validate().is_err());
}
#[test]
fn references_must_belong_to_selected_packets_and_capture() {
    let c = fixture();
    let good = format!("Observado [[packet:{}:2]].", c.sha256);
    assert!(c.validate_references(&good).is_ok());
    assert!(
        c.validate_references(&good.replace(":2]]", ":3]]"))
            .is_err()
    );
    assert!(c.validate_references(&good.replace('A', "B")).is_err());
    assert!(c.validate_references("[[packet:missing").is_err());
    assert!(c.validate_references(&"x".repeat(128 * 1024 + 1)).is_err());
}

fn aggregate_fixture() -> Context {
    let mut c = fixture();
    c.total_packets = 100_000;
    c.inspected_packets = 100_000;
    c.omitted_packets = 0;
    c.packets[0].number = 99_999;
    c.analysis = Some(AnalysisEvidence {
        summary: "Totais de todos os pacotes; amostra distribuída para inspeção.".into(),
        analyzed_packets: 100_000,
        sample_packets: 512,
        baseline_sha256: Some("B".repeat(64)),
        packet_references: vec![50_001],
    });
    c
}
#[test]
fn aggregate_context_accepts_sparse_samples_and_explicit_finding_anchors() {
    let c = aggregate_fixture();
    assert!(c.prompt("", "Compare com a referência").is_ok());
    for number in [99_999, 50_001] {
        assert!(
            c.validate_references(&format!("[[packet:{}:{number}]]", c.sha256))
                .is_ok()
        );
    }
    assert!(
        c.validate_references(&format!("[[packet:{}:50_002]]", c.sha256))
            .is_err()
    );
    assert!(
        c.validate_references(&format!("[[packet:{}:50_001]]", "B".repeat(64)))
            .is_err()
    );
    let old_json = serde_json::to_value(fixture()).unwrap();
    assert!(old_json.get("analysis").is_none());
    assert!(
        serde_json::from_value::<Context>(old_json)
            .unwrap()
            .validate()
            .is_ok()
    );
}
#[test]
fn aggregate_context_rejects_inconsistent_totals_and_unbounded_or_foreign_anchors() {
    let mut c = aggregate_fixture();
    c.analysis.as_mut().unwrap().analyzed_packets -= 1;
    assert!(c.validate().is_err());
    let mut c = aggregate_fixture();
    c.analysis.as_mut().unwrap().sample_packets = 100_001;
    assert!(c.validate().is_err());
    for number in [0, 100_001] {
        let mut c = aggregate_fixture();
        c.analysis.as_mut().unwrap().packet_references = vec![number];
        assert!(c.validate().is_err());
    }
    let mut c = aggregate_fixture();
    c.analysis.as_mut().unwrap().packet_references = vec![1; 25];
    assert!(c.validate().is_err());
    let mut c = aggregate_fixture();
    c.analysis.as_mut().unwrap().summary = "x".repeat(8193);
    assert!(c.validate().is_err());
    let mut c = aggregate_fixture();
    c.analysis.as_mut().unwrap().baseline_sha256 = Some("invalid".into());
    assert!(c.validate().is_err());
}
