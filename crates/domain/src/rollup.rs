//! The weekly rollup of old events (N6). Events older than 180 days fold into
//! `rollups_weekly`, one whole week at a time.

use std::fmt;

use crate::ids::Timestamp;

const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_WEEK: i64 = 7 * SECONDS_PER_DAY;
/// 1970-01-05, the first Monday after the epoch.
const FIRST_MONDAY: i64 = 345_600;

/// How long an event stays in `events` before its week rolls up.
pub const KEEP_DAYS: i64 = 180;

/// A timestamp that is not a Monday 00:00 UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotMonday(pub i64);

impl fmt::Display for NotMonday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} is not a Monday 00:00 UTC", self.0)
    }
}

impl std::error::Error for NotMonday {}

/// The Monday 00:00 UTC that starts a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WeekStart(i64);

impl WeekStart {
    pub fn new(start: Timestamp) -> Result<Self, NotMonday> {
        if start.0.rem_euclid(SECONDS_PER_WEEK) == FIRST_MONDAY {
            Ok(Self(start.0))
        } else {
            Err(NotMonday(start.0))
        }
    }

    /// The week that contains `at`.
    pub fn containing(at: Timestamp) -> Self {
        let since_monday = (at.0 - FIRST_MONDAY).rem_euclid(SECONDS_PER_WEEK);
        Self(at.0 - since_monday)
    }

    /// Reads the `YYYY-MM-DD` of a Monday, as a `week:` work key spells it.
    pub fn parse_date(date: &str) -> Option<Self> {
        let at = Timestamp::parse_rfc3339(&format!("{date}T00:00:00Z")).ok()?;
        Self::new(at).ok()
    }

    pub fn timestamp(self) -> Timestamp {
        Timestamp(self.0)
    }

    /// The first instant of the next week.
    pub fn end(self) -> Timestamp {
        Timestamp(self.0 + SECONDS_PER_WEEK)
    }

    /// The `YYYY-MM-DD` of the Monday.
    pub fn date(self) -> String {
        self.timestamp().to_string().chars().take(10).collect()
    }
}

/// The Monday of the week that contains the instant 180 days before `now`.
/// Events from this week on stay.
pub fn rollup_cutoff(now: Timestamp) -> WeekStart {
    WeekStart::containing(Timestamp(now.0 - KEEP_DAYS * SECONDS_PER_DAY))
}

/// Whether `week` rolls up now. A week rolls only when it ends at or before
/// the cutoff, so one run never sees half of a week.
pub fn rolls_up(week: WeekStart, now: Timestamp) -> bool {
    week.end() <= rollup_cutoff(now).timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Timestamp {
        Timestamp::parse_rfc3339(s).expect("timestamp")
    }

    fn week_of_event(now: Timestamp, age_days: i64) -> WeekStart {
        WeekStart::containing(Timestamp(now.0 - age_days * SECONDS_PER_DAY))
    }

    #[test]
    fn cutoff_is_the_monday_of_the_week_180_days_back() {
        // 180 days before Monday 2026-11-09 is Wednesday 2026-05-13.
        assert_eq!(
            rollup_cutoff(at("2026-11-09T00:00:00Z")).date(),
            "2026-05-11"
        );
        // 180 days before Sunday 2026-11-08 is Tuesday 2026-05-12.
        assert_eq!(
            rollup_cutoff(at("2026-11-08T23:59:59Z")).date(),
            "2026-05-11"
        );
    }

    #[test]
    fn the_cutoff_week_splits_day_180_from_day_181() {
        // 180 days before Saturday 2026-11-07 is Monday 2026-05-11, midnight.
        let now = at("2026-11-07T00:00:00Z");
        assert_eq!(rollup_cutoff(now).date(), "2026-05-11");
        for (age, rolls) in [(179, false), (180, false), (181, true)] {
            assert_eq!(rolls_up(week_of_event(now, age), now), rolls, "age {age}");
        }
        // One second earlier, the same Monday still starts the cutoff week.
        let before = at("2026-11-06T23:59:59Z");
        assert_eq!(rollup_cutoff(before).date(), "2026-05-04");
        assert!(!rolls_up(week_of_event(before, 180), before));
    }

    #[test]
    fn cutoff_crosses_a_year_boundary() {
        // 180 days before 2026-06-30 is Thursday 2026-01-01.
        assert_eq!(
            rollup_cutoff(at("2026-06-30T10:00:00Z")).date(),
            "2025-12-29"
        );
        // 180 days before 2024-06-28 is Sunday 2023-12-31.
        assert_eq!(
            rollup_cutoff(at("2024-06-28T00:00:00Z")).date(),
            "2023-12-25"
        );
    }

    #[test]
    fn only_weeks_that_end_before_the_cutoff_roll() {
        let now = at("2026-11-09T00:00:00Z");
        let week = |d: &str| WeekStart::parse_date(d).expect("monday");
        assert!(rolls_up(week("2026-05-04"), now));
        assert!(rolls_up(week("2026-04-27"), now));
        assert!(!rolls_up(week("2026-05-11"), now));
        assert!(!rolls_up(week("2026-11-02"), now));
        let a_week_later = at("2026-11-16T00:00:00Z");
        assert!(rolls_up(week("2026-05-11"), a_week_later));
    }

    #[test]
    fn week_start_must_be_a_monday_midnight_utc() {
        assert!(WeekStart::new(at("2026-11-02T00:00:00Z")).is_ok());
        for bad in [
            "2026-11-02T00:00:01Z",
            "2026-11-03T00:00:00Z",
            "2026-11-01T00:00:00Z",
        ] {
            let ts = at(bad);
            assert_eq!(WeekStart::new(ts), Err(NotMonday(ts.0)));
        }
    }

    #[test]
    fn containing_snaps_back_to_monday() {
        let monday = at("2026-11-02T00:00:00Z");
        assert_eq!(
            WeekStart::containing(at("2026-11-08T23:59:59Z")).timestamp(),
            monday
        );
        assert_eq!(
            WeekStart::containing(at("2026-11-09T00:00:00Z")).timestamp(),
            at("2026-11-09T00:00:00Z")
        );
        assert_eq!(
            WeekStart::containing(Timestamp(0)).timestamp(),
            Timestamp(-259_200)
        );
    }

    #[test]
    fn a_week_key_round_trips() {
        let week = WeekStart::parse_date("2026-05-04").expect("monday");
        assert_eq!(week.date(), "2026-05-04");
        assert_eq!(WeekStart::parse_date("2026-05-05"), None);
        assert_eq!(WeekStart::parse_date("nightly"), None);
    }
}
