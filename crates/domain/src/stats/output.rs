//! S5, Output: what I opened and merged, from the same `build_trends` rows the
//! Trends page used, so the two agree.

use serde::{Deserialize, Serialize};

use crate::ids::Timestamp;
use crate::trends::{DayCell, Trends, WeekBucket};

const ROLLING_WEEKS: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub weeks: Vec<WeekPoint>,
    pub months: Vec<MonthPoint>,
    pub best_week: Option<Best>,
    pub best_month: Option<Best>,
    pub current_streak: u32,
    pub longest_streak: u32,
    pub heatmap: Vec<DayCell>,
    pub merged_total: u32,
    pub repos_total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeekPoint {
    pub start: Timestamp,
    pub opened: u32,
    pub merged: u32,
    pub repos: u32,
    /// The mean of merged over this week and the three before it.
    pub rolling_merged: f64,
}

/// Weeks summed into the calendar month their Monday falls in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthPoint {
    pub start: Timestamp,
    pub opened: u32,
    pub merged: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Best {
    pub start: Timestamp,
    pub merged: u32,
}

pub(crate) fn output(trends: &Trends) -> Output {
    let weeks: Vec<WeekPoint> = trends
        .weeks
        .iter()
        .enumerate()
        .map(|(i, week)| {
            let from = (i + 1).saturating_sub(ROLLING_WEEKS);
            let window = trends.weeks.get(from..=i).unwrap_or_default();
            let sum: u32 = window.iter().map(|w| w.merged).sum();
            WeekPoint {
                start: week.week_start,
                opened: week.opened,
                merged: week.merged,
                repos: week.repos,
                rolling_merged: f64::from(sum) / window.len().max(1) as f64,
            }
        })
        .collect();
    let mut months: Vec<MonthPoint> = Vec::new();
    for week in &trends.weeks {
        let start = week.week_start.month_start();
        match months.last_mut().filter(|m| m.start == start) {
            Some(month) => {
                month.opened += week.opened;
                month.merged += week.merged;
            }
            None => months.push(MonthPoint {
                start,
                opened: week.opened,
                merged: week.merged,
            }),
        }
    }
    let (current_streak, longest_streak) = streaks(&trends.weeks);
    Output {
        best_week: best(weeks.iter().map(|w| (w.start, w.merged))),
        best_month: best(months.iter().map(|m| (m.start, m.merged))),
        weeks,
        months,
        current_streak,
        longest_streak,
        heatmap: trends.heatmap.clone(),
        merged_total: trends.merged_total,
        repos_total: trends.repos_total,
    }
}

/// The largest count, the latest on a tie, or `None` when nothing merged.
fn best(points: impl Iterator<Item = (Timestamp, u32)>) -> Option<Best> {
    points
        .filter(|(_, merged)| *merged > 0)
        .max_by_key(|(start, merged)| (*merged, *start))
        .map(|(start, merged)| Best { start, merged })
}

/// Runs of weeks with a merge. The current week does not break the current
/// streak while it has no merge yet, because it is not over.
fn streaks(weeks: &[WeekBucket]) -> (u32, u32) {
    let (mut longest, mut run) = (0, 0);
    for week in weeks {
        run = if week.merged > 0 { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let finished = match weeks.split_last() {
        Some((last, rest)) if last.merged == 0 => rest,
        _ => weeks,
    };
    let current = finished.iter().rev().take_while(|w| w.merged > 0).count();
    (super::count(current), longest)
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;
    use crate::trends::{build_trends, PrFact};

    const NOW: &str = "2026-10-14T12:00:00Z";

    fn trends_with_merges(merged_at: &[&str]) -> Trends {
        let prs: Vec<PrFact> = merged_at
            .iter()
            .map(|when| PrFact {
                opened_at: at(when),
                merged_at: Some(at(when)),
            })
            .collect();
        build_trends(&prs, &[], &[], at(NOW))
    }

    #[test]
    fn no_activity_has_no_best_and_no_streak() {
        let got = output(&trends_with_merges(&[]));
        assert_eq!(got.weeks.len(), 52);
        assert_eq!((got.best_week, got.best_month), (None, None));
        assert_eq!((got.current_streak, got.longest_streak), (0, 0));
        assert!(got.weeks.iter().all(|w| w.rolling_merged == 0.0));
    }

    #[test]
    fn weeks_match_trends_and_the_rolling_mean_covers_four_weeks() {
        let trends = trends_with_merges(&["2026-10-13T00:00:00Z", "2026-10-06T00:00:00Z"]);
        let got = output(&trends);
        for (week, bucket) in got.weeks.iter().zip(&trends.weeks) {
            assert_eq!(
                (week.start, week.opened, week.merged, week.repos),
                (
                    bucket.week_start,
                    bucket.opened,
                    bucket.merged,
                    bucket.repos
                )
            );
        }
        assert_eq!(got.weeks[51].rolling_merged, 0.5);
        assert_eq!(got.weeks[0].rolling_merged, 0.0);
    }

    #[test]
    fn a_sunday_and_a_monday_are_different_weeks_and_the_best_week_is_the_busiest() {
        let trends = trends_with_merges(&[
            "2026-10-11T23:59:59Z",
            "2026-10-12T00:00:00Z",
            "2026-10-13T00:00:00Z",
        ]);
        let got = output(&trends);
        assert_eq!(got.weeks[50].merged, 1);
        assert_eq!(got.weeks[51].merged, 2);
        assert_eq!(
            got.best_week,
            Some(Best {
                start: at("2026-10-12T00:00:00Z"),
                merged: 2
            })
        );
    }

    #[test]
    fn months_sum_the_weeks_that_start_in_them_and_the_best_month_is_found() {
        let trends = trends_with_merges(&[
            "2026-09-08T00:00:00Z",
            "2026-09-15T00:00:00Z",
            "2026-10-13T00:00:00Z",
        ]);
        let got = output(&trends);
        let sep = got
            .months
            .iter()
            .find(|m| m.start == at("2026-09-01T00:00:00Z"))
            .expect("september");
        assert_eq!(sep.merged, 2);
        assert_eq!(
            got.best_month.map(|b| b.start),
            Some(at("2026-09-01T00:00:00Z"))
        );
    }

    #[test]
    fn the_current_week_without_a_merge_does_not_break_the_streak() {
        let trends = trends_with_merges(&["2026-09-29T00:00:00Z", "2026-10-06T00:00:00Z"]);
        let got = output(&trends);
        assert_eq!(got.weeks[51].merged, 0);
        assert_eq!((got.current_streak, got.longest_streak), (2, 2));
    }

    #[test]
    fn a_gap_ends_the_current_streak_and_the_longest_survives() {
        let trends = trends_with_merges(&[
            "2026-08-04T00:00:00Z",
            "2026-08-11T00:00:00Z",
            "2026-08-18T00:00:00Z",
            "2026-10-13T00:00:00Z",
        ]);
        let got = output(&trends);
        assert_eq!((got.current_streak, got.longest_streak), (1, 3));
    }
}
