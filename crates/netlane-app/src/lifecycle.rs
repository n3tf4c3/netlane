//! Shared decision for initial Exit and the recheck after asynchronous cleanup.
use netlane_agent::SessionState;
#[derive(Debug, PartialEq, Eq)]
pub enum ExitDecision {
    Busy,
    ConfirmEdits,
    StopAndWait,
    CleanupUnconfirmed,
    Quit,
}
pub fn exit_decision(dirty: bool, state: &SessionState) -> ExitDecision {
    if state.busy {
        ExitDecision::Busy
    } else if dirty {
        ExitDecision::ConfirmEdits
    } else if state.running {
        ExitDecision::StopAndWait
    } else if !state.cleanup_confirmed {
        ExitDecision::CleanupUnconfirmed
    } else {
        ExitDecision::Quit
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exit_rechecks_new_edits_and_requires_cleanup_even_on_retry() {
        let mut state = SessionState {
            running: true,
            cleanup_confirmed: false,
            ..Default::default()
        };
        assert_eq!(exit_decision(true, &state), ExitDecision::ConfirmEdits);
        assert_eq!(exit_decision(false, &state), ExitDecision::StopAndWait);
        state.busy = true;
        assert_eq!(exit_decision(false, &state), ExitDecision::Busy);
        state.running = false;
        state.busy = false;
        assert_eq!(
            exit_decision(false, &state),
            ExitDecision::CleanupUnconfirmed
        );
        assert_eq!(
            exit_decision(false, &state),
            ExitDecision::CleanupUnconfirmed
        );
        state.cleanup_confirmed = true;
        assert_eq!(exit_decision(true, &state), ExitDecision::ConfirmEdits);
        assert_eq!(exit_decision(false, &state), ExitDecision::Quit);
    }
}
