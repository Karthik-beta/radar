use serde::{Deserialize, Serialize};

use crate::activity::{Activity, Actor};
use crate::ids::{ItemRef, Login, Timestamp};

/// One pull request or issue, as metadata. It has no read state, because
/// reading a thread on github.com is not handling it (ADR 0002).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Item {
    pub item_ref: ItemRef,
    pub author: Actor,
    pub state: ItemState,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub closed_at: Option<Timestamp>,
    pub review_decision: Option<ReviewDecision>,
    pub review_requests: Vec<Login>,
    pub assignees: Vec<Login>,
    pub ci: Option<Ci>,
    pub activity: Vec<Activity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    Open,
    Draft,
    Merged,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Ci {
    pub state: CiState,
    pub at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiState {
    Pending,
    Passing,
    Failing,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{ActivityKind, ActorKind};
    use crate::ids::{ItemKind, RepoName};

    fn keys(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, inner) in map {
                    out.push(key.clone());
                    keys(inner, out);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| keys(v, out)),
            _ => {}
        }
    }

    #[test]
    fn item_has_no_read_state() {
        let login = Login::parse("karthik-beta").unwrap();
        let actor = Actor {
            login: login.clone(),
            kind: ActorKind::User,
        };
        let item = Item {
            item_ref: ItemRef {
                repo: RepoName::parse("karthik-beta/radar").unwrap(),
                number: 1,
                kind: ItemKind::Pr,
            },
            author: actor.clone(),
            state: ItemState::Open,
            created_at: Timestamp(0),
            updated_at: Timestamp(1),
            closed_at: None,
            review_decision: Some(ReviewDecision::ReviewRequired),
            review_requests: vec![login.clone()],
            assignees: vec![login],
            ci: Some(Ci {
                state: CiState::Passing,
                at: Timestamp(1),
            }),
            activity: vec![Activity {
                at: Timestamp(1),
                actor,
                kind: ActivityKind::Comment { mentions_me: true },
            }],
        };
        let mut found = Vec::new();
        keys(&serde_json::to_value(&item).unwrap(), &mut found);
        assert!(found.contains(&"item_ref".to_owned()));
        for key in &found {
            assert!(!key.contains("read"), "unexpected read-state key `{key}`");
        }
    }
}
