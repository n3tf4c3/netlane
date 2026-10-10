use netlane_ai::{
    Context, PacketEvidence,
    report::{Draft, Response},
};
use sha2::{Digest, Sha256};
fn draft() -> Draft {
    Draft {
        context: Context {
            investigation_id: "case-1".into(),
            sha256: "a".repeat(64),
            format: "PCAPNG".into(),
            total_packets: 100,
            inspected_packets: 100,
            omitted_packets: 0,
            analysis: Some(netlane_ai::AnalysisEvidence {
                summary: "Origem: exemplo fictício. Agregados de 100 pacotes, 1 amostra.".into(),
                analyzed_packets: 100,
                sample_packets: 1,
                baseline_sha256: Some("b".repeat(64)),
                packet_references: vec![99],
            }),
            packets: vec![PacketEvidence {
                number: 2,
                interface: "Lab".into(),
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
        },
        problem: "Falha intermitente".into(),
        question: "O que verificar?".into(),
        response: None,
    }
}
#[test]
fn literal_user_and_provider_text_cannot_inject_html_markdown_or_remote_resources() {
    let mut d = draft();
    d.problem =
        "</pre><script>alert(1)</script>\n![remote](https://example.com/a)\n```\n# fake title\n```"
            .into();
    d.response = Some(Response {
        provider: "Codex".into(),
        model: "example-model".into(),
        status: "Turno parcial".into(),
        text: format!(
            "<img src=x onerror=alert(1)> [[packet:{}:99]]",
            d.context.sha256
        ),
    });
    let html = d.html().unwrap();
    assert!(!html.contains("<script>"));
    assert!(!html.contains("<img "));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("default-src 'none'"));
    let md = d.markdown().unwrap();
    assert!(md.contains("    # fake title"));
    assert!(md.contains("    ![remote]"));
    assert!(md.contains("Turno parcial"));
    assert!(md.contains("último turno"));
}
#[test]
fn report_bundle_has_correct_hashes_fixed_evidence_and_no_payload_or_auth_fields() {
    let d = draft();
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("case");
    let r = d.save(&dir).unwrap();
    assert!(!r.ai_included);
    assert!(!r.references_checked);
    for f in &r.files {
        let bytes = std::fs::read(dir.join(&f.file)).unwrap();
        assert_eq!(f.bytes, bytes.len());
        assert_eq!(f.sha256, format!("{:x}", Sha256::digest(bytes)));
    }
    let snapshot: Draft =
        serde_json::from_slice(&std::fs::read(dir.join("context.json")).unwrap()).unwrap();
    assert_eq!(snapshot.context, d.context);
    assert_eq!(snapshot.problem, d.problem);
    let json = std::fs::read_to_string(dir.join("context.json")).unwrap();
    assert!(!json.contains("auth.json"));
    assert!(!json.contains("payload"));
    assert!(!json.contains("file_path"));
    let md = std::fs::read(dir.join("report.md")).unwrap();
    assert!(d.save(&dir).is_err());
    assert_eq!(std::fs::read(dir.join("report.md")).unwrap(), md);
    assert!(dir.join("receipt.json").exists());
}
#[test]
fn invalid_response_reference_is_rejected_without_creating_a_report() {
    let mut d = draft();
    let temp = tempfile::tempdir().unwrap();
    for (n, text) in [
        format!("[[packet:{}:3]]", d.context.sha256),
        format!("[[packet:{}:2]]", "c".repeat(64)),
        "[[packet:unfinished".into(),
    ]
    .into_iter()
    .enumerate()
    {
        d.response = Some(Response {
            provider: "Codex".into(),
            model: "test".into(),
            status: "partial".into(),
            text,
        });
        let dir = temp.path().join(n.to_string());
        assert!(d.save(&dir).is_err());
        assert!(!dir.exists());
    }
    d.response = None;
    assert!(d.markdown().unwrap().contains("Sem resposta de IA"));
}
