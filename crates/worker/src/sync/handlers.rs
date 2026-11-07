//! What each kind of unit does. Until R11 replaces it, `BackfillPage` only reads
//! `GET /rate_limit` and records what it reports.

use domain::ids::Timestamp;
use serde::de::IgnoredAny;
use worker::Env;

use super::enrich;
use super::poll;
use super::scheduler::UnitOutcome;
use super::state::SyncState;
use super::work::WorkKind;
use crate::auth;
use crate::github::{ApiPath, Client, Conditional, Fetched, GithubError};

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
        WorkKind::BackfillPage(_) => check_rate(env, state).await.into(),
    }
}

async fn check_rate(env: &Env, state: &mut SyncState) -> UnitOutcome {
    let Some(token) = auth::load_token(env).await else {
        return UnitOutcome::NoToken;
    };
    let base = env
        .var("GITHUB_API_BASE")
        .map(|v| v.to_string())
        .unwrap_or_default();
    let fetched = match (Client::new(&base, token), ApiPath::new("/rate_limit")) {
        (Ok(client), Ok(path)) => client.get::<IgnoredAny>(path, Conditional::None).await,
        (Err(err), _) | (_, Err(err)) => Err(err),
    };
    match fetched {
        Ok(Fetched::Modified { meta, .. } | Fetched::NotModified { meta }) => {
            state.record_rate(&meta);
            UnitOutcome::Done { follow_ups: vec![] }
        }
        Err(GithubError::RateLimited { retry_after_secs }) => {
            UnitOutcome::RateLimited { retry_after_secs }
        }
        Err(err) => UnitOutcome::Retry {
            error: err.to_string(),
        },
    }
}
