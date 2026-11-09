//! When to compare session plans for tray notifications. The plan received after the
//! user saves rules or switches profiles reflects that edit, not an interface change,
//! so it only becomes the new baseline.
use netlane_core::planning::PlannedRule;
use std::time::{Duration, Instant};

/// Longer than one session cycle (10 s), so the plan built from the edited rules
/// arrives inside the window.
const QUIET_AFTER_EDIT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct Alerts {
    baseline: Option<Vec<PlannedRule>>,
    quiet_until: Option<Instant>,
}
impl Alerts {
    /// Called when rules edited by the user are sent to the session.
    pub fn hush(&mut self) {
        self.hush_at(Instant::now());
    }
    fn hush_at(&mut self, now: Instant) {
        self.quiet_until = Some(now + QUIET_AFTER_EDIT);
    }
    /// The previous plan to compare against when `current` is a new plan of a running
    /// session outside the quiet window. The first plan of a session is silent.
    pub fn advance(
        &mut self,
        running: bool,
        current: &[PlannedRule],
        now: Instant,
    ) -> Option<Vec<PlannedRule>> {
        if !running {
            self.baseline = None;
            return None;
        }
        if self.baseline.as_deref() == Some(current) {
            return None;
        }
        let previous = self.baseline.replace(current.to_vec());
        if self.quiet_until.is_some_and(|until| now < until) {
            return None;
        }
        previous
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use netlane_core::{RouteMode, planning::PlanState};

    fn plan(state: PlanState) -> Vec<PlannedRule> {
        vec![PlannedRule {
            application: "a.exe".into(),
            executable: None,
            mode: RouteMode::Automatic,
            interface_id: None,
            luid: 0,
            ipv4: true,
            ipv6: true,
            detail: String::new(),
            state,
        }]
    }

    #[test]
    fn first_plan_edits_and_stopped_sessions_are_silent_but_later_changes_compare() {
        let start = Instant::now();
        let mut alerts = Alerts::default();
        let applied = plan(PlanState::Applied);
        let fallback = plan(PlanState::Fallback);
        assert_eq!(
            alerts.advance(true, &applied, start),
            None,
            "Início da sessão."
        );
        assert_eq!(alerts.advance(true, &applied, start), None, "Sem mudança.");
        assert_eq!(
            alerts.advance(true, &fallback, start),
            Some(applied.clone()),
            "Mudança de interface."
        );

        // A user edit: the next plan becomes the baseline without comparison.
        alerts.hush_at(start);
        let later = start + Duration::from_secs(5);
        assert_eq!(alerts.advance(true, &applied, later), None);
        let after = start + QUIET_AFTER_EDIT + Duration::from_secs(1);
        assert_eq!(
            alerts.advance(true, &fallback, after),
            Some(applied.clone())
        );

        // Stopping clears the baseline: the next session starts silent.
        assert_eq!(alerts.advance(false, &[], after), None);
        assert_eq!(alerts.advance(true, &applied, after), None);
    }
}
