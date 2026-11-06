use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Draft,
    CiRunning,
    CiFailing,
    AwaitingReview,
    ChangesRequested,
    Approved,
    Merged,
    Closed,
}

impl Stage {
    pub const ALL: [Stage; 8] = [
        Stage::Draft,
        Stage::CiRunning,
        Stage::CiFailing,
        Stage::AwaitingReview,
        Stage::ChangesRequested,
        Stage::Approved,
        Stage::Merged,
        Stage::Closed,
    ];
}
