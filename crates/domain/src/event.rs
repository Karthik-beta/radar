use serde::{Deserialize, Serialize};

use crate::activity::ReviewState;
use crate::ids::{Login, RepoName, Timestamp};

/// The primary key of an event. A re-read of the same timeline item builds the
/// same id, so it writes nothing (N8).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiOutcome {
    Passed,
    Failed,
}

impl EventId {
    /// A timeline item already has a GitHub node id.
    pub fn for_node(node_id: &str) -> Self {
        Self(node_id.to_owned())
    }

    /// An item's creation is not a timeline node, so its id names the item.
    pub fn for_open(item_node_id: &str) -> Self {
        Self(format!("open:{item_node_id}"))
    }

    /// A mention is a second event on a comment that already has its own, so
    /// its id names the comment.
    pub fn for_mention(comment_node_id: &str) -> Self {
        Self(format!("mention:{comment_node_id}"))
    }

    /// A CI transition has no node of its own, so its id names the item, the
    /// commit, and the outcome. The `ci:` prefix cannot collide with a node id.
    pub fn for_ci(item_node_id: &str, commit_oid: &str, outcome: CiOutcome) -> Self {
        let word = match outcome {
            CiOutcome::Passed => "passed",
            CiOutcome::Failed => "failed",
        };
        Self(format!("ci:{item_node_id}:{commit_oid}:{word}"))
    }

    /// An id read back from the `events` table.
    pub fn from_stored(id: &str) -> Self {
        Self(id.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `Reviewed` carries its state, so `review_state` cannot be set on any other kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Opened,
    Commented,
    Reviewed(ReviewState),
    ReviewRequested,
    Assigned,
    Mentioned,
    Committed,
    Merged,
    Closed,
    Reopened,
    CiFailed,
    CiPassed,
}

impl EventKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Commented => "commented",
            Self::Reviewed(_) => "reviewed",
            Self::ReviewRequested => "review_requested",
            Self::Assigned => "assigned",
            Self::Mentioned => "mentioned",
            Self::Committed => "committed",
            Self::Merged => "merged",
            Self::Closed => "closed",
            Self::Reopened => "reopened",
            Self::CiFailed => "ci_failed",
            Self::CiPassed => "ci_passed",
        }
    }

    pub fn review_state(self) -> Option<ReviewState> {
        match self {
            Self::Reviewed(state) => Some(state),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventActor {
    pub login: Login,
    pub is_me: bool,
    pub is_bot: bool,
}

/// One thing that happened on an item, as R20 to R22 and R24 read it. CI
/// transitions have no actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub id: EventId,
    /// The GitHub node id of the pull request or issue.
    pub item: String,
    pub repo: RepoName,
    pub kind: EventKind,
    pub actor: Option<EventActor>,
    pub occurred_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_of_different_origins_never_collide() {
        let ids = [
            EventId::for_node("IC_1"),
            EventId::for_open("PR_1"),
            EventId::for_mention("IC_1"),
            EventId::for_ci("PR_1", "abc", CiOutcome::Failed),
            EventId::for_ci("PR_1", "abc", CiOutcome::Passed),
            EventId::for_ci("PR_1", "def", CiOutcome::Failed),
        ];
        for (i, a) in ids.iter().enumerate() {
            for b in &ids[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn only_a_review_has_a_review_state() {
        assert_eq!(
            EventKind::Reviewed(ReviewState::Approved).review_state(),
            Some(ReviewState::Approved)
        );
        assert_eq!(EventKind::Commented.review_state(), None);
    }

    #[test]
    fn kind_serializes_with_the_review_state_inside() {
        let json = serde_json::to_string(&EventKind::Reviewed(ReviewState::Approved)).unwrap();
        assert_eq!(json, r#"{"reviewed":"approved"}"#);
        assert_eq!(
            serde_json::to_string(&EventKind::CiFailed).unwrap(),
            r#""ci_failed""#
        );
    }
}
