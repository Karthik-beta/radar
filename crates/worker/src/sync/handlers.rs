//! What each kind of unit does.

use domain::ids::Timestamp;
use worker::Env;

use super::backfill;
use super::enrich;
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
        WorkKind::Rollup => UnitOutcome::Done { follow_ups: vec![] }.into(),
        WorkKind::PollNotifications => poll::run(env, state).await,
        WorkKind::Enrich(batch) => enrich::run(batch, env, state, now).await.into(),
        WorkKind::BackfillPage(cursor) => backfill::run(cursor, env, state, now).await.into(),
    }
}
