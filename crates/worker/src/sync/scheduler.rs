//! The Syncer's rules as pure functions: which unit runs next, what a unit's
//! outcome does to its row, and what a kick enqueues.

use domain::ids::Timestamp;
use domain::sync_status::SyncKind;

use super::config::{BACKOFF_BASE_SECS, BACKOFF_MAX_SECS, MAX_ATTEMPTS, RETRY_DELAYS_SECS};
use super::gate::{gate, Gate};
use super::state::{SyncState, Throttle};
use super::work::{KickReason, RowState, WorkKind, WorkRow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitOutcome {
    Done { follow_ups: Vec<WorkKind> },
    Retry { error: String },
    RateLimited { retry_after_secs: u64 },
    NoToken,
}

impl UnitOutcome {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Done { .. } => "done",
            Self::Retry { .. } => "retry",
            Self::RateLimited { .. } => "rate_limited",
            Self::NoToken => "no_token",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Run(WorkRow),
    Sleep(Timestamp),
    Idle,
}

impl Decision {
    /// When the next alarm should fire for this decision.
    pub fn alarm(&self, now: Timestamp) -> Option<Timestamp> {
        match self {
            Self::Run(_) => Some(now),
            Self::Sleep(at) => Some(*at),
            Self::Idle => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowEffect {
    Delete,
    Update(WorkRow),
    MarkDead(WorkRow),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub row: RowEffect,
    pub throttle: Throttle,
}

/// The earliest time `row` may run, or `None` while it cannot run at all.
fn ready_at(row: &WorkRow, state: &SyncState, now: Timestamp) -> Option<Timestamp> {
    let kind = row.kind.kind();
    if row.is_dead() || (kind.calls_github() && state.throttle.no_token) {
        return None;
    }
    let mut at = row.due_at;
    if let Gate::WaitUntil(open) = gate(kind, state, now) {
        at = at.max(open);
    }
    if kind == SyncKind::PollNotifications {
        if let Some(next_poll) = state.next_poll_at {
            at = at.max(next_poll);
        }
    }
    Some(at)
}

pub fn plan(rows: &[WorkRow], state: &SyncState, now: Timestamp) -> Decision {
    let ready: Vec<(Timestamp, &WorkRow)> = rows
        .iter()
        .filter_map(|row| ready_at(row, state, now).map(|at| (at, row)))
        .collect();
    let runnable = ready
        .iter()
        .filter(|(at, _)| *at <= now)
        .map(|(_, row)| *row)
        .min_by_key(|row| (row.due_at, row.kind.kind(), row.kind.key()));
    if let Some(row) = runnable {
        return Decision::Run(row.clone());
    }
    match ready.iter().map(|(at, _)| *at).min() {
        Some(at) => Decision::Sleep(at),
        None => Decision::Idle,
    }
}

fn after(now: Timestamp, secs: u64) -> Timestamp {
    Timestamp(
        now.0
            .saturating_add(i64::try_from(secs).unwrap_or(i64::MAX)),
    )
}

pub fn apply(row: WorkRow, outcome: &UnitOutcome, throttle: Throttle, now: Timestamp) -> Applied {
    match outcome {
        UnitOutcome::Done { .. } => Applied {
            row: RowEffect::Delete,
            throttle: Throttle::default(),
        },
        UnitOutcome::Retry { error } => {
            let attempts = row.attempts.saturating_add(1);
            let failed = WorkRow {
                attempts,
                last_error: Some(error.clone()),
                ..row
            };
            let delay = usize::try_from(attempts - 1)
                .ok()
                .and_then(|i| RETRY_DELAYS_SECS.get(i));
            let effect = match delay {
                Some(&secs) if attempts < MAX_ATTEMPTS => RowEffect::Update(WorkRow {
                    due_at: Timestamp(now.0.saturating_add(secs)),
                    ..failed
                }),
                _ => RowEffect::MarkDead(WorkRow {
                    state: RowState::Dead,
                    ..failed
                }),
            };
            Applied {
                row: effect,
                throttle: Throttle {
                    no_token: false,
                    ..throttle
                },
            }
        }
        UnitOutcome::RateLimited { retry_after_secs } => {
            let doubled = 1u64
                .checked_shl(throttle.strikes)
                .map_or(u64::MAX, |factor| BACKOFF_BASE_SECS.saturating_mul(factor));
            let until = after(now, doubled.max(*retry_after_secs).min(BACKOFF_MAX_SECS));
            Applied {
                row: RowEffect::Update(WorkRow {
                    due_at: until,
                    ..row
                }),
                throttle: Throttle {
                    strikes: throttle.strikes.saturating_add(1),
                    retry_at: Some(until),
                    no_token: false,
                },
            }
        }
        UnitOutcome::NoToken => Applied {
            row: RowEffect::Update(row),
            throttle: Throttle {
                no_token: true,
                ..throttle
            },
        },
    }
}

fn find<'a>(rows: &'a [WorkRow], kind: &WorkKind) -> Option<&'a WorkRow> {
    rows.iter().find(|row| &row.kind == kind)
}

/// The row to insert for `kind`, or `None` when it is already queued.
pub fn enqueue(rows: &[WorkRow], kind: WorkKind, due_at: Timestamp) -> Option<WorkRow> {
    match find(rows, &kind) {
        None => Some(WorkRow::pending(kind, due_at)),
        Some(_) => None,
    }
}

/// Queues `kind` due at `due_at`, or brings its dead row back with zero attempts.
fn revive_or_insert(rows: &[WorkRow], kind: WorkKind, due_at: Timestamp) -> Option<WorkRow> {
    match find(rows, &kind) {
        None => Some(WorkRow::pending(kind, due_at)),
        Some(row) if row.is_dead() => Some(row.clone().revived(due_at)),
        Some(_) => None,
    }
}

/// Deferred, not dropped (ADR 0002). Returns the rows to upsert by key.
pub fn kick(
    reason: KickReason,
    rows: &[WorkRow],
    state: &mut SyncState,
    now: Timestamp,
) -> Vec<WorkRow> {
    state.last_kick_at = Some(now);
    state.throttle.no_token = false;
    match reason {
        KickReason::Rollup => {
            let dead_backfills = rows
                .iter()
                .filter(|row| row.is_dead() && row.kind.kind() == SyncKind::BackfillPage)
                .map(|row| row.clone().revived(now));
            revive_or_insert(rows, WorkKind::Rollup, now)
                .into_iter()
                .chain(dead_backfills)
                .collect()
        }
        KickReason::Reconcile
        | KickReason::Email
        | KickReason::Webhook
        | KickReason::SignedIn
        | KickReason::Manual => {
            let due = state.next_poll_at.map_or(now, |next| next.max(now));
            revive_or_insert(rows, WorkKind::PollNotifications, due)
                .into_iter()
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::sync_status::Rate;

    use super::*;
    use crate::sync::work::{Cursor, EnrichBatch, NodeId};

    const NOW: Timestamp = Timestamp(1_000_000);

    fn at(offset: i64) -> Timestamp {
        Timestamp(NOW.0 + offset)
    }

    fn enrich(id: &str) -> WorkKind {
        WorkKind::Enrich(EnrichBatch::new(vec![NodeId::parse(id).unwrap()]).unwrap())
    }

    fn row(kind: WorkKind, due: i64) -> WorkRow {
        WorkRow::pending(kind, at(due))
    }

    fn dead(kind: WorkKind) -> WorkRow {
        WorkRow {
            attempts: 3,
            state: RowState::Dead,
            last_error: Some("server status=500".into()),
            ..row(kind, -100)
        }
    }

    fn low_budget() -> SyncState {
        SyncState {
            rate: Rate {
                limit: Some(5000),
                remaining: Some(400),
                reset: Some(at(20)),
            },
            ..SyncState::default()
        }
    }

    fn upsert(rows: &mut Vec<WorkRow>, new: Vec<WorkRow>) {
        for new in new {
            rows.retain(|r| r.kind != new.kind);
            rows.push(new);
        }
    }

    #[test]
    fn picks_the_earliest_due_row() {
        let rows = vec![
            row(enrich("PR_b"), -5),
            row(enrich("PR_a"), -10),
            row(WorkKind::Rollup, 30),
        ];
        assert_eq!(
            plan(&rows, &SyncState::default(), NOW),
            Decision::Run(row(enrich("PR_a"), -10))
        );
    }

    #[test]
    fn skips_dead_rows() {
        let rows = vec![dead(enrich("PR_a")), row(enrich("PR_b"), 0)];
        assert_eq!(
            plan(&rows, &SyncState::default(), NOW),
            Decision::Run(row(enrich("PR_b"), 0))
        );
        assert_eq!(
            plan(&[dead(enrich("PR_a"))], &SyncState::default(), NOW),
            Decision::Idle
        );
    }

    #[test]
    fn poll_waits_for_next_poll_at() {
        let state = SyncState {
            next_poll_at: Some(at(5)),
            ..SyncState::default()
        };
        let rows = vec![row(WorkKind::PollNotifications, 0)];
        assert_eq!(plan(&rows, &state, NOW), Decision::Sleep(at(5)));
        assert!(matches!(plan(&rows, &state, at(5)), Decision::Run(_)));
    }

    #[test]
    fn enrich_waits_while_the_gate_is_closed() {
        let state = low_budget();
        let rows = vec![
            row(enrich("PR_a"), 0),
            row(WorkKind::BackfillPage(Cursor::Start), 0),
        ];
        assert_eq!(plan(&rows, &state, NOW), Decision::Sleep(at(20)));
        assert!(matches!(plan(&rows, &state, at(20)), Decision::Run(_)));
        let with_poll = vec![row(enrich("PR_a"), -1), row(WorkKind::PollNotifications, 0)];
        assert_eq!(
            plan(&with_poll, &state, NOW),
            Decision::Run(row(WorkKind::PollNotifications, 0))
        );
    }

    #[test]
    fn rollup_ignores_the_gate() {
        let mut state = low_budget();
        state.throttle.retry_at = Some(at(600));
        state.throttle.no_token = true;
        let rows = vec![row(enrich("PR_a"), -1), row(WorkKind::Rollup, 0)];
        assert_eq!(
            plan(&rows, &state, NOW),
            Decision::Run(row(WorkKind::Rollup, 0))
        );
    }

    #[test]
    fn idle_when_nothing_is_pending() {
        assert_eq!(plan(&[], &SyncState::default(), NOW), Decision::Idle);
        assert_eq!(Decision::Idle.alarm(NOW), None);
    }

    #[test]
    fn no_token_parks_github_rows_until_a_kick() {
        let mut state = SyncState::default();
        state.throttle.no_token = true;
        let rows = vec![row(WorkKind::PollNotifications, 0)];
        assert_eq!(plan(&rows, &state, NOW), Decision::Idle);
        kick(KickReason::Reconcile, &rows, &mut state, NOW);
        assert!(matches!(plan(&rows, &state, NOW), Decision::Run(_)));
    }

    #[test]
    fn success_deletes_the_row() {
        let done = UnitOutcome::Done { follow_ups: vec![] };
        let applied = apply(row(enrich("PR_a"), 0), &done, Throttle::default(), NOW);
        assert_eq!(applied.row, RowEffect::Delete);
    }

    fn fail(r: WorkRow) -> RowEffect {
        let retry = UnitOutcome::Retry {
            error: "server status=500".into(),
        };
        apply(r, &retry, Throttle::default(), NOW).row
    }

    #[test]
    fn first_failure_retries_in_10_seconds() {
        let RowEffect::Update(r) = fail(row(enrich("PR_a"), 0)) else {
            panic!("expected a retry");
        };
        assert_eq!(
            (r.attempts, r.due_at, r.state),
            (1, at(10), RowState::Pending)
        );
        assert_eq!(r.last_error.as_deref(), Some("server status=500"));
    }

    #[test]
    fn second_failure_retries_in_60_seconds() {
        let once = WorkRow {
            attempts: 1,
            ..row(enrich("PR_a"), 0)
        };
        let RowEffect::Update(r) = fail(once) else {
            panic!("expected a retry");
        };
        assert_eq!((r.attempts, r.due_at), (2, at(60)));
    }

    #[test]
    fn third_failure_marks_dead() {
        let twice = WorkRow {
            attempts: 2,
            ..row(enrich("PR_a"), 0)
        };
        let RowEffect::MarkDead(r) = fail(twice) else {
            panic!("expected dead");
        };
        assert_eq!((r.attempts, r.state), (3, RowState::Dead));
        assert_eq!(r.last_error.as_deref(), Some("server status=500"));
    }

    #[test]
    fn rate_limited_adds_no_attempt() {
        let limited = UnitOutcome::RateLimited {
            retry_after_secs: 15,
        };
        let applied = apply(row(enrich("PR_a"), 0), &limited, Throttle::default(), NOW);
        let RowEffect::Update(r) = applied.row else {
            panic!("expected an update");
        };
        assert_eq!((r.attempts, r.due_at), (0, at(60)));
        assert_eq!(applied.throttle.retry_at, Some(at(60)));
    }

    #[test]
    fn strikes_double_the_wait_and_reset_on_success() {
        let limited = |secs| UnitOutcome::RateLimited {
            retry_after_secs: secs,
        };
        let mut throttle = Throttle::default();
        let mut waits = Vec::new();
        for _ in 0..8 {
            throttle = apply(row(enrich("PR_a"), 0), &limited(15), throttle, NOW).throttle;
            waits.push(throttle.retry_at.map(|t| t.0 - NOW.0));
        }
        let expected = [60, 120, 240, 480, 960, 1920, 3600, 3600].map(Some);
        assert_eq!(waits, expected);
        assert_eq!(throttle.strikes, 8);
        let long = apply(
            row(enrich("PR_a"), 0),
            &limited(900),
            Throttle::default(),
            NOW,
        );
        assert_eq!(long.throttle.retry_at, Some(at(900)));
        let done = UnitOutcome::Done { follow_ups: vec![] };
        let reset = apply(row(enrich("PR_a"), 0), &done, throttle, NOW).throttle;
        assert_eq!(reset, Throttle::default());
    }

    #[test]
    fn many_kicks_make_one_row() {
        let mut rows = Vec::new();
        let mut state = SyncState::default();
        for n in 0..100 {
            let new = kick(KickReason::Email, &rows, &mut state, at(n / 50));
            upsert(&mut rows, new);
        }
        assert_eq!(rows, vec![row(WorkKind::PollNotifications, 0)]);
        assert_eq!(state.last_kick_at, Some(at(1)));
    }

    #[test]
    fn kick_during_cooldown_is_deferred_not_dropped() {
        let mut state = SyncState {
            next_poll_at: Some(at(5)),
            ..SyncState::default()
        };
        let rows = kick(KickReason::Email, &[], &mut state, NOW);
        assert_eq!(rows, vec![row(WorkKind::PollNotifications, 5)]);
        assert_eq!(plan(&rows, &state, NOW).alarm(NOW), Some(at(5)));
    }

    #[test]
    fn rollup_kick_inserts_nightly_once() {
        let mut state = SyncState::default();
        let mut rows = Vec::new();
        let first = kick(KickReason::Rollup, &rows, &mut state, NOW);
        assert_eq!(first, vec![row(WorkKind::Rollup, 0)]);
        upsert(&mut rows, first);
        assert!(kick(KickReason::Rollup, &rows, &mut state, at(1)).is_empty());
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn kick_revives_a_dead_poll_row_with_zero_attempts() {
        let mut state = SyncState::default();
        let rows = vec![dead(WorkKind::PollNotifications)];
        let revived = kick(KickReason::Reconcile, &rows, &mut state, NOW);
        assert_eq!(revived, vec![row(WorkKind::PollNotifications, 0)]);
    }

    #[test]
    fn nightly_kick_rearms_a_dead_backfill_row_at_its_cursor() {
        let mut state = SyncState::default();
        let cursor = WorkKind::BackfillPage(Cursor::After("Y3Vyc29yOjM=".into()));
        let rows = vec![dead(cursor.clone()), row(WorkKind::Rollup, -1)];
        let rearmed = kick(KickReason::Rollup, &rows, &mut state, NOW);
        assert_eq!(rearmed, vec![row(cursor, 0)]);
    }

    #[test]
    fn reconcile_kick_leaves_a_dead_enrich_row_dead() {
        let mut state = SyncState::default();
        let rows = vec![
            dead(enrich("PR_a")),
            dead(WorkKind::BackfillPage(Cursor::Start)),
        ];
        let new = kick(KickReason::Reconcile, &rows, &mut state, NOW);
        assert_eq!(new, vec![row(WorkKind::PollNotifications, 0)]);
    }

    #[test]
    fn a_kick_leaves_a_pending_poll_row_alone() {
        let mut state = SyncState::default();
        let backing_off = WorkRow {
            attempts: 1,
            ..row(WorkKind::PollNotifications, 10)
        };
        assert!(kick(KickReason::Manual, &[backing_off], &mut state, NOW).is_empty());
    }

    #[test]
    fn enqueue_inserts_only_an_absent_unit() {
        let rows = vec![row(enrich("PR_a"), 0)];
        assert_eq!(enqueue(&rows, enrich("PR_a"), NOW), None);
        assert_eq!(
            enqueue(&rows, enrich("PR_b"), NOW),
            Some(row(enrich("PR_b"), 0))
        );
    }
}
