use anyhow::Result;
use netlane_core::default_connection::*;
use uuid::Uuid;

fn initial() -> Snapshot {
    let interfaces: Vec<_> = [
        (1, "Ethernet", Kind::Cable, 25),
        (2, "Wi-Fi", Kind::Wifi, 35),
    ]
    .into_iter()
    .map(|(id, name, kind, metric)| Interface {
        id: Uuid::from_u128(id),
        name: name.into(),
        kind,
        index: id as u32,
        connected: true,
        automatic_metric: true,
        metric,
        default_route_metric: Some(0),
        saved_metric: None,
    })
    .collect();
    let routes = interfaces
        .iter()
        .map(|i| Route {
            index: i.index,
            name: i.name.clone(),
            interface_metric: i.metric,
            route_metric: 0,
        })
        .collect();
    Snapshot { interfaces, routes }
}
fn request(s: &Snapshot, id: u128) -> Request {
    Request {
        expected: s.clone(),
        changes: plan(s, Uuid::from_u128(id), &[]).unwrap(),
        preferred_interface: Some(Uuid::from_u128(id)),
    }
}
struct Fake {
    snapshot: Snapshot,
    writes: Vec<Metric>,
    fail_at: Option<usize>,
    partial: bool,
    external: bool,
    fail_cleanup: bool,
    competing_route: bool,
}
impl Fake {
    fn new() -> Self {
        Self {
            snapshot: initial(),
            writes: vec![],
            fail_at: None,
            partial: false,
            external: false,
            fail_cleanup: false,
            competing_route: false,
        }
    }
}
impl Settings for Fake {
    fn read(&mut self) -> Result<Snapshot> {
        Ok(self.snapshot.clone())
    }
    fn write(&mut self, m: &Metric) -> Result<()> {
        self.writes.push(m.clone());
        if self.fail_cleanup && m.automatic_metric {
            anyhow::bail!("cleanup failed");
        }
        let failed = self.fail_at == Some(self.writes.len());
        let i = self
            .snapshot
            .interfaces
            .iter_mut()
            .find(|i| i.id == m.id)
            .unwrap();
        i.automatic_metric = m.automatic_metric;
        i.metric = if failed && self.external {
            77
        } else if m.automatic_metric {
            if i.kind == Kind::Cable { 25 } else { 35 }
        } else {
            m.metric
        };
        if !(failed && self.partial) {
            i.saved_metric = (!m.automatic_metric).then_some(i.metric);
        }
        for r in self
            .snapshot
            .routes
            .iter_mut()
            .filter(|r| r.index == i.index)
        {
            r.interface_metric = i.metric;
        }
        if self.competing_route && self.writes.len() == 2 {
            self.snapshot.routes.push(Route {
                index: 90,
                name: "VPN".into(),
                interface_metric: 1,
                route_metric: 0,
            });
        }
        if failed {
            anyhow::bail!("failure after write");
        }
        Ok(())
    }
}
#[test]
fn wifi_then_cable_then_restoration_preserves_first_original() {
    let mut settings = Fake::new();
    let (first, request) = Journal::prepare(&settings.snapshot, Uuid::from_u128(2), None).unwrap();
    let receipt = apply(&mut settings, &request, || true).unwrap();
    assert!(receipt.result.success);
    assert_eq!(receipt.result.snapshot.current_label(), "Atual: Wi-Fi");
    assert_eq!(settings.writes[0].id, Uuid::from_u128(1));
    let first = first.confirmed(&receipt.result.snapshot).unwrap();
    let (second, request) =
        Journal::prepare(&settings.snapshot, Uuid::from_u128(1), Some(&first)).unwrap();
    assert!(
        apply(&mut settings, &request, || true)
            .unwrap()
            .result
            .success
    );
    let second = second.confirmed(&settings.snapshot).unwrap();
    assert_eq!(first.original, second.original);
    assert!(second.original.iter().all(|m| m.automatic_metric));
    let request = second.restoration(&settings.snapshot).unwrap().unwrap();
    assert!(
        apply(&mut settings, &request, || true)
            .unwrap()
            .result
            .success
    );
    assert_eq!(settings.snapshot.current_label(), "Atual: Ethernet");
    assert!(
        settings
            .snapshot
            .interfaces
            .iter()
            .all(|i| i.automatic_metric)
    );
    assert!(second.restoration(&settings.snapshot).unwrap().is_none());
}
#[test]
fn partial_writes_revert_in_reverse_order_including_split_stores() {
    for fail_at in [1, 2] {
        for partial in [false, true] {
            let mut settings = Fake::new();
            settings.fail_at = Some(fail_at);
            settings.partial = partial;
            let receipt = apply(&mut settings, &request(&initial(), 2), || true).unwrap();
            assert!(
                !receipt.result.success && receipt.result.rollback_confirmed,
                "{}",
                receipt.result.detail
            );
            assert_eq!(settings.snapshot, initial());
            assert_eq!(settings.writes[fail_at].id, settings.writes[fail_at - 1].id);
        }
    }
}
#[test]
fn external_edit_during_failure_is_not_overwritten_and_cleanup_failure_is_explicit() {
    for external in [true, false] {
        let mut settings = Fake::new();
        settings.fail_at = Some(2);
        settings.external = external;
        settings.fail_cleanup = !external;
        let receipt = apply(&mut settings, &request(&initial(), 2), || true).unwrap();
        assert!(!receipt.result.success && !receipt.result.rollback_confirmed);
        assert!(receipt.result.detail.contains("Restauração incompleta"));
        if external {
            assert_eq!(settings.snapshot.interfaces[1].metric, 77);
            assert!(
                !settings
                    .writes
                    .iter()
                    .any(|m| m.id == Uuid::from_u128(2) && m.automatic_metric)
            );
        }
    }
}
#[test]
fn stale_metrics_routes_identity_and_tampered_plan_refuse_before_writes() {
    for kind in 0..4 {
        let mut settings = Fake::new();
        let mut r = request(&initial(), 2);
        match kind {
            0 => {
                settings.snapshot.interfaces[0].automatic_metric = false;
                settings.snapshot.interfaces[0].metric = 77;
                settings.snapshot.interfaces[0].saved_metric = Some(77);
                settings.snapshot.routes[0].interface_metric = 77;
            }
            1 => settings.snapshot.routes.push(Route {
                index: 90,
                name: "VPN".into(),
                interface_metric: 1,
                route_metric: 0,
            }),
            2 => {
                settings.snapshot.interfaces[0].index = 50;
                settings.snapshot.routes[0].index = 50;
            }
            _ => r.changes[0].metric = 1,
        }
        assert!(apply(&mut settings, &r, || true).is_err());
        assert!(settings.writes.is_empty());
    }
}
#[test]
fn disconnected_saved_override_competing_vpn_overflow_and_invalid_lists_refuse() {
    let mut disconnected = initial();
    disconnected.interfaces[1].connected = false;
    disconnected.interfaces[1].default_route_metric = None;
    disconnected.routes.pop();
    assert!(plan(&disconnected, Uuid::from_u128(2), &[]).is_err());
    let mut override_ = initial();
    override_.interfaces[1].automatic_metric = false;
    assert!(plan(&override_, Uuid::from_u128(2), &[]).is_err());
    let mut vpn = initial();
    vpn.routes.push(Route {
        index: 90,
        name: "VPN".into(),
        interface_metric: 1,
        route_metric: 0,
    });
    assert!(plan(&vpn, Uuid::from_u128(2), &[]).is_err());
    let mut overflow = initial();
    overflow.interfaces[1].default_route_metric = Some(u32::MAX);
    overflow.routes[1].route_metric = u32::MAX;
    assert!(plan(&overflow, Uuid::from_u128(2), &[]).is_err());
    let mut invalid = initial();
    invalid.interfaces[1].id = invalid.interfaces[0].id;
    assert!(invalid.validate().is_err());
    let mut r = request(&initial(), 2);
    r.changes.push(r.changes[0].clone());
    assert!(r.validate_against(&initial()).is_err());
}
#[test]
fn route_metric_and_multiple_gateways_participate_and_ties_are_reported() {
    let mut s = initial();
    s.interfaces[1].default_route_metric = Some(100);
    s.routes[1].route_metric = 100;
    let changes = plan(&s, Uuid::from_u128(2), &[]).unwrap();
    assert_eq!(changes[0].metric, 155);
    s.routes.push(Route {
        index: 2,
        name: "Wi-Fi".into(),
        interface_metric: 35,
        route_metric: 200,
    });
    assert!(s.validate().is_ok());
    s.routes[1].interface_metric = 25;
    s.interfaces[1].metric = 25;
    s.routes[1].route_metric = 0;
    s.interfaces[1].default_route_metric = Some(0);
    s.routes[2].interface_metric = 25;
    assert_eq!(s.current_label(), "Prioridades iguais — Windows decide");
}
#[test]
fn cancellation_before_or_between_writes_and_post_write_vpn_revert() {
    for stop_at in [0, 2] {
        let mut settings = Fake::new();
        let calls = std::cell::Cell::new(0);
        let receipt = apply(&mut settings, &request(&initial(), 2), || {
            let n = calls.get();
            calls.set(n + 1);
            n < stop_at
        })
        .unwrap();
        assert!(!receipt.result.success && receipt.result.rollback_confirmed);
        assert_eq!(settings.snapshot, initial());
    }
    let mut settings = Fake::new();
    settings.competing_route = true;
    let receipt = apply(&mut settings, &request(&initial(), 2), || true).unwrap();
    assert!(!receipt.result.success && receipt.result.rollback_confirmed);
    assert!(
        settings
            .snapshot
            .interfaces
            .iter()
            .all(|i| i.automatic_metric)
    );
    assert_eq!(settings.snapshot.routes.last().unwrap().name, "VPN");
}
#[test]
fn journal_covers_cancelled_authorization_missing_interfaces_and_external_edits() {
    let (pending, _) = Journal::prepare(&initial(), Uuid::from_u128(2), None).unwrap();
    assert!(pending.restoration(&initial()).unwrap().is_none());
    let mut settings = Fake::new();
    apply(&mut settings, &request(&initial(), 2), || true).unwrap();
    settings.snapshot.interfaces[1].metric = 77;
    settings.snapshot.interfaces[1].saved_metric = Some(77);
    settings.snapshot.routes[1].interface_metric = 77;
    assert!(pending.restoration(&settings.snapshot).is_err());
    assert!(Journal::prepare(&settings.snapshot, Uuid::from_u128(1), Some(&pending)).is_err());
    let mut missing = initial();
    missing.interfaces.pop();
    missing.routes.pop();
    assert!(pending.restoration(&missing).is_err());
}
#[test]
fn dotnet_journal_reopens_after_restart_and_revision_protects_external_file() {
    let directory = tempfile::tempdir().unwrap();
    let file = JournalFile::new(directory.path().join("default-connection.json"));
    assert!(file.load().unwrap().0.is_none());
    assert!(!file.path.exists());
    let (mut pending, request) = Journal::prepare(&initial(), Uuid::from_u128(2), None).unwrap();
    pending
        .extra
        .insert("FutureField".into(), serde_json::json!({"kept": true}));
    let rev = file.save(&pending, None).unwrap();
    let json = std::fs::read_to_string(&file.path).unwrap();
    assert!(json.contains("\"Original\"") && json.contains("\"AutomaticMetric\""));
    let mut settings = Fake::new();
    let receipt = apply(&mut settings, &request, || true).unwrap();
    let confirmed = pending.confirmed(&receipt.result.snapshot).unwrap();
    let new_rev = file.save(&confirmed, Some(&rev)).unwrap();
    assert!(file.path.with_extension("json.bak").exists());
    let reopened = JournalFile::new(&file.path);
    let (journal, revision) = reopened.load().unwrap();
    let journal = journal.unwrap();
    assert_eq!(
        journal.extra["FutureField"],
        serde_json::json!({"kept":true})
    );
    assert_eq!(revision, Some(new_rev.clone()));
    assert!(journal.previous.is_empty());
    let restore = journal.restoration(&settings.snapshot).unwrap().unwrap();
    assert!(
        apply(&mut settings, &restore, || true)
            .unwrap()
            .result
            .success
    );
    assert!(file.save(&pending, Some(&rev)).is_err());
    assert!(file.clear(Some(&rev)).is_err());
    assert_eq!(reopened.load().unwrap().1, Some(new_rev.clone()));
    reopened.clear(Some(&new_rev)).unwrap();
    assert!(!file.path.exists());
}
#[test]
fn corrupt_incomplete_oversized_journals_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let file = JournalFile::new(directory.path().join("default-connection.json"));
    let (pending, _) = Journal::prepare(&initial(), Uuid::from_u128(2), None).unwrap();
    for content in [
        "{broken".to_owned(),
        "{\"Original\":[],\"Applied\":[],\"Previous\":[]}".into(),
        " ".repeat(65_537),
    ] {
        std::fs::write(&file.path, &content).unwrap();
        assert!(file.load().is_err());
        assert!(file.save(&pending, None).is_err());
        assert!(file.clear(None).is_err());
        assert_eq!(std::fs::read_to_string(&file.path).unwrap(), content);
    }
}
