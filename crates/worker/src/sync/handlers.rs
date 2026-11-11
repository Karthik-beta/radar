//! What each kind of unit does.

use domain::ids::Timestamp;
use worker::Env;

use super::backfill;
use super::enrich;
use super::history;
use super::poll;
use super::scheduler::UnitOutcome;
use super::state::SyncState;
use super::work::WorkKind;

/// What a unit reports to the Syncer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handled {
    pub outcome: UnitOutcome,
    /// Set by a poll that has seen its last page. The page-1 request time decides
    /// whether a kick that landed meanwhile needs another poll (P2).
    pub finished_from: Option<Timestamp>,
}

impl From<UnitOutcome> for Handled {
    fn from(outcome: UnitOutcome) -> Self {
        Self {
            outcome,
            finished_from: None,
        }
    }
}

/// Each GitHub call is awaited before the next starts, so no two requests for the
/// token overlap (P5).
pub async fn run(kind: &WorkKind, env: &Env, state: &mut SyncState, now: Timestamp) -> Handled {
    match kind {
        WorkKind::Rollup(key) => crate::rollup::run_unit(*key, env, now).await.into(),
        WorkKind::PollNotifications => poll::run(env, state).await,
        WorkKind::Enrich(batch) => enrich::run(batch, env, state, now).await.into(),
        WorkKind::BackfillPage(cursor) => backfill::run(cursor, env, state, now).await.into(),
        WorkKind::SeedHistory(key) => history::run(key, env, state, now).await.into(),
        WorkKind::SendAlerts => send_alerts(env, now).await.into(),
    }
}

async fn send_alerts(env: &Env, now: Timestamp) -> UnitOutcome {
    match env.d1("DB") {
        Ok(db) => crate::alerts::dispatch(&db, env, now).await,
        Err(err) => UnitOutcome::Retry {
            error: format!("d1 binding: {err}"),
        },
    }
}
