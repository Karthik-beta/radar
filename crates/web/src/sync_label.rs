use domain::api::{Budget, Freshness};
use domain::ids::Timestamp;
use domain::sync_status::SyncKind;

/// A last good notifications poll older than this many seconds is `Quiet`.
const QUIET_AFTER: i64 = 30 * 60;

/// How current Radar's data is, as the header's status button reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncHealth {
    Fresh { age: i64 },
    Quiet { age: i64 },
    Failed { age: Option<i64>, dead: usize },
    Waiting,
    Unknown,
}

impl SyncHealth {
    /// Reads the last good notifications poll and the dead work. Dead rows
    /// win over age, because a failure is what the operator must act on.
    pub fn of(freshness: &Freshness, now: i64) -> Self {
        let age = last_ok(freshness, SyncKind::PollNotifications).map(|at| (now - at.0).max(0));
        let dead = freshness.dead.len();
        match (age, dead) {
            (age, 1..) => Self::Failed { age, dead },
            (Some(age), 0) if age > QUIET_AFTER => Self::Quiet { age },
            (Some(age), 0) => Self::Fresh { age },
            (None, 0) => Self::Waiting,
        }
    }
}

pub fn last_ok(freshness: &Freshness, kind: SyncKind) -> Option<Timestamp> {
    freshness
        .sources
        .iter()
        .find(|s| s.source == kind)
        .and_then(|s| s.last_ok_at)
}

pub fn sync_label(health: SyncHealth) -> String {
    match health {
        SyncHealth::Fresh { age } => format!("Synced {} ago", age_compact(age)),
        SyncHealth::Quiet { age } => format!("Not synced for {}", age_spaced(age)),
        SyncHealth::Failed {
            age: Some(age),
            dead,
        } => format!("Synced {} ago, {dead} failed", age_compact(age)),
        SyncHealth::Failed { age: None, dead } => format!("Not synced yet, {dead} failed"),
        SyncHealth::Waiting => "Waiting for the first sync".to_owned(),
        SyncHealth::Unknown => "Sync status unavailable".to_owned(),
    }
}

/// "40s", "2m", "3h", "4d".
pub fn age_compact(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3_599 => format!("{}m", secs / 60),
        3_600..=86_399 => format!("{}h", secs / 3_600),
        _ => format!("{}d", secs / 86_400),
    }
}

/// "45 min", "3 h", "4 d".
fn age_spaced(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        0..=3_599 => format!("{} min", secs / 60),
        3_600..=86_399 => format!("{} h", secs / 3_600),
        _ => format!("{} d", secs / 86_400),
    }
}

/// When a source last succeeded, for the panel: "40s ago" or "Never".
pub fn source_when(last_ok: Option<Timestamp>, now: i64) -> String {
    match last_ok {
        Some(at) => format!("{} ago", age_compact(now - at.0)),
        None => "Never".to_owned(),
    }
}

pub fn source_name(kind: SyncKind) -> &'static str {
    match kind {
        SyncKind::PollNotifications => "Notifications poll",
        SyncKind::Enrich => "Enrichment",
        SyncKind::BackfillPage => "Search backfill",
        SyncKind::Rollup => "Nightly run",
    }
}

pub fn attempts_words(attempts: u32) -> String {
    if attempts == 1 {
        "1 attempt".to_owned()
    } else {
        format!("{attempts} attempts")
    }
}

/// "250 of 5,000 (5%)", or `None` until GitHub has reported both numbers.
pub fn budget_used(budget: &Budget) -> Option<String> {
    let (limit, remaining) = (budget.limit?, budget.remaining?);
    let used = limit.saturating_sub(remaining);
    let percent = if limit == 0 {
        0
    } else {
        (u64::from(used) * 100 + u64::from(limit) / 2) / u64::from(limit)
    };
    Some(format!(
        "{} of {} ({percent}%)",
        thousands(used),
        thousands(limit)
    ))
}

fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// "14:05", from the local hour and minute.
pub fn clock(hours: u32, minutes: u32) -> String {
    format!("{hours:02}:{minutes:02}")
}

#[cfg(test)]
mod tests {
    use domain::api::{ScopeState, SourceFreshness};
    use domain::sync_status::DeadWork;

    use super::*;

    fn freshness(poll_age: Option<i64>, dead: usize, now: i64) -> Freshness {
        Freshness {
            sources: vec![SourceFreshness {
                source: SyncKind::PollNotifications,
                last_ok_at: poll_age.map(|age| Timestamp(now - age)),
                pending: 0,
            }],
            budget: Budget {
                limit: None,
                remaining: None,
                reset_at: None,
            },
            dead: (0..dead)
                .map(|i| DeadWork {
                    kind: SyncKind::Enrich,
                    key: format!("k{i}"),
                    attempts: 3,
                    since: Timestamp(now),
                })
                .collect(),
            scope: ScopeState::Ok,
            next_poll_at: None,
        }
    }

    fn label(poll_age: Option<i64>, dead: usize) -> String {
        sync_label(SyncHealth::of(&freshness(poll_age, dead, 10_000), 10_000))
    }

    #[test]
    fn sync_label_29_minutes_is_fresh_and_31_is_quiet() {
        assert_eq!(
            SyncHealth::of(&freshness(Some(29 * 60), 0, 10_000), 10_000),
            SyncHealth::Fresh { age: 29 * 60 }
        );
        assert_eq!(label(Some(29 * 60), 0), "Synced 29m ago");
        assert_eq!(
            SyncHealth::of(&freshness(Some(31 * 60), 0, 10_000), 10_000),
            SyncHealth::Quiet { age: 31 * 60 }
        );
        assert_eq!(label(Some(31 * 60), 0), "Not synced for 31 min");
    }

    #[test]
    fn sync_label_reads_seconds_and_dead_rows() {
        assert_eq!(label(Some(40), 0), "Synced 40s ago");
        assert_eq!(label(Some(120), 1), "Synced 2m ago, 1 failed");
        assert_eq!(label(Some(120), 2), "Synced 2m ago, 2 failed");
        assert_eq!(label(Some(31 * 60), 1), "Synced 31m ago, 1 failed");
    }

    #[test]
    fn sync_label_names_an_unsynced_and_an_unavailable_state() {
        assert_eq!(label(None, 0), "Waiting for the first sync");
        assert_eq!(label(None, 1), "Not synced yet, 1 failed");
        assert_eq!(sync_label(SyncHealth::Unknown), "Sync status unavailable");
    }

    #[test]
    fn sync_label_a_clock_ahead_of_the_server_reads_zero() {
        assert_eq!(
            SyncHealth::of(&freshness(Some(-30), 0, 10_000), 10_000),
            SyncHealth::Fresh { age: 0 }
        );
    }

    #[test]
    fn sync_label_ages_round_down_by_unit() {
        assert_eq!(source_when(Some(Timestamp(100)), 100 + 3 * 3_600), "3h ago");
        assert_eq!(
            source_when(Some(Timestamp(100)), 100 + 2 * 86_400),
            "2d ago"
        );
        assert_eq!(source_when(None, 100), "Never");
        assert_eq!(age_spaced(5 * 3_600), "5 h");
    }

    fn budget(limit: u32, remaining: u32) -> Budget {
        Budget {
            limit: Some(limit),
            remaining: Some(remaining),
            reset_at: None,
        }
    }

    #[test]
    fn sync_label_budget_wording_counts_what_is_used() {
        assert_eq!(
            budget_used(&budget(5000, 4750)).as_deref(),
            Some("250 of 5,000 (5%)")
        );
        assert_eq!(
            budget_used(&budget(5000, 0)).as_deref(),
            Some("5,000 of 5,000 (100%)")
        );
        assert_eq!(
            budget_used(&budget(5000, 5000)).as_deref(),
            Some("0 of 5,000 (0%)")
        );
        assert_eq!(
            budget_used(&Budget {
                limit: Some(5000),
                remaining: None,
                reset_at: None
            }),
            None
        );
    }

    #[test]
    fn sync_label_clock_pads_to_two_digits() {
        assert_eq!(clock(14, 5), "14:05");
        assert_eq!(clock(0, 0), "00:00");
    }
}
