//! The JSON the Worker serves under `/api/` and the UI reads. A field rename
//! breaks `tests/api_contract.rs`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{ItemKind, Login, RepoName, Timestamp};
use crate::settings::NudgeThresholds;
use crate::stage::Stage;
use crate::sync_status::{DeadWork, SyncKind, SyncStatus};
use crate::turn::Reason;

/// Merged or closed items stay in `Dashboard.recent` this long.
pub const RECENT_WINDOW_SECS: i64 = 14 * 86_400;

/// A GitHub node id, the key of `items` and `local_state`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ItemId(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadItemId(pub String);

impl fmt::Display for BadItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid item id `{}`", self.0)
    }
}

impl std::error::Error for BadItemId {}

impl ItemId {
    const MAX_LEN: usize = 128;

    pub fn parse(s: &str) -> Result<Self, BadItemId> {
        let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '=');
        if s.is_empty() || s.len() > Self::MAX_LEN || !s.chars().all(allowed) {
            return Err(BadItemId(s.to_owned()));
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ItemId {
    type Error = BadItemId;
    fn try_from(s: String) -> Result<Self, BadItemId> {
        Self::parse(&s)
    }
}

impl From<ItemId> for String {
    fn from(id: ItemId) -> Self {
        id.0
    }
}

/// `GET /api/dashboard`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Dashboard {
    pub generated_at: Timestamp,
    pub counters: Counters,
    pub your_turn: Vec<TurnRow>,
    pub their_turn: Vec<TheirRow>,
    pub pipeline: Pipeline,
    pub recent: Vec<RecentRow>,
    pub nudge: NudgeThresholds,
}

/// The header tabs (F5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Counters {
    pub your_turn: u32,
    pub their_turn: u32,
    pub open_prs: u32,
    pub merged_this_month: u32,
}

/// What the last human did, as stored in `items.last_human_kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Comment,
    Review,
    ReviewRequested,
    Assigned,
    Pushed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LastAction {
    pub actor: Login,
    pub what: Action,
    pub at: Timestamp,
}

/// One Your turn row (F1, F2). `actor` and `what` are empty when no human
/// acted, as when CI failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TurnRow {
    pub id: ItemId,
    pub kind: ItemKind,
    pub repo: RepoName,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub reason: Reason,
    pub actor: Option<Login>,
    pub what: Option<Action>,
    pub since: Timestamp,
    pub agent: bool,
}

/// One Their turn row (F4). `waiting_on` lists the requested reviewers, and
/// is empty when the item waits on the maintainers in general.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TheirRow {
    pub id: ItemId,
    pub kind: ItemKind,
    pub repo: RepoName,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub since: Timestamp,
    pub agent: bool,
    pub last_action: Option<LastAction>,
    pub waiting_on: Vec<Login>,
}

/// My open pull requests, always all six open stages in this order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Pipeline {
    pub columns: [PipelineColumn; 6],
}

impl Pipeline {
    pub const STAGES: [Stage; 6] = [
        Stage::Draft,
        Stage::CiRunning,
        Stage::CiFailing,
        Stage::AwaitingReview,
        Stage::ChangesRequested,
        Stage::Approved,
    ];

    pub fn empty() -> Self {
        Self {
            columns: Self::STAGES.map(|stage| PipelineColumn {
                stage,
                cards: Vec::new(),
            }),
        }
    }

    pub fn card_count(&self) -> usize {
        self.columns.iter().map(|c| c.cards.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PipelineColumn {
    pub stage: Stage,
    pub cards: Vec<PipelineCard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PipelineCard {
    pub id: ItemId,
    pub repo: RepoName,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub agent: bool,
    pub stage_since: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ended {
    Merged,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RecentRow {
    pub id: ItemId,
    pub kind: ItemKind,
    pub repo: RepoName,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: Ended,
    pub ended_at: Timestamp,
}

/// The body of `POST /api/items/:id/snooze`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct SnoozeRequest {
    #[serde(rename = "for")]
    pub snooze_for: SnoozeFor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnoozeFor {
    #[serde(rename = "1d")]
    Day,
    #[serde(rename = "1w")]
    Week,
}

impl SnoozeFor {
    pub fn secs(self) -> i64 {
        match self {
            Self::Day => 86_400,
            Self::Week => 7 * 86_400,
        }
    }

    pub fn wake_at(self, now: Timestamp) -> Timestamp {
        Timestamp(now.0.saturating_add(self.secs()))
    }
}

/// `GET /api/freshness`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Freshness {
    pub sources: Vec<SourceFreshness>,
    pub budget: Budget,
    pub dead: Vec<DeadWork>,
    pub scope: ScopeState,
    pub next_poll_at: Option<Timestamp>,
}

impl Freshness {
    pub fn from_status(status: SyncStatus) -> Self {
        let sources = SyncKind::ALL
            .into_iter()
            .map(|source| SourceFreshness {
                source,
                last_ok_at: status.last_ok_by_kind.get(&source).copied(),
                pending: status.pending_by_kind.get(&source).copied().unwrap_or(0),
            })
            .collect();
        Self {
            sources,
            budget: Budget {
                limit: status.rate.limit,
                remaining: status.rate.remaining,
                reset_at: status.rate.reset,
            },
            dead: status.dead,
            scope: ScopeState::parse(status.scopes.as_deref()),
            next_poll_at: status.next_poll_at,
        }
    }
}

/// One kind of sync work: when it last succeeded and how many units wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SourceFreshness {
    pub source: SyncKind,
    pub last_ok_at: Option<Timestamp>,
    pub pending: u32,
}

/// GitHub's REST rate limit as last reported. Each field is empty until
/// GitHub has sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Budget {
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset_at: Option<Timestamp>,
}

/// Whether the token can read notifications (A5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeState {
    Ok,
    MissingNotifications,
    Unknown,
}

impl ScopeState {
    /// Reads an `X-OAuth-Scopes` value. No header is `Unknown`, and only the
    /// exact token `notifications` counts.
    pub fn parse(header: Option<&str>) -> Self {
        match header {
            None => Self::Unknown,
            Some(list) if list.split(',').any(|s| s.trim() == "notifications") => Self::Ok,
            Some(_) => Self::MissingNotifications,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::sync_status::Rate;

    #[test]
    fn scope_needs_the_exact_notifications_token() {
        assert_eq!(
            ScopeState::parse(Some("notifications, read:user")),
            ScopeState::Ok
        );
        assert_eq!(
            ScopeState::parse(Some("read:user,notifications")),
            ScopeState::Ok
        );
        assert_eq!(
            ScopeState::parse(Some("read:user")),
            ScopeState::MissingNotifications
        );
        assert_eq!(
            ScopeState::parse(Some("notifications-extra")),
            ScopeState::MissingNotifications
        );
        assert_eq!(
            ScopeState::parse(Some("")),
            ScopeState::MissingNotifications
        );
        assert_eq!(ScopeState::parse(None), ScopeState::Unknown);
        assert_eq!(
            serde_json::to_string(&ScopeState::MissingNotifications).unwrap(),
            r#""missing_notifications""#
        );
    }

    #[test]
    fn snooze_body_takes_one_day_or_one_week() {
        let day: SnoozeRequest = serde_json::from_str(r#"{"for":"1d"}"#).unwrap();
        assert_eq!(day.snooze_for, SnoozeFor::Day);
        let week: SnoozeRequest = serde_json::from_str(r#"{"for":"1w"}"#).unwrap();
        assert_eq!(week.snooze_for.wake_at(Timestamp(0)), Timestamp(604_800));
        for bad in [
            r#"{"for":"1y"}"#,
            r#"{"for":"1d","x":1}"#,
            r#"{}"#,
            r#"{"for":1}"#,
            "",
        ] {
            assert!(serde_json::from_str::<SnoozeRequest>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn item_id_accepts_node_ids_only() {
        for good in ["PR_kwDOAbc123", "I_kw-DO_9", "MDExOlB1bGxSZXF1ZXN0MQ=="] {
            assert!(ItemId::parse(good).is_ok(), "{good}");
        }
        let long = "x".repeat(129);
        for bad in ["", "a/b", "a b", "..", "PR_1?x=1", long.as_str()] {
            assert!(ItemId::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn empty_pipeline_holds_the_six_open_stages_in_order() {
        let pipeline = Pipeline::empty();
        let stages: Vec<Stage> = pipeline.columns.iter().map(|c| c.stage).collect();
        assert_eq!(stages, Pipeline::STAGES);
        assert_eq!(pipeline.card_count(), 0);
    }

    #[test]
    fn every_source_appears_with_its_last_success_and_queue() {
        let status = SyncStatus {
            pending_by_kind: BTreeMap::from([(SyncKind::Enrich, 2)]),
            last_ok_by_kind: BTreeMap::from([(SyncKind::PollNotifications, Timestamp(900))]),
            next_poll_at: Some(Timestamp(960)),
            rate: Rate {
                limit: Some(5000),
                remaining: Some(4990),
                reset: Some(Timestamp(4000)),
            },
            dead: vec![DeadWork {
                kind: SyncKind::Enrich,
                key: "abc".into(),
                attempts: 3,
                since: Timestamp(800),
            }],
            scopes: Some("notifications,read:user".into()),
            ..SyncStatus::default()
        };
        let out = Freshness::from_status(status);
        let sources: Vec<_> = out
            .sources
            .iter()
            .map(|s| (s.source, s.last_ok_at, s.pending))
            .collect();
        assert_eq!(
            sources,
            [
                (SyncKind::PollNotifications, Some(Timestamp(900)), 0),
                (SyncKind::Enrich, None, 2),
                (SyncKind::BackfillPage, None, 0),
                (SyncKind::Rollup, None, 0),
            ]
        );
        assert_eq!(out.scope, ScopeState::Ok);
        assert_eq!(out.budget.remaining, Some(4990));
        assert_eq!(out.dead[0].attempts, 3);
        assert_eq!(out.next_poll_at, Some(Timestamp(960)));
    }

    #[test]
    fn scope_reads_what_github_last_sent() {
        let with = |scopes: Option<&str>| {
            Freshness::from_status(SyncStatus {
                scopes: scopes.map(str::to_owned),
                ..SyncStatus::default()
            })
            .scope
        };
        assert_eq!(with(None), ScopeState::Unknown);
        assert_eq!(with(Some("read:user")), ScopeState::MissingNotifications);
        assert_eq!(with(Some("notifications")), ScopeState::Ok);
    }
}
