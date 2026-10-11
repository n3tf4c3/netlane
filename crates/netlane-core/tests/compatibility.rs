use anyhow::Result;
use netlane_core::{
    engine::{PolicySession, RoutingEngine},
    persistence::PolicyFile,
    planning::{expand_related, plan},
    traffic::TrafficTracker,
    *,
};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[test]
fn dotnet_case_insensitive_fields_numeric_modes_and_unknown_fields_roundtrip() {
    let json = r#"[{"ApplicationId":"app.exe","ExecutablePath":"C:\\Apps\\app.exe","ROUTEMODE":2,"InterfaceId":"{AA6F3B88-072A-45B6-B0CB-8AFE9567E62F}","FallbackInterfaceId":"D3AE43D2-8203-41D6-9D7F-0B6FA59518FD","InterfaceTypeHint":"wifi","futureSetting":{"value":42}}]"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.json");
    std::fs::write(&path, json).unwrap();
    let file = PolicyFile::new(&path);
    let mut loaded = file.load().unwrap();
    assert_eq!(loaded.policies[0].route_mode, RouteMode::WiFi);
    assert!(loaded.policies[0].enabled);
    assert!(loaded.policies[0].include_related_executables);
    loaded.policies[0].enabled = false;
    let revision = file
        .save(&loaded.policies, loaded.revision.as_deref())
        .unwrap();
    let reread = file.load().unwrap();
    assert_eq!(reread.revision.as_deref(), Some(revision.as_str()));
    assert_eq!(
        reread.policies[0].additional_data["futureSetting"]["value"],
        42
    );
    assert_eq!(
        reread.policies[0].fallback_interface_id.as_deref(),
        Some("D3AE43D2-8203-41D6-9D7F-0B6FA59518FD")
    );
    assert_eq!(
        std::fs::read_to_string(path.with_extension("json.bak")).unwrap(),
        json
    );
}
#[test]
fn bom_and_missing_file_are_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.json");
    let file = PolicyFile::new(&path);
    let loaded = file.load().unwrap();
    assert!(loaded.policies.is_empty());
    assert!(loaded.revision.is_none());
    assert!(!path.exists());
    std::fs::write(&path, b"\xef\xbb\xbf[]").unwrap();
    assert!(file.load().unwrap().policies.is_empty());
}
#[test]
fn invalid_input_is_not_replaced() {
    for input in [
        "null",
        "[null]",
        "[{",
        "[{\"applicationId\":\"x\",\"routeMode\":99}]",
        "[{\"applicationId\":\"x\",\"interfaceId\":\"bad\"}]",
        "[{\"applicationId\":\"X\"},{\"applicationId\":\"x\"}]",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rules.json");
        std::fs::write(&path, input).unwrap();
        assert!(PolicyFile::new(&path).load().is_err(), "{input}");
        assert_eq!(std::fs::read_to_string(path).unwrap(), input);
    }
}
#[test]
fn external_change_is_not_overwritten_and_last_rule_removal_keeps_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.json");
    let file = PolicyFile::new(&path);
    let revision = file
        .save(&[RoutingPolicy::new("C:\\app.exe")], None)
        .unwrap();
    std::fs::write(&path, "[]").unwrap();
    assert!(file.save(&[], Some(&revision)).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[]");
    let revision = file.load().unwrap().revision.unwrap();
    file.save(&[], Some(&revision)).unwrap();
    assert!(file.load().unwrap().policies.is_empty());
    assert!(path.with_extension("json.bak").exists());
}
#[test]
fn oversized_serialization_preserves_readable_rules_revision_and_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rules.json");
    let mut policy = RoutingPolicy::new("C:\\app.exe");
    policy.additional_data.insert(
        "futureSetting".into(),
        serde_json::json!(vec![0u8; 600_000]),
    );
    let compact = serde_json::to_vec(&[policy]).unwrap();
    assert!(compact.len() as u64 <= netlane_core::persistence::MAX_JSON_BYTES);
    std::fs::write(&path, &compact).unwrap();
    let backup = path.with_extension("json.bak");
    std::fs::write(&backup, b"previous backup").unwrap();
    let file = PolicyFile::new(&path);
    let loaded = file.load().unwrap();
    assert!(
        serde_json::to_vec_pretty(&loaded.policies).unwrap().len() as u64
            > netlane_core::persistence::MAX_JSON_BYTES
    );
    assert!(
        file.save(&loaded.policies, loaded.revision.as_deref())
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), compact);
    assert_eq!(file.load().unwrap().revision, loaded.revision);
    assert_eq!(std::fs::read(backup).unwrap(), b"previous backup");
}

#[test]
fn explicit_interface_never_falls_back() {
    let adapters = vec![Adapter {
        id: "aa6f3b88-072a-45b6-b0cb-8afe9567e62f".into(),
        name: "Wi-Fi A".into(),
        kind: "Wireless80211".into(),
        connected: true,
        luid: 4,
        ipv4: true,
        ..Default::default()
    }];
    let mut policy = RoutingPolicy::new("C:\\x.exe");
    policy.route_mode = RouteMode::WiFi;
    policy.interface_id = Some("d3ae43d2-8203-41d6-9d7f-0b6fa59518fd".into());
    assert!(resolve_interface(&policy, &adapters).is_none());
    assert_eq!(
        plan(&[policy.clone()], &adapters)[0].mode,
        RouteMode::Automatic
    );
    policy.interface_id = None;
    assert_eq!(resolve_interface(&policy, &adapters).unwrap().luid, 4);
}
#[test]
fn explicit_disabled_companion_wins_and_arbitrary_children_are_not_inherited() {
    let mut steam = RoutingPolicy::new("C:\\Steam\\steam.exe");
    steam.route_mode = RouteMode::WiFi;
    let mut explicit = RoutingPolicy::new("C:\\Steam\\bin\\steamwebhelper.exe");
    explicit.enabled = false;
    let expanded = expand_related(&[steam, explicit], |_| true);
    assert_eq!(expanded.len(), 5);
    assert!(!expanded[1].enabled);
    assert!(!expanded.iter().any(|p| p.application_id == "game.exe"));
    assert_eq!(
        expanded
            .iter()
            .filter(|p| p.application_id == "steamwebhelper.exe")
            .count(),
        1
    );
}
#[test]
fn connections_show_ipv6_udp_and_read_snapshots_without_protocol() {
    let old: netlane_core::Connection = serde_json::from_str(
        r#"{"pid":1,"name":"a.exe","executable":null,"local_address":"192.0.2.1","local_port":5000,"remote_address":"203.0.113.1","remote_port":443,"interface":null}"#,
    )
    .unwrap();
    assert_eq!(old.protocol, "");
    assert_eq!(old.remote_endpoint().as_deref(), Some("203.0.113.1:443"));
    let udp = netlane_core::Connection {
        protocol: "UDP".into(),
        local_address: "2001:db8::1".into(),
        local_port: 27015,
        ..Default::default()
    };
    assert_eq!(udp.local_endpoint(), "[2001:db8::1]:27015");
    assert_eq!(udp.remote_endpoint(), None);
}
#[test]
fn missing_counter_reset_and_long_gap_do_not_invent_traffic() {
    let mut tracker = TrafficTracker::default();
    let mut adapter = Adapter {
        id: "x".into(),
        received: Some(1000),
        sent: Some(2000),
        ..Default::default()
    };
    tracker.sample(Duration::ZERO, &[adapter.clone()]);
    adapter.received = Some(1_001_000);
    tracker.sample(Duration::from_secs(2), &[adapter.clone()]);
    assert_eq!(tracker.usage["x"].download_mbps, Some(4.));
    assert_eq!(tracker.usage["x"].received, 1_000_000);
    adapter.received = None;
    tracker.sample(Duration::from_secs(4), &[adapter.clone()]);
    adapter.received = Some(9_001_000);
    tracker.sample(Duration::from_secs(6), &[adapter.clone()]);
    assert_eq!(tracker.usage["x"].download_mbps, None);
    adapter.received = Some(10);
    tracker.sample(Duration::from_secs(8), &[adapter.clone()]);
    assert_eq!(tracker.usage["x"].download_mbps, None);
    adapter.received = Some(900);
    tracker.sample(Duration::from_secs(100), &[adapter]);
    assert_eq!(tracker.usage["x"].download_mbps, None);
    assert_eq!(tracker.usage["x"].received, 1_000_000);
}
#[derive(Default)]
struct Trace {
    calls: Vec<&'static str>,
    fail_add: bool,
    fail_abort: bool,
}
struct Fake(Rc<RefCell<Trace>>);
impl PolicySession for Fake {
    type Key = u64;
    fn begin(&mut self) -> Result<()> {
        self.0.borrow_mut().calls.push("begin");
        Ok(())
    }
    fn add(&mut self, _: &netlane_core::planning::PlannedRule) -> Result<Vec<u64>> {
        let mut t = self.0.borrow_mut();
        t.calls.push("add");
        if t.fail_add {
            anyhow::bail!("add failed");
        }
        Ok(vec![1])
    }
    fn delete(&mut self, _: &u64) -> Result<()> {
        self.0.borrow_mut().calls.push("delete");
        Ok(())
    }
    fn commit(&mut self) -> Result<()> {
        self.0.borrow_mut().calls.push("commit");
        Ok(())
    }
    fn abort(&mut self) -> Result<()> {
        let mut t = self.0.borrow_mut();
        t.calls.push("abort");
        if t.fail_abort {
            anyhow::bail!("abort failed");
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.0.borrow_mut().calls.push("close");
        Ok(())
    }
}
#[test]
fn transaction_noop_and_removal() {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut engine = RoutingEngine::new(Fake(trace.clone()));
    let mut policy = RoutingPolicy::new("C:\\x.exe");
    policy.route_mode = RouteMode::Blocked;
    let rules = plan(&[policy], &[]);
    engine.apply(&rules).unwrap();
    engine.apply(&rules).unwrap();
    assert_eq!(trace.borrow().calls, vec!["begin", "add", "commit"]);
    engine.apply(&[]).unwrap();
    assert_eq!(
        trace.borrow().calls,
        vec!["begin", "add", "commit", "begin", "delete", "commit"]
    );
}
#[test]
fn failed_abort_closes_session_and_prevents_reuse() {
    let trace = Rc::new(RefCell::new(Trace {
        fail_add: true,
        fail_abort: true,
        ..Default::default()
    }));
    let mut engine = RoutingEngine::new(Fake(trace.clone()));
    let mut policy = RoutingPolicy::new("C:\\x.exe");
    policy.route_mode = RouteMode::Blocked;
    assert!(engine.apply(&plan(&[policy], &[])).is_err());
    assert_eq!(trace.borrow().calls, vec!["begin", "add", "abort", "close"]);
    assert!(engine.apply(&[]).is_err());
}
