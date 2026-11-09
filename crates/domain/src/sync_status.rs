//! What the Syncer reports about its queue, read by `GET /api/freshness`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncKind {
    PollNotifications,
    Enrich,
    BackfillPage,
    Rollup,
}

impl SyncKind {
    pub const ALL: [SyncKind; 4] = [
        SyncKind::PollNotifications,
        SyncKind::Enrich,
        SyncKind::BackfillPage,
        SyncKind::Rollup,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PollNotifications => "poll_notifications",
            Self::Enrich => "enrich",
            Self::BackfillPage => "backfill_page",
            Self::Rollup => "rollup",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == s)
    }

    pub fn calls_github(self) -> bool {
        !matches!(self, Self::Rollup)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeferCause {
    RateLimited,
    NoToken,
}

impl DeferCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RateLimited => "rate_limited",
            Self::NoToken => "no_token",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rate {
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncStatus {
    pub pending_by_kind: BTreeMap<SyncKind, u32>,
    pub dead_by_kind: BTreeMap<SyncKind, u32>,
    pub next_alarm: Option<Timestamp>,
    pub next_poll_at: Option<Timestamp>,
    pub rate: Rate,
    pub last_ok_by_kind: BTreeMap<SyncKind, Timestamp>,
    pub blocked: Option<DeferCause>,
    pub dead: Vec<DeadWork>,
    /// The last `X-OAuth-Scopes` GitHub sent, or `None` before any.
    pub scopes: Option<String>,
}

/// A unit the Syncer gave up on. `since` is when its last attempt was due.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DeadWork {
    pub kind: SyncKind,
    pub key: String,
    pub attempts: u32,
    pub since: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_json() {
        let status = SyncStatus {
            pending_by_kind: BTreeMap::from([(SyncKind::Enrich, 2), (SyncKind::Rollup, 1)]),
            dead_by_kind: BTreeMap::from([(SyncKind::PollNotifications, 1)]),
            next_alarm: Some(Timestamp(1_793_900_010)),
            next_poll_at: Some(Timestamp(1_793_900_005)),
            rate: Rate {
                limit: Some(5000),
                remaining: Some(4990),
                reset: Some(Timestamp(1_793_903_000)),
            },
            last_ok_by_kind: BTreeMap::from([(SyncKind::Rollup, Timestamp(1_793_890_000))]),
            blocked: Some(DeferCause::NoToken),
            dead: vec![DeadWork {
                kind: SyncKind::PollNotifications,
                key: "main".into(),
                attempts: 3,
                since: Timestamp(1_793_899_000),
            }],
            scopes: Some("notifications,read:user".into()),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains(r#""pending_by_kind":{"enrich":2,"rollup":1}"#));
        assert!(json.contains(r#""blocked":"no_token""#));
        assert!(json.contains(r#""next_poll_at":"2026-11-05T"#), "{json}");
        let back: SyncStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, status);
    }

    #[test]
    fn kind_names_round_trip() {
        for kind in SyncKind::ALL {
            assert_eq!(SyncKind::parse(kind.as_str()), Some(kind));
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.as_str()));
        }
        assert_eq!(SyncKind::parse("seed_history"), None);
    }
}
