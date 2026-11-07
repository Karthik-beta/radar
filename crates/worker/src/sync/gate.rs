//! The rate-limit decision for one kind of unit (P4).

use domain::ids::Timestamp;
use domain::sync_status::SyncKind;

use super::config::GATE_PERCENT;
use super::state::SyncState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Open,
    WaitUntil(Timestamp),
}

/// A rate-limit backoff holds every GitHub kind. A low budget holds only the
/// kinds that spend it, because a poll that gets a 304 costs nothing.
pub fn gate(kind: SyncKind, state: &SyncState, now: Timestamp) -> Gate {
    if !kind.calls_github() {
        return Gate::Open;
    }
    if let Some(retry_at) = state.throttle.retry_at.filter(|&at| at > now) {
        return Gate::WaitUntil(retry_at);
    }
    let spends_budget = matches!(kind, SyncKind::Enrich | SyncKind::BackfillPage);
    let rate = state.rate;
    match (rate.limit, rate.remaining, rate.reset) {
        (Some(limit), Some(remaining), Some(reset))
            if spends_budget
                && reset > now
                && u64::from(remaining) * 100 < u64::from(limit) * u64::from(GATE_PERCENT) =>
        {
            Gate::WaitUntil(reset)
        }
        _ => Gate::Open,
    }
}

#[cfg(test)]
mod tests {
    use domain::sync_status::Rate;

    use super::*;

    const NOW: Timestamp = Timestamp(1_000_000);
    const RESET: Timestamp = Timestamp(1_000_020);

    fn with_remaining(remaining: u32) -> SyncState {
        SyncState {
            rate: Rate {
                limit: Some(5000),
                remaining: Some(remaining),
                reset: Some(RESET),
            },
            ..SyncState::default()
        }
    }

    #[test]
    fn below_20_percent_closes_enrich_but_not_poll() {
        let state = with_remaining(400);
        assert_eq!(gate(SyncKind::Enrich, &state, NOW), Gate::WaitUntil(RESET));
        assert_eq!(
            gate(SyncKind::BackfillPage, &state, NOW),
            Gate::WaitUntil(RESET)
        );
        assert_eq!(gate(SyncKind::PollNotifications, &state, NOW), Gate::Open);
        assert_eq!(gate(SyncKind::Rollup, &state, NOW), Gate::Open);
    }

    #[test]
    fn exactly_20_percent_is_open() {
        assert_eq!(
            gate(SyncKind::Enrich, &with_remaining(1000), NOW),
            Gate::Open
        );
        assert_eq!(
            gate(SyncKind::Enrich, &with_remaining(999), NOW),
            Gate::WaitUntil(RESET)
        );
    }

    #[test]
    fn reopens_at_reset() {
        let state = with_remaining(0);
        assert_eq!(gate(SyncKind::Enrich, &state, RESET), Gate::Open);
        assert_eq!(
            gate(SyncKind::Enrich, &state, Timestamp(RESET.0 - 1)),
            Gate::WaitUntil(RESET)
        );
    }

    #[test]
    fn a_backoff_holds_every_github_kind_but_not_rollup() {
        let mut state = SyncState::default();
        state.throttle.retry_at = Some(RESET);
        for kind in [
            SyncKind::PollNotifications,
            SyncKind::Enrich,
            SyncKind::BackfillPage,
        ] {
            assert_eq!(gate(kind, &state, NOW), Gate::WaitUntil(RESET));
            assert_eq!(gate(kind, &state, RESET), Gate::Open);
        }
        assert_eq!(gate(SyncKind::Rollup, &state, NOW), Gate::Open);
    }

    #[test]
    fn an_unknown_budget_is_open() {
        assert_eq!(
            gate(SyncKind::Enrich, &SyncState::default(), NOW),
            Gate::Open
        );
    }
}
