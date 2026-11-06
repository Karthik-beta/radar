use serde::{Deserialize, Serialize};

use crate::activity::Activity;
use crate::ids::Timestamp;
use crate::stage::Stage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Turn {
    Me(Reason),
    Them,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    ReviewRequested,
    ChangesRequested,
    CiFailed,
    Mentioned,
    Assigned,
    Replied,
}

impl Reason {
    /// When several triggers are outstanding, the first one here is shown.
    pub const PRECEDENCE: [Reason; 6] = [
        Reason::ChangesRequested,
        Reason::CiFailed,
        Reason::ReviewRequested,
        Reason::Mentioned,
        Reason::Assigned,
        Reason::Replied,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Classification {
    pub turn: Turn,
    pub since: Timestamp,
    pub stage: Option<Stage>,
    pub last_human: Option<Activity>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_lists_every_reason_once() {
        for reason in Reason::PRECEDENCE {
            let rank = match reason {
                Reason::ReviewRequested
                | Reason::ChangesRequested
                | Reason::CiFailed
                | Reason::Mentioned
                | Reason::Assigned
                | Reason::Replied => Reason::PRECEDENCE.iter().filter(|r| **r == reason),
            };
            assert_eq!(rank.count(), 1, "{reason:?}");
        }
    }

    #[test]
    fn turn_serializes_as_me_with_reason_or_them() {
        let me = serde_json::to_string(&Turn::Me(Reason::CiFailed)).unwrap();
        assert_eq!(me, r#"{"me":"ci_failed"}"#);
        assert_eq!(serde_json::to_string(&Turn::Them).unwrap(), r#""them""#);
    }
}
