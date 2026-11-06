use serde::{Deserialize, Serialize};

use crate::ids::{Login, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Actor {
    pub login: Login,
    pub kind: ActorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    User,
    Bot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Activity {
    pub at: Timestamp,
    pub actor: Actor,
    pub kind: ActivityKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActivityKind {
    /// R10 sets `mentions_me` from the notification reasons `mention` and
    /// `team_mention` (F1), on the newest human comment after my last
    /// activity. Radar never scans a comment body (N5).
    Comment {
        mentions_me: bool,
    },
    Review {
        state: ReviewState,
    },
    ReviewRequested {
        reviewer: Login,
    },
    Assigned {
        assignee: Login,
    },
    Pushed,
}

impl ActivityKind {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Comment { .. } => "comment",
            Self::Review { .. } => "review",
            Self::ReviewRequested { .. } => "review_requested",
            Self::Assigned { .. } => "assigned",
            Self::Pushed => "pushed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_matches_the_serialized_type() {
        let login = Login::parse("octocat").unwrap();
        let kinds = [
            ActivityKind::Comment { mentions_me: true },
            ActivityKind::Review {
                state: ReviewState::Approved,
            },
            ActivityKind::ReviewRequested {
                reviewer: login.clone(),
            },
            ActivityKind::Assigned { assignee: login },
            ActivityKind::Pushed,
        ];
        for kind in kinds {
            let json = serde_json::to_value(&kind).unwrap();
            assert_eq!(json["type"], kind.tag());
        }
    }
}
