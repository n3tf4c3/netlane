//! Notifications derived from two consecutive plans of a running session: a rule moved
//! to its fallback, lost its interface (Windows decides) or returned to the chosen
//! interface. Several changes in one cycle become a single alert.
use crate::{
    RouteMode,
    planning::{PlanState, PlannedRule},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub title: String,
    pub body: String,
    /// Shown with the warning icon: some app no longer uses its chosen interface.
    pub warning: bool,
}
/// Lines listed before "e mais N".
const MAX_LINES: usize = 4;

/// `watched` limits alerts to the user's own rules (not Steam helpers added by the
/// plan); `interface_name` turns an adapter id into its display name.
pub fn rule_alert(
    previous: &[PlannedRule],
    current: &[PlannedRule],
    watched: impl Fn(&str) -> bool,
    interface_name: impl Fn(&str) -> Option<String>,
) -> Option<Alert> {
    let target = |rule: &PlannedRule| {
        rule.interface_id
            .as_deref()
            .and_then(&interface_name)
            .unwrap_or_else(|| rule.mode.label().to_owned())
    };
    let mut lost = Vec::new();
    let mut restored = Vec::new();
    for rule in current.iter().filter(|r| watched(&r.application)) {
        let Some(before) = previous
            .iter()
            .find(|p| p.application.eq_ignore_ascii_case(&rule.application))
        else {
            continue;
        };
        match (before.state, rule.state) {
            (PlanState::Applied | PlanState::Unavailable, PlanState::Fallback) => {
                lost.push(if rule.mode == RouteMode::Blocked {
                    format!("{}: bloqueado até a interface voltar.", rule.application)
                } else {
                    format!("{}: usando a reserva {}.", rule.application, target(rule))
                });
            }
            (PlanState::Applied | PlanState::Fallback, PlanState::Unavailable) => {
                lost.push(format!(
                    "{}: sem a interface escolhida; o Windows decide a saída.",
                    rule.application
                ));
            }
            (PlanState::Fallback | PlanState::Unavailable, PlanState::Applied) => {
                restored.push(format!(
                    "{}: de volta a {}.",
                    rule.application,
                    target(rule)
                ));
            }
            _ => {}
        }
    }
    if lost.is_empty() && restored.is_empty() {
        return None;
    }
    let warning = !lost.is_empty();
    let title = if !warning {
        "Interface escolhida de volta"
    } else if restored.is_empty() {
        "Interface escolhida indisponível"
    } else {
        "Mudança nas interfaces das regras"
    };
    let lines: Vec<String> = lost.into_iter().chain(restored).collect();
    let mut body = lines
        .iter()
        .take(MAX_LINES)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > MAX_LINES {
        let rest = lines.len() - MAX_LINES;
        body.push_str(&format!(
            "\ne mais {rest} {}.",
            if rest == 1 { "regra" } else { "regras" }
        ));
    }
    Some(Alert {
        title: title.to_owned(),
        body,
        warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Adapter, RoutingPolicy,
        planning::{expand_related, plan},
    };

    fn adapters(ethernet: bool, wifi: bool) -> Vec<Adapter> {
        vec![
            Adapter {
                id: "d3ae43d2-8203-41d6-9d7f-0b6fa59518fd".into(),
                name: "Ethernet".into(),
                kind: "Ethernet".into(),
                connected: ethernet,
                luid: 7,
                ipv4: true,
                ..Default::default()
            },
            Adapter {
                id: "aa6f3b88-072a-45b6-b0cb-8afe9567e62f".into(),
                name: "Wi-Fi Casa".into(),
                kind: "Wireless80211".into(),
                connected: wifi,
                luid: 9,
                ipv4: true,
                ..Default::default()
            },
        ]
    }
    fn policies() -> Vec<RoutingPolicy> {
        let mut steam = RoutingPolicy::new("C:\\Steam\\steam.exe");
        steam.route_mode = RouteMode::Ethernet;
        steam.fallback_route_mode = RouteMode::WiFi;
        let mut game = RoutingPolicy::new("C:\\Games\\game.exe");
        game.route_mode = RouteMode::Ethernet;
        game.fallback_route_mode = RouteMode::Blocked;
        let mut chat = RoutingPolicy::new("C:\\Chat\\chat.exe");
        chat.route_mode = RouteMode::Ethernet;
        vec![steam, game, chat]
    }
    fn cycle(ethernet: bool, wifi: bool) -> (Vec<PlannedRule>, Vec<Adapter>) {
        let adapters = adapters(ethernet, wifi);
        let expanded = expand_related(&policies(), |_| true);
        (plan(&expanded, &adapters), adapters)
    }
    fn alert(
        previous: &[PlannedRule],
        current: &[PlannedRule],
        adapters: &[Adapter],
    ) -> Option<Alert> {
        let names: Vec<String> = policies().into_iter().map(|p| p.application_id).collect();
        rule_alert(
            previous,
            current,
            |app| names.iter().any(|n| n.eq_ignore_ascii_case(app)),
            |id| {
                adapters
                    .iter()
                    .find(|a| crate::same_interface(&a.id, id))
                    .map(|a| a.name.clone())
            },
        )
    }

    #[test]
    fn interface_loss_and_return_produce_one_alert_each_for_the_users_rules() {
        let (up, _) = cycle(true, true);
        let (down, adapters) = cycle(false, true);
        let lost = alert(&up, &down, &adapters).unwrap();
        assert!(lost.warning);
        assert_eq!(lost.title, "Interface escolhida indisponível");
        assert_eq!(
            lost.body,
            "steam.exe: usando a reserva Wi-Fi Casa.\n\
             game.exe: bloqueado até a interface voltar.\n\
             chat.exe: sem a interface escolhida; o Windows decide a saída."
        );
        assert!(
            !lost.body.contains("steamwebhelper"),
            "Auxiliares não notificam."
        );

        // Nothing changed: no repeated alert every cycle.
        assert_eq!(alert(&down, &down, &adapters), None);

        let (back, adapters) = cycle(true, true);
        let returned = alert(&down, &back, &adapters).unwrap();
        assert!(!returned.warning);
        assert_eq!(returned.title, "Interface escolhida de volta");
        assert!(returned.body.starts_with("steam.exe: de volta a Ethernet."));
        assert_eq!(returned.body.lines().count(), 3);
    }

    #[test]
    fn fallback_lost_too_is_reported_and_new_or_inactive_rules_are_silent() {
        let (only_ethernet_lost, _) = cycle(false, true);
        let (nothing, adapters) = cycle(false, false);
        let both = alert(&only_ethernet_lost, &nothing, &adapters).unwrap();
        assert_eq!(
            both.body,
            "steam.exe: sem a interface escolhida; o Windows decide a saída."
        );
        // Rules that first appear in this plan (session start, new rule) are silent.
        assert_eq!(alert(&[], &nothing, &adapters), None);
        // Plans from an agent without the state field read as Inactive: silent.
        let mut old = only_ethernet_lost.clone();
        old.iter_mut().for_each(|r| r.state = PlanState::Inactive);
        assert_eq!(alert(&old, &nothing, &adapters), None);
    }

    #[test]
    fn long_lists_are_summarized() {
        let rules = |state| -> Vec<PlannedRule> {
            (0..6)
                .map(|i| PlannedRule {
                    application: format!("app{i}.exe"),
                    executable: None,
                    mode: RouteMode::Automatic,
                    interface_id: None,
                    luid: 0,
                    ipv4: true,
                    ipv6: true,
                    detail: String::new(),
                    state,
                })
                .collect()
        };
        let alert = rule_alert(
            &rules(PlanState::Applied),
            &rules(PlanState::Unavailable),
            |_| true,
            |_| None,
        )
        .unwrap();
        assert_eq!(alert.body.lines().count(), MAX_LINES + 1);
        assert!(alert.body.ends_with("e mais 2 regras."));
    }
}
