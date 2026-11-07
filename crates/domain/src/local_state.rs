use crate::ids::Timestamp;

/// What I did to an item inside Radar. Radar never writes it back to GitHub (F9).
/// An item with no local state is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalState {
    /// Hidden until a human acts after `seen_human_at`. It is `Timestamp(0)`
    /// when the item had no human activity.
    Dismissed { seen_human_at: Timestamp },
    /// Hidden until `until`.
    Snoozed { until: Timestamp },
}

impl LocalState {
    /// The `kind` and `value` columns of `local_state`.
    pub fn from_columns(kind: &str, value: i64) -> Option<Self> {
        match kind {
            "dismissed" => Some(Self::Dismissed {
                seen_human_at: Timestamp(value),
            }),
            "snoozed" => Some(Self::Snoozed {
                until: Timestamp(value),
            }),
            _ => None,
        }
    }

    pub fn columns(self) -> (&'static str, i64) {
        match self {
            Self::Dismissed { seen_human_at } => ("dismissed", seen_human_at.0),
            Self::Snoozed { until } => ("snoozed", until.0),
        }
    }
}

/// Whether Your turn shows an item. A newer human activity brings a dismissed
/// item back, and a snoozed item returns at its wake time.
pub fn visible(
    local: Option<LocalState>,
    last_human_at: Option<Timestamp>,
    now: Timestamp,
) -> bool {
    match local {
        None => true,
        Some(LocalState::Dismissed { seen_human_at }) => {
            last_human_at.unwrap_or(Timestamp(0)) > seen_human_at
        }
        Some(LocalState::Snoozed { until }) => until <= now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Timestamp = Timestamp(10_000);

    fn dismissed(at: i64) -> Option<LocalState> {
        Some(LocalState::Dismissed {
            seen_human_at: Timestamp(at),
        })
    }

    #[test]
    fn an_item_without_local_state_is_visible() {
        assert!(visible(None, None, NOW));
        assert!(visible(None, Some(Timestamp(5)), NOW));
    }

    #[test]
    fn a_newer_human_activity_revives_a_dismissed_item() {
        assert!(visible(dismissed(500), Some(Timestamp(501)), NOW));
    }

    #[test]
    fn a_dismissed_item_with_the_same_activity_stays_hidden() {
        assert!(!visible(dismissed(500), Some(Timestamp(500)), NOW));
        assert!(!visible(dismissed(500), Some(Timestamp(400)), NOW));
    }

    #[test]
    fn a_moved_since_does_not_revive_a_dismissed_item() {
        // `since` is not an input: only `last_human_at` can bring the item back.
        let before = visible(dismissed(500), Some(Timestamp(500)), Timestamp(1_000));
        let after = visible(dismissed(500), Some(Timestamp(500)), Timestamp(9_000));
        assert!(!before && !after);
    }

    #[test]
    fn dismissing_an_item_with_no_human_activity_hides_it_until_one_arrives() {
        assert!(!visible(dismissed(0), None, NOW));
        assert!(visible(dismissed(0), Some(Timestamp(1)), NOW));
    }

    #[test]
    fn a_snoozed_item_is_hidden_until_its_wake_time() {
        let snoozed = Some(LocalState::Snoozed {
            until: Timestamp(1_000),
        });
        assert!(!visible(snoozed, None, Timestamp(999)));
        assert!(visible(snoozed, None, Timestamp(1_000)));
        assert!(visible(snoozed, Some(Timestamp(5)), Timestamp(5_000)));
    }

    #[test]
    fn a_state_round_trips_through_its_columns() {
        for state in [
            LocalState::Dismissed {
                seen_human_at: Timestamp(7),
            },
            LocalState::Snoozed {
                until: Timestamp(9),
            },
        ] {
            let (kind, value) = state.columns();
            assert_eq!(LocalState::from_columns(kind, value), Some(state));
        }
        assert_eq!(LocalState::from_columns("archived", 1), None);
    }
}
