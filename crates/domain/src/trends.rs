//! Contribution trends (R22): 52 weekly buckets and a 26 week day heatmap,
//! built from live rows and the weekly rollups.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::{RepoName, Timestamp};
use crate::rollup::WeekStart;

const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_WEEK: i64 = 7 * SECONDS_PER_DAY;

pub const WEEKS: usize = 52;
pub const HEATMAP_WEEKS: usize = 26;
const HEATMAP_DAYS: usize = HEATMAP_WEEKS * 7;
const MIN_CEILING: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeekSource {
    Live,
    Rolled,
    Seeded,
}

impl WeekSource {
    pub fn parse(tag: &str) -> Option<Self> {
        match tag {
            "rolled" => Some(Self::Rolled),
            "seeded" => Some(Self::Seeded),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeatLevel {
    None,
    Low,
    Mid,
    High,
}

/// One week, starting Monday 00:00 UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeekBucket {
    pub week_start: Timestamp,
    pub opened: u32,
    pub merged: u32,
    pub repos: u32,
    pub source: WeekSource,
}

/// One day of the heatmap, at 00:00 UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayCell {
    pub date: Timestamp,
    pub count: u32,
    pub level: HeatLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trends {
    pub weeks: Vec<WeekBucket>,
    pub heatmap: Vec<DayCell>,
    pub merged_total: u32,
    pub repos_total: u32,
}

/// A pull request I authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrFact {
    pub opened_at: Timestamp,
    pub merged_at: Option<Timestamp>,
}

/// An event whose actor is me.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MyEvent {
    pub repo: RepoName,
    pub at: Timestamp,
}

/// A `rollups_weekly` row. `source` is `Rolled` or `Seeded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RolledWeek {
    pub week: WeekStart,
    pub opened: u32,
    pub merged: u32,
    pub repos: u32,
    pub source: WeekSource,
}

/// The first instant the weekly chart reads, 51 weeks before the current one.
pub fn window_start(now: Timestamp) -> Timestamp {
    Timestamp(WeekStart::containing(now).timestamp().0 - (WEEKS as i64 - 1) * SECONDS_PER_WEEK)
}

pub fn heat_level(count: u32, max: u32) -> HeatLevel {
    let (count, max) = (u64::from(count), u64::from(max));
    if count == 0 {
        HeatLevel::None
    } else if 3 * count <= max {
        HeatLevel::Low
    } else if 3 * count <= 2 * max {
        HeatLevel::Mid
    } else {
        HeatLevel::High
    }
}

/// Rounds a chart ceiling up to 1, 2, 5, or 10 times a power of ten, at least 4.
pub fn nice_max(n: u32) -> u32 {
    if n <= MIN_CEILING {
        return MIN_CEILING;
    }
    let mut power: u32 = 1;
    loop {
        for step in [1u32, 2, 5] {
            let ceiling = step.saturating_mul(power);
            if n <= ceiling {
                return ceiling;
            }
        }
        match power.checked_mul(10) {
            Some(next) => power = next,
            None => return u32::MAX,
        }
    }
}

#[derive(Default)]
struct LiveWeek {
    opened: u32,
    merged: u32,
    repos: BTreeSet<String>,
}

pub fn build_trends(
    prs: &[PrFact],
    events: &[MyEvent],
    rollups: &[RolledWeek],
    now: Timestamp,
) -> Trends {
    let first = window_start(now).0;
    let week_index = |at: Timestamp| {
        let offset = at.0.checked_sub(first).filter(|o| *o >= 0)?;
        usize::try_from(offset / SECONDS_PER_WEEK)
            .ok()
            .filter(|i| *i < WEEKS)
    };
    let mut live: Vec<LiveWeek> = (0..WEEKS).map(|_| LiveWeek::default()).collect();
    for pr in prs {
        if let Some(week) = week_index(pr.opened_at).and_then(|i| live.get_mut(i)) {
            week.opened += 1;
        }
        if let Some(week) = pr
            .merged_at
            .and_then(week_index)
            .and_then(|i| live.get_mut(i))
        {
            week.merged += 1;
        }
    }
    for event in events {
        if let Some(week) = week_index(event.at).and_then(|i| live.get_mut(i)) {
            week.repos.insert(event.repo.to_string());
        }
    }
    let rolled: BTreeMap<WeekStart, &RolledWeek> =
        rollups.iter().map(|row| (row.week, row)).collect();
    let weeks: Vec<WeekBucket> = live
        .iter()
        .enumerate()
        .map(|(i, week)| {
            let start = Timestamp(first + i as i64 * SECONDS_PER_WEEK);
            match rolled.get(&WeekStart::containing(start)) {
                Some(row) => WeekBucket {
                    week_start: start,
                    opened: row.opened,
                    merged: row.merged,
                    repos: row.repos,
                    source: row.source,
                },
                None => WeekBucket {
                    week_start: start,
                    opened: week.opened,
                    merged: week.merged,
                    repos: u32::try_from(week.repos.len()).unwrap_or(u32::MAX),
                    source: WeekSource::Live,
                },
            }
        })
        .collect();

    let heat_start = first + (WEEKS - HEATMAP_WEEKS) as i64 * SECONDS_PER_WEEK;
    let mut counts = [0u32; HEATMAP_DAYS];
    let mut heat_repos = BTreeSet::new();
    for event in events {
        let day = event
            .at
            .0
            .checked_sub(heat_start)
            .filter(|o| *o >= 0)
            .and_then(|o| usize::try_from(o / SECONDS_PER_DAY).ok())
            .and_then(|i| counts.get_mut(i));
        if let Some(count) = day {
            *count += 1;
            heat_repos.insert(event.repo.to_string());
        }
    }
    let max = counts.iter().copied().max().unwrap_or(0);
    let heatmap = counts
        .iter()
        .enumerate()
        .map(|(i, count)| DayCell {
            date: Timestamp(heat_start + i as i64 * SECONDS_PER_DAY),
            count: *count,
            level: heat_level(*count, max),
        })
        .collect();
    let merged_total = weeks
        .iter()
        .skip(WEEKS - HEATMAP_WEEKS)
        .map(|week| week.merged)
        .sum();
    Trends {
        weeks,
        heatmap,
        merged_total,
        repos_total: u32::try_from(heat_repos.len()).unwrap_or(u32::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Timestamp {
        Timestamp::parse_rfc3339(s).expect("timestamp")
    }

    fn week(date: &str) -> WeekStart {
        WeekStart::parse_date(date).expect("monday")
    }

    fn event(repo: &str, when: &str) -> MyEvent {
        MyEvent {
            repo: RepoName::parse(repo).expect("repo"),
            at: at(when),
        }
    }

    fn pr(opened: &str, merged: Option<&str>) -> PrFact {
        PrFact {
            opened_at: at(opened),
            merged_at: merged.map(at),
        }
    }

    const NOW: &str = "2026-10-14T12:00:00Z";

    #[test]
    fn no_data_gives_52_monday_weeks_of_zeros() {
        let trends = build_trends(&[], &[], &[], at(NOW));
        assert_eq!(trends.weeks.len(), 52);
        assert_eq!(trends.heatmap.len(), 182);
        for (i, w) in trends.weeks.iter().enumerate() {
            assert_eq!(
                WeekStart::containing(w.week_start).timestamp(),
                w.week_start
            );
            assert_eq!((w.opened, w.merged, w.repos), (0, 0, 0));
            assert_eq!(w.source, WeekSource::Live);
            if i > 0 {
                assert_eq!(w.week_start.0 - trends.weeks[i - 1].week_start.0, 604_800);
            }
        }
        assert_eq!(trends.weeks[51].week_start, at("2026-10-12T00:00:00Z"));
        assert_eq!(trends.weeks[0].week_start, at("2025-10-20T00:00:00Z"));
        assert_eq!((trends.merged_total, trends.repos_total), (0, 0));
        assert!(trends.heatmap.iter().all(|c| c.level == HeatLevel::None));
    }

    #[test]
    fn a_rolled_week_wins_over_live_rows_and_a_seeded_week_is_marked() {
        let prs = [pr("2026-10-13T08:00:00Z", Some("2026-10-14T08:00:00Z"))];
        let rolled = RolledWeek {
            week: week("2026-10-12"),
            opened: 7,
            merged: 5,
            repos: 3,
            source: WeekSource::Rolled,
        };
        let seeded = RolledWeek {
            week: week("2025-11-03"),
            opened: 2,
            merged: 1,
            repos: 1,
            source: WeekSource::Seeded,
        };
        let trends = build_trends(&prs, &[], &[rolled, seeded], at(NOW));
        let last = trends.weeks[51];
        assert_eq!((last.opened, last.merged, last.repos), (7, 5, 3));
        assert_eq!(last.source, WeekSource::Rolled);
        let old = trends.weeks[2];
        assert_eq!(old.week_start, at("2025-11-03T00:00:00Z"));
        assert_eq!((old.opened, old.source), (2, WeekSource::Seeded));
        assert_eq!(trends.weeks[50].source, WeekSource::Live);
    }

    #[test]
    fn distinct_repos_count_once_per_week() {
        let one = [
            event("o/a", "2026-10-12T01:00:00Z"),
            event("o/a", "2026-10-13T01:00:00Z"),
        ];
        assert_eq!(build_trends(&[], &one, &[], at(NOW)).weeks[51].repos, 1);
        let two = [one[0].clone(), event("o/b", "2026-10-14T01:00:00Z")];
        let trends = build_trends(&[], &two, &[], at(NOW));
        assert_eq!(trends.weeks[51].repos, 2);
        assert_eq!(trends.repos_total, 2);
    }

    #[test]
    fn sunday_and_monday_land_in_different_weeks() {
        let prs = [
            pr("2026-10-11T23:59:59Z", Some("2026-10-11T23:59:59Z")),
            pr("2026-10-12T00:00:00Z", None),
        ];
        let trends = build_trends(&prs, &[], &[], at(NOW));
        assert_eq!((trends.weeks[50].opened, trends.weeks[50].merged), (1, 1));
        assert_eq!((trends.weeks[51].opened, trends.weeks[51].merged), (1, 0));
    }

    #[test]
    fn the_week_that_spans_new_year_is_placed_once() {
        let now = at("2027-01-05T00:00:00Z");
        let prs = [
            pr("2026-12-31T10:00:00Z", None),
            pr("2027-01-01T10:00:00Z", None),
        ];
        let trends = build_trends(&prs, &[], &[], now);
        let across = trends
            .weeks
            .iter()
            .find(|w| w.week_start == at("2026-12-28T00:00:00Z"))
            .expect("week");
        assert_eq!(across.opened, 2);
        assert_eq!(trends.weeks.iter().map(|w| w.opened).sum::<u32>(), 2);
    }

    #[test]
    fn pull_requests_before_the_window_are_ignored() {
        let prs = [pr("2020-01-01T00:00:00Z", Some("2020-01-02T00:00:00Z"))];
        let trends = build_trends(&prs, &[], &[], at(NOW));
        assert!(trends.weeks.iter().all(|w| w.opened == 0 && w.merged == 0));
    }

    #[test]
    fn the_heatmap_ends_in_todays_column_and_counts_events_per_day() {
        let events = [
            event("o/a", "2026-10-14T01:00:00Z"),
            event("o/a", "2026-10-14T02:00:00Z"),
            event("o/b", "2026-10-13T02:00:00Z"),
        ];
        let trends = build_trends(&[], &events, &[], at(NOW));
        let wednesday = &trends.heatmap[25 * 7 + 2];
        assert_eq!(wednesday.date, at("2026-10-14T00:00:00Z"));
        assert_eq!((wednesday.count, wednesday.level), (2, HeatLevel::High));
        assert_eq!(trends.heatmap[25 * 7 + 1].level, HeatLevel::Mid);
        assert_eq!(trends.heatmap[0].date, at("2026-04-20T00:00:00Z"));
    }

    #[test]
    fn heat_levels_split_at_thirds() {
        let levels: Vec<_> = [0, 1, 3, 4, 6, 9]
            .into_iter()
            .map(|n| heat_level(n, 9))
            .collect();
        assert_eq!(
            levels,
            [
                HeatLevel::None,
                HeatLevel::Low,
                HeatLevel::Low,
                HeatLevel::Mid,
                HeatLevel::Mid,
                HeatLevel::High
            ]
        );
    }

    #[test]
    fn nice_max_rounds_up_to_1_2_5() {
        let got: Vec<_> = [0, 5, 7, 11, 4, 21, 100, 101]
            .into_iter()
            .map(nice_max)
            .collect();
        assert_eq!(got, [4, 5, 10, 20, 4, 50, 100, 200]);
        assert_eq!(nice_max(u32::MAX), u32::MAX);
    }
}
