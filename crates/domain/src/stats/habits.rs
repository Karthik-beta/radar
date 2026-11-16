//! S8, Habits: when I work, progress toward my monthly goal, and milestones.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::period::Span;
use super::trust::STAR_MERGE_MIN_STARS;
use super::{count, MergeRow, DAY, HOUR};
use crate::ids::{RepoName, Timestamp};

const FIRST_MILESTONES: [(usize, MilestoneKind); 2] = [
    (50, MilestoneKind::Merged50),
    (100, MilestoneKind::Merged100),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Habits {
    /// 7 rows, Monday first, of 24 hourly counts, in my UTC offset.
    pub grid: Vec<Vec<u32>>,
    pub events: u32,
    /// `None` until `settings.goal_merged_per_month` is set.
    pub goal: Option<Goal>,
    pub milestones: Vec<Milestone>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub target: u32,
    pub merged_this_month: u32,
    pub progress: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneKind {
    FirstMergeInRepo,
    Merged50,
    Merged100,
    FirstStarMerge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Milestone {
    pub kind: MilestoneKind,
    pub at: Timestamp,
    pub repo: Option<RepoName>,
}

pub(crate) fn habits(
    events: &[Timestamp],
    merges: &[MergeRow],
    span: Span,
    utc_offset_minutes: i32,
    goal: Option<u32>,
    now: Timestamp,
) -> Habits {
    let offset = i64::from(utc_offset_minutes) * 60;
    let mut grid = vec![vec![0u32; 24]; 7];
    let mut total = 0;
    for at in events.iter().filter(|at| span.contains(**at)) {
        let local = at.0 + offset;
        // 1970-01-01 was a Thursday, which is index 3 with Monday first.
        let day = usize::try_from((local.div_euclid(DAY) + 3).rem_euclid(7)).unwrap_or(0);
        let hour = usize::try_from(local.rem_euclid(DAY) / HOUR).unwrap_or(0);
        if let Some(cell) = grid.get_mut(day).and_then(|row| row.get_mut(hour)) {
            *cell += 1;
            total += 1;
        }
    }

    let external: Vec<&MergeRow> = merges.iter().filter(|m| m.external).collect();
    let this_month = now.month_start();
    let goal = goal.filter(|target| *target > 0).map(|target| {
        let merged = count(
            external
                .iter()
                .filter(|m| m.at >= this_month && m.at <= now)
                .count(),
        );
        Goal {
            target,
            merged_this_month: merged,
            progress: f64::from(merged) / f64::from(target),
        }
    });

    let mut milestones = Vec::new();
    let mut seen = BTreeSet::new();
    let mut star_done = false;
    for (i, merge) in external.iter().enumerate() {
        let mut hit = |kind, repo: Option<&RepoName>| {
            if span.contains(merge.at) {
                milestones.push(Milestone {
                    kind,
                    at: merge.at,
                    repo: repo.cloned(),
                });
            }
        };
        if seen.insert(merge.repo.to_string()) {
            hit(MilestoneKind::FirstMergeInRepo, Some(&merge.repo));
        }
        if !star_done && merge.stars.is_some_and(|s| s >= STAR_MERGE_MIN_STARS) {
            star_done = true;
            hit(MilestoneKind::FirstStarMerge, Some(&merge.repo));
        }
        for (nth, kind) in FIRST_MILESTONES {
            if i + 1 == nth {
                hit(kind, None);
            }
        }
    }
    Habits {
        grid,
        events: total,
        goal,
        milestones,
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;

    const NOW: &str = "2026-11-16T12:00:00Z";

    fn span() -> Span {
        Span {
            from: at("2026-01-01T00:00:00Z"),
            to: Timestamp(at(NOW).0 + 1),
        }
    }

    fn merge(name: &str, when: &str, stars: Option<u32>) -> MergeRow {
        MergeRow {
            repo: repo(name),
            external: true,
            at: at(when),
            stars,
        }
    }

    fn run(events: &[Timestamp], merges: &[MergeRow], offset: i32, goal: Option<u32>) -> Habits {
        habits(events, merges, span(), offset, goal, at(NOW))
    }

    #[test]
    fn no_events_gives_an_empty_grid_and_no_goal() {
        let got = run(&[], &[], 0, None);
        assert_eq!((got.grid.len(), got.grid[0].len()), (7, 24));
        assert_eq!(got.events, 0);
        assert!(got.goal.is_none() && got.milestones.is_empty());
    }

    #[test]
    fn an_event_at_2330_utc_moves_to_the_next_days_column_with_a_60_minute_offset() {
        // 2026-11-09 is a Monday.
        let event = [at("2026-11-09T23:30:00Z")];
        let utc = run(&event, &[], 0, None);
        assert_eq!(utc.grid[0][23], 1);
        let shifted = run(&event, &[], 60, None);
        assert_eq!(shifted.grid[0][23], 0);
        assert_eq!(shifted.grid[1][0], 1);
        let west = run(&event, &[], -60, None);
        assert_eq!(west.grid[0][22], 1);
    }

    #[test]
    fn the_grid_sums_to_the_events_inside_the_span() {
        let events = [
            at("2026-03-01T10:00:00Z"),
            at("2026-03-02T10:00:00Z"),
            at("2025-12-31T23:00:00Z"),
        ];
        let got = run(&events, &[], 330, None);
        assert_eq!(got.events, 2);
        assert_eq!(got.grid.iter().flatten().sum::<u32>(), 2);
    }

    #[test]
    fn the_goal_appears_only_when_set_and_counts_this_month() {
        let merges = [
            merge("a/x", "2026-11-02T00:00:00Z", None),
            merge("a/x", "2026-10-31T23:59:59Z", None),
        ];
        assert!(run(&[], &merges, 0, None).goal.is_none());
        let goal = run(&[], &merges, 0, Some(4)).goal.expect("goal");
        assert_eq!(
            (goal.target, goal.merged_this_month, goal.progress),
            (4, 1, 0.25)
        );
    }

    #[test]
    fn milestones_mark_new_repos_the_50th_and_100th_and_the_first_star_merge() {
        let mut merges = vec![merge("old/repo", "2025-01-01T00:00:00Z", None)];
        for i in 0..98 {
            merges.push(merge(
                "a/x",
                &format!("2026-02-{:02}T00:00:00Z", 1 + i % 28),
                Some(10),
            ));
        }
        merges.push(merge("big/lib", "2026-05-01T00:00:00Z", Some(1_000)));
        merges.push(merge("c/y", "2026-06-01T00:00:00Z", None));
        let got = run(&[], &merges, 0, None);
        let kinds: Vec<_> = got.milestones.iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds
                .iter()
                .filter(|k| **k == MilestoneKind::FirstMergeInRepo)
                .count(),
            3
        );
        assert!(kinds.contains(&MilestoneKind::Merged50));
        assert!(kinds.contains(&MilestoneKind::Merged100));
        assert!(kinds.contains(&MilestoneKind::FirstStarMerge));
        let new_repos: Vec<_> = got
            .milestones
            .iter()
            .filter(|m| m.kind == MilestoneKind::FirstMergeInRepo)
            .filter_map(|m| m.repo.as_ref().map(ToString::to_string))
            .collect();
        assert!(!new_repos.contains(&"old/repo".to_owned()));
    }

    #[test]
    fn an_own_repo_never_makes_a_milestone() {
        let mut own = merge("me/mine", "2026-03-01T00:00:00Z", Some(5_000));
        own.external = false;
        assert!(run(&[], &[own], 0, Some(1)).milestones.is_empty());
    }
}
