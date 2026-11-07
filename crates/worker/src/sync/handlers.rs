//! What each kind of unit does. Until R09 to R11 replace them, the GitHub kinds
//! only read `GET /rate_limit` and record what it reports.

use domain::ids::Timestamp;
use serde::de::IgnoredAny;
use worker::Env;

use super::config::DEFAULT_POLL_INTERVAL_SECS;
use super::scheduler::UnitOutcome;
use super::state::SyncState;
use super::work::WorkKind;
use crate::auth;
use crate::github::{ApiPath, Client, Conditional, Fetched, GithubError, RateMeta};

/// Each GitHub call is awaited before the next starts, so no two requests for the
/// token overlap (P5).
pub async fn run(kind: &WorkKind, env: &Env, state: &mut SyncState, now: Timestamp) -> UnitOutcome {
    let done = UnitOutcome::Done { follow_ups: vec![] };
    match kind {
        WorkKind::Rollup => done,
        WorkKind::PollNotifications => match check_rate(env, state).await {
            Ok(meta) => {
                let interval = meta.poll_interval.unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
                state.next_poll_at = Some(Timestamp(now.0.saturating_add(i64::from(interval))));
                done
            }
            Err(outcome) => outcome,
        },
        WorkKind::Enrich(_) | WorkKind::BackfillPage(_) => match check_rate(env, state).await {
            Ok(_) => done,
            Err(outcome) => outcome,
        },
    }
}

async fn check_rate(env: &Env, state: &mut SyncState) -> Result<RateMeta, UnitOutcome> {
    let token = auth::load_token(env).await.ok_or(UnitOutcome::NoToken)?;
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
            Ok(meta)
        }
        Err(GithubError::RateLimited { retry_after_secs }) => {
            Err(UnitOutcome::RateLimited { retry_after_secs })
        }
        Err(err) => Err(UnitOutcome::Retry {
            error: err.to_string(),
        }),
    }
}
