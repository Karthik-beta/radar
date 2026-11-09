use std::fmt;

use domain::activity::{ActorKind, ReviewState};
use domain::ids::{IdError, ItemKind, Timestamp};
use domain::item::{CiState, ItemState, ReviewDecision};
use domain::stage::Stage;
use domain::settings::SettingsFallback;
use domain::turn::{Reason, Turn};

#[derive(Debug, Clone, PartialEq)]
pub enum RowError {
    UnknownTag {
        column: &'static str,
        value: String,
    },
    BadTurn {
        turn: String,
        reason: Option<String>,
    },
    BadId {
        column: &'static str,
        source: IdError,
    },
    BadNumber {
        column: &'static str,
        value: i64,
    },
    NotMonday(i64),
    NotFinite(&'static str),
    PartialLastHuman,
    BadReviewers,
    Settings(SettingsFallback),
}

impl fmt::Display for RowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTag { column, value } => write!(f, "{column}: unknown value `{value}`"),
            Self::BadTurn { turn, reason } => {
                write!(f, "turn `{turn}` cannot have reason {reason:?}")
            }
            Self::BadId { column, source } => write!(f, "{column}: {source}"),
            Self::BadNumber { column, value } => write!(f, "{column}: {value} is out of range"),
            Self::NotMonday(ts) => write!(f, "week_start {ts} is not a Monday 00:00 UTC"),
            Self::NotFinite(column) => write!(f, "{column}: not a finite number"),
            Self::PartialLastHuman => f.write_str("last_human columns are only partly set"),
            Self::BadReviewers => f.write_str("requested_reviewers is not a JSON array of logins"),
            Self::Settings(fallback) => write!(f, "{fallback}"),
        }
    }
}

impl std::error::Error for RowError {}

/// The stored word for an enum column. The `match` is exhaustive, so a new
/// domain variant stops the build until the column learns it.
pub trait Tagged: Copy + 'static {
    const VARIANTS: &'static [Self];
    fn tag(self) -> &'static str;
    fn from_tag(tag: &str) -> Option<Self>;
}

macro_rules! tagged {
    ($ty:ty { $($variant:path => $tag:literal),+ $(,)? }) => {
        impl $crate::db::rows::Tagged for $ty {
            const VARIANTS: &'static [Self] = &[$($variant),+];

            fn tag(self) -> &'static str {
                match self {
                    $($variant => $tag),+
                }
            }

            fn from_tag(tag: &str) -> Option<Self> {
                match tag {
                    $($tag => Some($variant),)+
                    _ => None,
                }
            }
        }
    };
}
pub(crate) use tagged;

pub fn untag<T: Tagged>(column: &'static str, value: &str) -> Result<T, RowError> {
    T::from_tag(value).ok_or_else(|| RowError::UnknownTag {
        column,
        value: value.to_owned(),
    })
}

pub fn untag_opt<T: Tagged>(
    column: &'static str,
    value: Option<&str>,
) -> Result<Option<T>, RowError> {
    value.map(|v| untag(column, v)).transpose()
}

tagged!(ItemKind {
    ItemKind::Pr => "pr",
    ItemKind::Issue => "issue",
});

tagged!(ItemState {
    ItemState::Open => "open",
    ItemState::Draft => "draft",
    ItemState::Merged => "merged",
    ItemState::Closed => "closed",
});

tagged!(ActorKind {
    ActorKind::User => "user",
    ActorKind::Bot => "bot",
});

tagged!(CiState {
    CiState::Pending => "pending",
    CiState::Passing => "passing",
    CiState::Failing => "failing",
});

tagged!(ReviewDecision {
    ReviewDecision::Approved => "approved",
    ReviewDecision::ChangesRequested => "changes_requested",
    ReviewDecision::ReviewRequired => "review_required",
});

tagged!(ReviewState {
    ReviewState::Approved => "approved",
    ReviewState::ChangesRequested => "changes_requested",
    ReviewState::Commented => "commented",
    ReviewState::Dismissed => "dismissed",
    ReviewState::Pending => "pending",
});

tagged!(Reason {
    Reason::ReviewRequested => "review_requested",
    Reason::ChangesRequested => "changes_requested",
    Reason::CiFailed => "ci_failed",
    Reason::Mentioned => "mentioned",
    Reason::Assigned => "assigned",
    Reason::Replied => "replied",
});

tagged!(Stage {
    Stage::Draft => "draft",
    Stage::CiRunning => "ci_running",
    Stage::CiFailing => "ci_failing",
    Stage::AwaitingReview => "awaiting_review",
    Stage::ChangesRequested => "changes_requested",
    Stage::Approved => "approved",
    Stage::Merged => "merged",
    Stage::Closed => "closed",
});

/// The `turn` and `reason` columns as stored. Converting to `Turn` is the only
/// way to read them, so a pair the `CHECK` would refuse never becomes a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnColumns {
    pub turn: String,
    pub reason: Option<String>,
}

impl From<Turn> for TurnColumns {
    fn from(turn: Turn) -> Self {
        match turn {
            Turn::Me(reason) => Self {
                turn: "me".to_owned(),
                reason: Some(reason.tag().to_owned()),
            },
            Turn::Them => Self {
                turn: "them".to_owned(),
                reason: None,
            },
        }
    }
}

impl TryFrom<TurnColumns> for Turn {
    type Error = RowError;

    fn try_from(cols: TurnColumns) -> Result<Self, RowError> {
        match (cols.turn.as_str(), cols.reason.as_deref()) {
            ("me", Some(reason)) => Ok(Turn::Me(untag("reason", reason)?)),
            ("them", None) => Ok(Turn::Them),
            _ => Err(RowError::BadTurn {
                turn: cols.turn,
                reason: cols.reason,
            }),
        }
    }
}

pub fn flag(column: &'static str, value: i64) -> Result<bool, RowError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RowError::BadNumber { column, value }),
    }
}

pub fn count(column: &'static str, value: i64) -> Result<u32, RowError> {
    u32::try_from(value).map_err(|_| RowError::BadNumber { column, value })
}

pub fn ts(value: i64) -> Timestamp {
    Timestamp(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serde_tag<T: serde::Serialize>(value: &T) -> String {
        serde_json::to_value(value)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    fn assert_matches_serde<T: Tagged + serde::Serialize>() {
        for variant in T::VARIANTS {
            assert_eq!(variant.tag(), serde_tag(variant));
            assert!(T::from_tag(variant.tag()).is_some());
        }
        assert!(T::from_tag("bogus").is_none());
    }

    #[test]
    fn stored_words_match_the_domain_serde_names() {
        assert_matches_serde::<ItemKind>();
        assert_matches_serde::<ItemState>();
        assert_matches_serde::<ActorKind>();
        assert_matches_serde::<CiState>();
        assert_matches_serde::<ReviewDecision>();
        assert_matches_serde::<ReviewState>();
        assert_matches_serde::<Reason>();
        assert_matches_serde::<Stage>();
    }

    #[test]
    fn turn_columns_round_trip_every_turn() {
        let turns = Reason::PRECEDENCE
            .into_iter()
            .map(Turn::Me)
            .chain([Turn::Them]);
        for turn in turns {
            assert_eq!(Turn::try_from(TurnColumns::from(turn)), Ok(turn));
        }
    }

    #[test]
    fn invalid_turn_pairs_never_become_a_turn() {
        let bad = [
            ("me", None),
            ("them", Some("ci_failed")),
            ("me", Some("bogus")),
            ("someone", None),
        ];
        for (turn, reason) in bad {
            let cols = TurnColumns {
                turn: turn.to_owned(),
                reason: reason.map(str::to_owned),
            };
            assert!(Turn::try_from(cols).is_err(), "{turn} {reason:?}");
        }
    }
}
