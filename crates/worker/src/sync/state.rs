//! The single `sync_state` row in the Syncer's SQLite.

use std::collections::BTreeMap;

use domain::ids::Timestamp;
use domain::sync_status::{DeferCause, Rate, SyncKind, SyncStatus};

use super::gate::{gate, Gate};
use super::poll::PollPhase;
use super::work::WorkRow;
use crate::github::RateMeta;

/// How the Syncer holds back GitHub work after a rate limit or without a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Throttle {
    /// Rate-limit responses in a row. The next success resets it to 0.
    pub strikes: u32,
    /// No GitHub unit runs before this time.
    pub retry_at: Option<Timestamp>,
    /// The last GitHub unit found no token. Only a kick clears it.
    pub no_token: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncState {
    pub last_modified: Option<String>,
    pub next_poll_at: Option<Timestamp>,
    pub rate: Rate,
    pub scopes: Option<String>,
    pub throttle: Throttle,
    pub last_kick_at: Option<Timestamp>,
    pub last_ok: BTreeMap<SyncKind, Timestamp>,
    /// Kept in the `kv` table, because it is not a `sync_state` column.
    pub poll_phase: PollPhase,
    /// When the first sign-in backfill finished all three searches. Set once, and
    /// kept in the `kv` table. A sign-in starts a backfill only while it is unset.
    pub backfill_done_at: Option<Timestamp>,
}

impl SyncState {
    /// Takes the fields a unit learns from GitHub and leaves the rest, which kicks own.
    pub fn observe(&mut self, unit: &SyncState) {
        self.last_modified.clone_from(&unit.last_modified);
        self.poll_phase.clone_from(&unit.poll_phase);
        self.backfill_done_at = unit.backfill_done_at.or(self.backfill_done_at);
        self.next_poll_at = unit.next_poll_at;
        self.rate = unit.rate;
        self.scopes.clone_from(&unit.scopes);
    }

    /// Keeps what GitHub reported and leaves a value it did not send unchanged.
    pub fn record_rate(&mut self, meta: &RateMeta) {
        if meta.limit.is_some() {
            self.rate.limit = meta.limit;
        }
        if meta.remaining.is_some() {
            self.rate.remaining = meta.remaining;
        }
        if let Some(reset) = meta.reset.and_then(|r| i64::try_from(r.0).ok()) {
            self.rate.reset = Some(Timestamp(reset));
        }
        if let Some(scopes) = &meta.scopes {
            self.scopes = Some(scopes.join(","));
        }
    }
}

/// What GitHub work is held back by, if anything.
pub fn blocked(state: &SyncState, now: Timestamp) -> Option<DeferCause> {
    if state.throttle.no_token {
        return Some(DeferCause::NoToken);
    }
    match gate(SyncKind::Enrich, state, now) {
        Gate::WaitUntil(_) => Some(DeferCause::RateLimited),
        Gate::Open => None,
    }
}

pub fn status(
    rows: &[WorkRow],
    state: &SyncState,
    next_alarm: Option<Timestamp>,
    now: Timestamp,
) -> SyncStatus {
    let mut out = SyncStatus {
        next_alarm,
        next_poll_at: state.next_poll_at,
        rate: state.rate,
        last_ok_by_kind: state.last_ok.clone(),
        blocked: blocked(state, now),
        ..SyncStatus::default()
    };
    for row in rows {
        let counts = if row.is_dead() {
            &mut out.dead_by_kind
        } else {
            &mut out.pending_by_kind
        };
        *counts.entry(row.kind.kind()).or_default() += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::Timestamp as GithubTimestamp;
    use crate::sync::work::{RowState, WorkKind};

    #[test]
    fn record_rate_keeps_values_github_did_not_send() {
        let mut state = SyncState::default();
        state.record_rate(&RateMeta {
            limit: Some(5000),
            remaining: Some(4990),
            reset: Some(GithubTimestamp(1_793_900_000)),
            poll_interval: None,
            scopes: Some(vec!["notifications".into(), "read:user".into()]),
        });
        state.record_rate(&RateMeta {
            remaining: Some(4989),
            ..RateMeta::default()
        });
        assert_eq!(
            state.rate,
            Rate {
                limit: Some(5000),
                remaining: Some(4989),
                reset: Some(Timestamp(1_793_900_000)),
            }
        );
        assert_eq!(state.scopes.as_deref(), Some("notifications,read:user"));
    }

    #[test]
    fn observe_keeps_a_kick_that_ran_during_the_unit() {
        let mut fresh = SyncState {
            last_kick_at: Some(Timestamp(50)),
            ..SyncState::default()
        };
        let unit = SyncState {
            next_poll_at: Some(Timestamp(65)),
            rate: Rate {
                limit: Some(5000),
                remaining: Some(4990),
                reset: Some(Timestamp(3600)),
            },
            scopes: Some("notifications".into()),
            poll_phase: PollPhase::Idle {
                last_modified: Some("Sat, 07 Nov 2026 09:55:00 GMT".into()),
            },
            ..SyncState::default()
        };
        fresh.observe(&unit);
        assert_eq!(fresh.backfill_done_at, None);
        fresh.observe(&SyncState {
            backfill_done_at: Some(Timestamp(70)),
            ..unit.clone()
        });
        assert_eq!(fresh.backfill_done_at, Some(Timestamp(70)));
        fresh.observe(&unit);
        assert_eq!(fresh.backfill_done_at, Some(Timestamp(70)));
        assert_eq!(fresh.last_kick_at, Some(Timestamp(50)));
        assert_eq!(fresh.poll_phase, unit.poll_phase);
        assert_eq!(fresh.next_poll_at, Some(Timestamp(65)));
        assert_eq!(fresh.rate, unit.rate);
        assert_eq!(fresh.scopes, unit.scopes);
    }

    #[test]
    fn status_counts_rows_by_kind_and_names_the_block() {
        let now = Timestamp(1_000_000);
        let poll = WorkRow::pending(WorkKind::PollNotifications, now);
        let rollup = WorkRow {
            state: RowState::Dead,
            ..WorkRow::pending(WorkKind::Rollup, now)
        };
        let mut state = SyncState::default();
        state.throttle.no_token = true;
        let out = status(&[poll, rollup], &state, Some(now), now);
        assert_eq!(
            out.pending_by_kind,
            BTreeMap::from([(SyncKind::PollNotifications, 1)])
        );
        assert_eq!(out.dead_by_kind, BTreeMap::from([(SyncKind::Rollup, 1)]));
        assert_eq!(out.blocked, Some(DeferCause::NoToken));
        state.throttle = Throttle {
            retry_at: Some(Timestamp(now.0 + 60)),
            ..Throttle::default()
        };
        assert_eq!(blocked(&state, now), Some(DeferCause::RateLimited));
        assert_eq!(blocked(&state, Timestamp(now.0 + 60)), None);
    }
}
