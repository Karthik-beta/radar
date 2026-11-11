//! The Syncer's rules as pure functions: which unit runs next, what a unit's
//! outcome does to its row, and what a kick enqueues.

use domain::ids::Timestamp;
use domain::sync_status::SyncKind;

use super::backfill::{BackfillCursor, SearchWindow};
use super::config::{BACKOFF_BASE_SECS, BACKOFF_MAX_SECS, MAX_ATTEMPTS, RETRY_DELAYS_SECS};
use super::gate::{gate, Gate};
use super::state::{SyncState, Throttle};
use super::work::{KickReason, RowState, WorkKind, WorkRow};
use crate::rollup::RollupKey;

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

/// The larger of `Retry-After` and `60 s * 2^strikes`, at most an hour (P4).
pub fn rate_limit_until(strikes: u32, retry_after_secs: u64, now: Timestamp) -> Timestamp {
    let doubled = 1u64
        .checked_shl(strikes)
        .map_or(u64::MAX, |factor| BACKOFF_BASE_SECS.saturating_mul(factor));
    after(now, doubled.max(retry_after_secs).min(BACKOFF_MAX_SECS))
}

pub fn apply(row: WorkRow, outcome: &UnitOutcome, throttle: Throttle, now: Timestamp) -> Applied {
    match outcome {
        UnitOutcome::Done { .. } => Applied {
            row: RowEffect::Delete,
            throttle: if row.kind.kind().calls_github() {
                Throttle::default()
            } else {
                throttle
            },
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
            let until = rate_limit_until(throttle.strikes, *retry_after_secs, now);
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

fn window_of(row: &WorkRow) -> Option<SearchWindow> {
    match &row.kind {
        WorkKind::BackfillPage(cursor) => Some(cursor.window),
        _ => None,
    }
}

/// Starts the three searches of `window` at the first page, unless some of them are
/// already queued. A dead page of the window comes back with zero attempts at its cursor.
fn start_window(rows: &[WorkRow], window: SearchWindow, now: Timestamp) -> Vec<WorkRow> {
    let mut queued = rows.iter().filter(|row| window_of(row) == Some(window));
    if queued.next().is_none() {
        return vec![WorkRow::pending(
            WorkKind::BackfillPage(BackfillCursor::first(window, now)),
            now,
        )];
    }
    rows.iter()
        .filter(|row| window_of(row) == Some(window) && row.is_dead())
        .map(|row| row.clone().revived(now))
        .collect()
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
    let backfill_wanted = state.backfill_done_at.is_none();
    match reason {
        KickReason::Rollup => {
            let mut queued: Vec<WorkRow> =
                revive_or_insert(rows, WorkKind::Rollup(RollupKey::Nightly), now)
                    .into_iter()
                    .collect();
            queued.extend(start_window(rows, SearchWindow::Nightly7d, now));
            if backfill_wanted {
                queued.extend(start_window(rows, SearchWindow::Backfill90d, now));
            }
            let dead_backfills = rows
                .iter()
                .filter(|row| row.is_dead() && row.kind.kind() == SyncKind::BackfillPage)
                .filter(|row| !queued.iter().any(|q| q.kind == row.kind))
                .map(|row| row.clone().revived(now))
                .collect::<Vec<_>>();
            queued.extend(dead_backfills);
            queued
        }
        KickReason::SignedIn => {
            let due = state.next_poll_at.map_or(now, |next| next.max(now));
            let mut queued: Vec<WorkRow> = revive_or_insert(rows, WorkKind::PollNotifications, due)
                .into_iter()
                .collect();
            if backfill_wanted {
                queued.extend(start_window(rows, SearchWindow::Backfill90d, now));
            }
            queued
        }
        KickReason::Reconcile | KickReason::Email | KickReason::Webhook | KickReason::Manual => {
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
    use crate::sync::backfill::SearchKind;
    use crate::sync::work::{EnrichBatch, NodeId};

    const NOW: Timestamp = Timestamp(1_000_000);

    fn backfill_start(window: SearchWindow) -> WorkKind {
        WorkKind::BackfillPage(BackfillCursor::first(window, NOW))
    }

    fn backfill_page(window: SearchWindow, kind: SearchKind, page: u32, after: &str) -> WorkKind {
        WorkKind::BackfillPage(BackfillCursor {
            since: window.since(NOW),
            window,
            kind,
            page,
            after: Some(after.to_owned()),
        })
    }

    fn done_state() -> SyncState {
        SyncState {
            backfill_done_at: Some(at(-500)),
            ..SyncState::default()
        }
    }

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
            row(WorkKind::Rollup(RollupKey::Nightly), 30),
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
            row(backfill_start(SearchWindow::Backfill90d), 0),
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
        let rows = vec![
            row(enrich("PR_a"), -1),
            row(WorkKind::Rollup(RollupKey::Nightly), 0),
        ];
        assert_eq!(
            plan(&rows, &state, NOW),
            Decision::Run(row(WorkKind::Rollup(RollupKey::Nightly), 0))
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
    fn rollup_done_keeps_the_github_backoff() {
        let limited = UnitOutcome::RateLimited {
            retry_after_secs: 15,
        };
        let backoff = apply(row(enrich("PR_a"), 0), &limited, Throttle::default(), NOW).throttle;
        let done = UnitOutcome::Done { follow_ups: vec![] };
        let kept = apply(
            row(WorkKind::Rollup(RollupKey::Nightly), 0),
            &done,
            backoff,
            NOW,
        )
        .throttle;
        assert_eq!(kept, backoff);
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
        let mut state = done_state();
        let mut rows = Vec::new();
        let first = kick(KickReason::Rollup, &rows, &mut state, NOW);
        assert_eq!(
            first,
            vec![
                row(WorkKind::Rollup(RollupKey::Nightly), 0),
                row(backfill_start(SearchWindow::Nightly7d), 0)
            ]
        );
        upsert(&mut rows, first);
        assert!(kick(KickReason::Rollup, &rows, &mut state, at(1)).is_empty());
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn nightly_kick_starts_the_search_not_a_second_backfill_once_the_backfill_is_done() {
        let mut state = done_state();
        let queued = kick(KickReason::Rollup, &[], &mut state, NOW);
        assert!(queued
            .iter()
            .all(|r| window_of(r) != Some(SearchWindow::Backfill90d)));
        assert!(queued
            .iter()
            .any(|r| window_of(r) == Some(SearchWindow::Nightly7d)));
    }

    #[test]
    fn nightly_kick_starts_a_backfill_that_never_ran() {
        let mut state = SyncState::default();
        let queued = kick(KickReason::Rollup, &[], &mut state, NOW);
        assert!(queued.contains(&row(backfill_start(SearchWindow::Backfill90d), 0)));
        assert!(queued.contains(&row(backfill_start(SearchWindow::Nightly7d), 0)));
    }

    #[test]
    fn a_nightly_search_in_progress_is_left_alone() {
        let mut state = done_state();
        let page = backfill_page(SearchWindow::Nightly7d, SearchKind::Involves, 1, "Y3I=");
        let rows = vec![row(WorkKind::Rollup(RollupKey::Nightly), -1), row(page, -1)];
        assert!(kick(KickReason::Rollup, &rows, &mut state, NOW).is_empty());
    }

    #[test]
    fn sign_in_polls_and_starts_the_first_backfill() {
        let mut state = SyncState::default();
        let queued = kick(KickReason::SignedIn, &[], &mut state, NOW);
        assert_eq!(
            queued,
            vec![
                row(WorkKind::PollNotifications, 0),
                row(backfill_start(SearchWindow::Backfill90d), 0)
            ]
        );
    }

    #[test]
    fn a_second_sign_in_after_the_backfill_finished_only_polls() {
        let mut state = done_state();
        let queued = kick(KickReason::SignedIn, &[], &mut state, NOW);
        assert_eq!(queued, vec![row(WorkKind::PollNotifications, 0)]);
    }

    #[test]
    fn sign_in_during_a_backfill_does_not_restart_it() {
        let mut state = SyncState::default();
        let page = backfill_page(SearchWindow::Backfill90d, SearchKind::Authored, 3, "Y3I=");
        let rows = vec![row(page, 0)];
        let queued = kick(KickReason::SignedIn, &rows, &mut state, NOW);
        assert_eq!(queued, vec![row(WorkKind::PollNotifications, 0)]);
    }

    #[test]
    fn sign_in_revives_a_dead_backfill_page_at_its_cursor() {
        let mut state = SyncState::default();
        let page = backfill_page(SearchWindow::Backfill90d, SearchKind::Authored, 3, "Y3I=");
        let queued = kick(KickReason::SignedIn, &[dead(page.clone())], &mut state, NOW);
        assert!(queued.contains(&row(page, 0)));
    }

    #[test]
    fn other_kicks_never_start_a_backfill() {
        for reason in [
            KickReason::Reconcile,
            KickReason::Email,
            KickReason::Webhook,
            KickReason::Manual,
        ] {
            let mut state = SyncState::default();
            let queued = kick(reason, &[], &mut state, NOW);
            assert_eq!(
                queued,
                vec![row(WorkKind::PollNotifications, 0)],
                "{reason:?}"
            );
        }
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
        let mut state = done_state();
        let cursor = backfill_page(
            SearchWindow::Backfill90d,
            SearchKind::Authored,
            3,
            "Y3Vyc29yOjM=",
        );
        let rows = vec![
            dead(cursor.clone()),
            row(WorkKind::Rollup(RollupKey::Nightly), -1),
        ];
        let rearmed = kick(KickReason::Rollup, &rows, &mut state, NOW);
        assert!(rearmed.contains(&row(cursor, 0)));
        assert!(rearmed.contains(&row(backfill_start(SearchWindow::Nightly7d), 0)));
        assert_eq!(rearmed.len(), 2);
    }

    #[test]
    fn nightly_kick_rearms_a_dead_nightly_page_once() {
        let mut state = done_state();
        let cursor = backfill_page(SearchWindow::Nightly7d, SearchKind::Involves, 2, "Y3I=");
        let rows = vec![
            dead(cursor.clone()),
            row(WorkKind::Rollup(RollupKey::Nightly), -1),
        ];
        assert_eq!(
            kick(KickReason::Rollup, &rows, &mut state, NOW),
            vec![row(cursor, 0)]
        );
    }

    #[test]
    fn reconcile_kick_leaves_a_dead_enrich_row_dead() {
        let mut state = SyncState::default();
        let rows = vec![
            dead(enrich("PR_a")),
            dead(backfill_start(SearchWindow::Backfill90d)),
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
