//! The period a Stats request asks for and the buckets it draws.

use serde::{Deserialize, Serialize};

use super::DAY;
use crate::ids::Timestamp;
use crate::rollup::WeekStart;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grain {
    Week,
    Month,
}

/// The last 12 calendar months, or this year against last year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "grain", rename_all = "snake_case")]
pub enum Period {
    Last12Months(Grain),
    YearOverYear,
}

/// A half-open range of instants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: Timestamp,
    pub to: Timestamp,
}

impl Span {
    pub fn contains(self, at: Timestamp) -> bool {
        self.from <= at && at < self.to
    }
}

impl Period {
    /// Reads `period=12m` or `period=yoy` and an optional `grain=week|month`.
    /// `None` is any other value.
    pub fn parse(period: &str, grain: Option<&str>) -> Option<Self> {
        let grain = match grain {
            None => None,
            Some("week") => Some(Grain::Week),
            Some("month") => Some(Grain::Month),
            Some(_) => return None,
        };
        match period {
            "12m" => Some(Self::Last12Months(grain.unwrap_or(Grain::Week))),
            "yoy" => Some(Self::YearOverYear),
            _ => None,
        }
    }

    fn grain(self) -> Grain {
        match self {
            Self::Last12Months(grain) => grain,
            Self::YearOverYear => Grain::Month,
        }
    }

    /// From the first instant of the period up to and including `now`.
    pub fn span(self, now: Timestamp) -> Span {
        let from = match self {
            Self::Last12Months(_) => add_months(now.month_start(), -11),
            Self::YearOverYear => year_start(now, 0),
        };
        Span {
            from,
            to: Timestamp(now.0 + 1),
        }
    }

    /// The earliest instant any section reads: the start of the first drawn
    /// bucket, or of last year for year over year.
    pub fn fetch_from(self, now: Timestamp) -> Timestamp {
        match self {
            Self::YearOverYear => year_start(now, -1),
            Self::Last12Months(Grain::Week) => self
                .bucket_starts(now)
                .first()
                .copied()
                .unwrap_or_else(|| self.span(now).from),
            Self::Last12Months(Grain::Month) => self.span(now).from,
        }
    }

    /// The start of each bucket the reviews chart draws, oldest first.
    pub fn bucket_starts(self, now: Timestamp) -> Vec<Timestamp> {
        let from = self.span(now).from;
        match self.grain() {
            Grain::Week => {
                let mut starts = Vec::new();
                let mut start = WeekStart::containing(from).timestamp();
                while start <= now {
                    starts.push(start);
                    start = Timestamp(start.0 + 7 * DAY);
                }
                starts
            }
            Grain::Month => self.month_starts(now),
        }
    }

    /// The start of each calendar month the period reaches, oldest first.
    pub fn month_starts(self, now: Timestamp) -> Vec<Timestamp> {
        let mut starts = Vec::new();
        let mut start = self.span(now).from;
        while start <= now {
            starts.push(start);
            start = add_months(start, 1);
        }
        starts
    }
}

/// January 1 of this year (`years` 0) or of an earlier year (negative `years`),
/// at 00:00 UTC.
pub(crate) fn year_start(now: Timestamp, years: i64) -> Timestamp {
    let (year, _) = now.year_month();
    Timestamp::from_ymd(year + years, 1, 1)
}

/// `start` is a month start. Moves it by whole calendar months.
pub(crate) fn add_months(start: Timestamp, months: i64) -> Timestamp {
    let (year, month) = start.year_month();
    let index = year * 12 + (month - 1) + months;
    Timestamp::from_ymd(index.div_euclid(12), index.rem_euclid(12) + 1, 1)
}

/// The bucket of `at`, or `None` when it falls before the first.
pub(crate) fn bucket_index(starts: &[Timestamp], at: Timestamp) -> Option<usize> {
    starts.partition_point(|start| *start <= at).checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::at;
    use super::*;

    #[test]
    fn only_the_documented_values_parse() {
        assert_eq!(
            Period::parse("12m", None),
            Some(Period::Last12Months(Grain::Week))
        );
        assert_eq!(
            Period::parse("12m", Some("month")),
            Some(Period::Last12Months(Grain::Month))
        );
        assert_eq!(Period::parse("yoy", None), Some(Period::YearOverYear));
        for (period, grain) in [
            ("all", None),
            ("", None),
            ("12m", Some("day")),
            ("yoy", Some("year")),
            ("12M", None),
        ] {
            assert_eq!(Period::parse(period, grain), None, "{period} {grain:?}");
        }
    }

    #[test]
    fn twelve_months_start_eleven_months_back_and_cross_a_year() {
        let now = at("2026-03-15T12:00:00Z");
        let span = Period::Last12Months(Grain::Month).span(now);
        assert_eq!(span.from, at("2025-04-01T00:00:00Z"));
        let months = Period::Last12Months(Grain::Month).month_starts(now);
        assert_eq!(months.len(), 12);
        assert_eq!(months[11], at("2026-03-01T00:00:00Z"));
        assert_eq!(months[9], at("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn weekly_buckets_start_on_a_monday_and_reach_this_week() {
        let now = at("2026-11-16T00:00:00Z");
        let starts = Period::Last12Months(Grain::Week).bucket_starts(now);
        assert_eq!(starts[0], at("2025-12-01T00:00:00Z"));
        assert_eq!(starts.last().copied(), Some(at("2026-11-16T00:00:00Z")));
        assert!(starts
            .iter()
            .all(|s| WeekStart::containing(*s).timestamp() == *s));
        assert_eq!(Period::Last12Months(Grain::Week).fetch_from(now), starts[0]);
    }

    #[test]
    fn year_over_year_runs_from_january_and_fetches_last_year() {
        let now = at("2026-11-16T12:00:00Z");
        assert_eq!(
            Period::YearOverYear.span(now).from,
            at("2026-01-01T00:00:00Z")
        );
        assert_eq!(
            Period::YearOverYear.fetch_from(now),
            at("2025-01-01T00:00:00Z")
        );
        assert_eq!(Period::YearOverYear.month_starts(now).len(), 11);
    }

    #[test]
    fn a_bucket_holds_its_first_instant_and_not_the_next() {
        let starts = [at("2026-01-01T00:00:00Z"), at("2026-02-01T00:00:00Z")];
        assert_eq!(bucket_index(&starts, at("2025-12-31T23:59:59Z")), None);
        assert_eq!(bucket_index(&starts, at("2026-01-01T00:00:00Z")), Some(0));
        assert_eq!(bucket_index(&starts, at("2026-01-31T23:59:59Z")), Some(0));
        assert_eq!(bucket_index(&starts, at("2026-02-01T00:00:00Z")), Some(1));
    }
}
