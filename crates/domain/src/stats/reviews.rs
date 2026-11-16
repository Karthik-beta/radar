//! S2, Reviews: the reviews I give, by state and over time, and where.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::period::{bucket_index, Span};
use super::{count, top, ReviewRow};
use crate::activity::ReviewState;
use crate::ids::{RepoName, Timestamp};

const TOP_REPOS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reviews {
    /// External reviews that approved, asked for changes, or commented.
    pub total: u32,
    pub approved: u32,
    pub changes_requested: u32,
    pub commented: u32,
    pub buckets: Vec<ReviewBucket>,
    pub repos_reviewed: u32,
    pub top_repos: Vec<RepoCount>,
    /// Every review in the span, in any state, in repos I do not own.
    pub external: u32,
    /// The same in repos I own. This is the one number that counts own repos.
    pub own: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewBucket {
    pub start: Timestamp,
    pub approved: u32,
    pub changes_requested: u32,
    pub commented: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoCount {
    pub repo: RepoName,
    pub count: u32,
}

/// Dismissed and pending reviews are not counted as reviews I gave.
pub(crate) fn counts(state: ReviewState) -> bool {
    matches!(
        state,
        ReviewState::Approved | ReviewState::ChangesRequested | ReviewState::Commented
    )
}

pub(crate) fn reviews(rows: &[ReviewRow], span: Span, starts: &[Timestamp]) -> Reviews {
    let mut buckets: Vec<ReviewBucket> = starts
        .iter()
        .map(|start| ReviewBucket {
            start: *start,
            approved: 0,
            changes_requested: 0,
            commented: 0,
        })
        .collect();
    let (mut external, mut own) = (0, 0);
    let (mut approved, mut changes, mut commented) = (0, 0, 0);
    let mut repos: BTreeMap<String, (RepoName, u32)> = BTreeMap::new();
    for row in rows.iter().filter(|row| span.contains(row.at)) {
        if !row.external {
            own += 1;
            continue;
        }
        external += 1;
        let bucket = bucket_index(starts, row.at).and_then(|i| buckets.get_mut(i));
        match (row.state, bucket) {
            (ReviewState::Approved, bucket) => {
                approved += 1;
                if let Some(b) = bucket {
                    b.approved += 1;
                }
            }
            (ReviewState::ChangesRequested, bucket) => {
                changes += 1;
                if let Some(b) = bucket {
                    b.changes_requested += 1;
                }
            }
            (ReviewState::Commented, bucket) => {
                commented += 1;
                if let Some(b) = bucket {
                    b.commented += 1;
                }
            }
            _ => continue,
        }
        repos
            .entry(row.repo.to_string())
            .or_insert_with(|| (row.repo.clone(), 0))
            .1 += 1;
    }
    let all: Vec<RepoCount> = repos
        .into_values()
        .map(|(repo, count)| RepoCount { repo, count })
        .collect();
    let repos_reviewed = count(all.len());
    let top_repos = top(all, TOP_REPOS, |a, b| {
        a.count > b.count || (a.count == b.count && a.repo.to_string() < b.repo.to_string())
    });
    Reviews {
        total: approved + changes + commented,
        approved,
        changes_requested: changes,
        commented,
        buckets,
        repos_reviewed,
        top_repos,
        external,
        own,
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;

    fn review(repo_name: &str, external: bool, state: ReviewState, when: &str) -> ReviewRow {
        ReviewRow {
            repo: repo(repo_name),
            external,
            state,
            at: at(when),
        }
    }

    fn span() -> Span {
        Span {
            from: at("2026-01-01T00:00:00Z"),
            to: at("2026-12-01T00:00:00Z"),
        }
    }

    fn months() -> Vec<Timestamp> {
        vec![at("2026-01-01T00:00:00Z"), at("2026-02-01T00:00:00Z")]
    }

    #[test]
    fn nothing_reviewed_is_all_zero_with_empty_buckets() {
        let got = reviews(&[], span(), &months());
        assert_eq!((got.total, got.external, got.own), (0, 0, 0));
        assert_eq!(got.buckets.len(), 2);
        assert!(got.top_repos.is_empty());
    }

    #[test]
    fn one_review_lands_in_its_state_bucket_and_repo() {
        let rows = [review(
            "a/x",
            true,
            ReviewState::Approved,
            "2026-02-01T00:00:00Z",
        )];
        let got = reviews(&rows, span(), &months());
        assert_eq!((got.total, got.approved), (1, 1));
        assert_eq!(got.buckets[1].approved, 1);
        assert_eq!(got.buckets[0].approved, 0);
        assert_eq!((got.repos_reviewed, got.top_repos[0].count), (1, 1));
    }

    #[test]
    fn a_month_boundary_splits_buckets_and_the_span_edge_is_exclusive() {
        let rows = [
            review("a/x", true, ReviewState::Commented, "2026-01-31T23:59:59Z"),
            review("a/x", true, ReviewState::Commented, "2026-02-01T00:00:00Z"),
            review("a/x", true, ReviewState::Commented, "2026-12-01T00:00:00Z"),
            review("a/x", true, ReviewState::Commented, "2025-12-31T23:59:59Z"),
        ];
        let got = reviews(&rows, span(), &months());
        assert_eq!(got.commented, 2);
        assert_eq!((got.buckets[0].commented, got.buckets[1].commented), (1, 1));
    }

    #[test]
    fn own_repos_stay_on_their_own_side_of_the_comparison() {
        let rows = [
            review("a/x", true, ReviewState::Approved, "2026-01-05T00:00:00Z"),
            review("a/x", true, ReviewState::Dismissed, "2026-01-05T00:00:00Z"),
            review(
                "me/mine",
                false,
                ReviewState::Approved,
                "2026-01-05T00:00:00Z",
            ),
            review(
                "me/mine",
                false,
                ReviewState::ChangesRequested,
                "2026-01-05T00:00:00Z",
            ),
        ];
        let got = reviews(&rows, span(), &months());
        assert_eq!((got.external, got.own), (2, 2));
        assert_eq!((got.total, got.approved, got.changes_requested), (1, 1, 0));
        assert_eq!(got.repos_reviewed, 1);
        assert!(got
            .top_repos
            .iter()
            .all(|r| r.repo.to_string() != "me/mine"));
    }

    #[test]
    fn repos_rank_by_count() {
        let rows = [
            review("a/x", true, ReviewState::Approved, "2026-01-05T00:00:00Z"),
            review("b/y", true, ReviewState::Approved, "2026-01-05T00:00:00Z"),
            review("b/y", true, ReviewState::Commented, "2026-01-06T00:00:00Z"),
        ];
        let got = reviews(&rows, span(), &months());
        assert_eq!(got.top_repos[0].repo.to_string(), "b/y");
        assert_eq!(got.repos_reviewed, 2);
    }
}
