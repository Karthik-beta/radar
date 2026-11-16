//! S3, Trust: where maintainers take my work.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::period::Span;
use super::{count, hours, percentile, top, MergeRow, Outcomes, PrRow};
use crate::ids::RepoName;

pub const STAR_MERGE_MIN_STARS: u32 = 1_000;
const TOP_REPOS: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trust {
    /// Repos whose first merged pull request ever falls in the period.
    pub new_repos: u32,
    /// Repos with 2 or more merged pull requests in the period.
    pub repeat_repos: u32,
    pub ranking: Vec<RepoRank>,
    /// Merged pull requests into repos with 1,000 or more stars.
    pub star_merges: u32,
    pub languages: Vec<LanguageCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoRank {
    pub repo: RepoName,
    pub merged: u32,
    pub merge_rate: Option<f64>,
    pub median_first_response_hours: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageCount {
    pub language: String,
    pub merged: u32,
}

#[derive(Default)]
struct RepoTally {
    merged: u32,
    outcomes: Outcomes,
    responses: Vec<f64>,
}

pub(crate) fn trust(prs: &[PrRow], merges: &[MergeRow], span: Span) -> Trust {
    let mut first_merge: BTreeMap<String, crate::ids::Timestamp> = BTreeMap::new();
    for merge in merges.iter().filter(|m| m.external) {
        let slot = first_merge
            .entry(merge.repo.to_string())
            .or_insert(merge.at);
        *slot = (*slot).min(merge.at);
    }
    let new_repos = first_merge
        .values()
        .filter(|at| span.contains(**at))
        .count();

    let mut repos: BTreeMap<String, (RepoName, RepoTally)> = BTreeMap::new();
    let mut languages: BTreeMap<&str, u32> = BTreeMap::new();
    let mut star_merges = 0;
    for pr in prs.iter().filter(|pr| pr.external) {
        let tally = &mut repos
            .entry(pr.repo.to_string())
            .or_insert_with(|| (pr.repo.clone(), RepoTally::default()))
            .1;
        if span.contains(pr.created_at) {
            tally.outcomes.add(pr);
            if let Some(response) = pr.first_response_at {
                tally.responses.push(hours(pr.created_at, response));
            }
        }
        if pr.merged_at.is_some_and(|at| span.contains(at)) {
            tally.merged += 1;
            star_merges += u32::from(pr.stars.is_some_and(|s| s >= STAR_MERGE_MIN_STARS));
            if let Some(language) = &pr.language {
                *languages.entry(language).or_default() += 1;
            }
        }
    }
    let repeat_repos = repos.values().filter(|(_, t)| t.merged >= 2).count();
    let ranking: Vec<RepoRank> = repos
        .into_values()
        .filter(|(_, tally)| tally.merged > 0)
        .map(|(repo, tally)| RepoRank {
            repo,
            merged: tally.merged,
            merge_rate: tally.outcomes.merge_rate(),
            median_first_response_hours: percentile(&tally.responses, 50),
        })
        .collect();
    let ranking = top(ranking, TOP_REPOS, outranks);
    let languages: Vec<LanguageCount> = languages
        .into_iter()
        .map(|(language, merged)| LanguageCount {
            language: language.to_owned(),
            merged,
        })
        .collect();
    let languages = top(languages, usize::MAX, |a, b| {
        a.merged > b.merged || (a.merged == b.merged && a.language < b.language)
    });
    Trust {
        new_repos: count(new_repos),
        repeat_repos: count(repeat_repos),
        ranking,
        star_merges,
        languages,
    }
}

/// Whether `a` leads `b` on one optional measure, or `None` on a tie. Any value
/// beats none.
fn lead(a: Option<f64>, b: Option<f64>, high_wins: bool) -> Option<bool> {
    match (a, b) {
        (Some(a), Some(b)) if a != b => Some((a > b) == high_wins),
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        _ => None,
    }
}

fn outranks(a: &RepoRank, b: &RepoRank) -> bool {
    if a.merged != b.merged {
        return a.merged > b.merged;
    }
    lead(a.merge_rate, b.merge_rate, true)
        .or_else(|| {
            lead(
                a.median_first_response_hours,
                b.median_first_response_hours,
                false,
            )
        })
        .unwrap_or_else(|| a.repo.to_string() < b.repo.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;
    use crate::ids::Timestamp;

    fn span() -> Span {
        Span {
            from: at("2026-01-01T00:00:00Z"),
            to: Timestamp(at("2026-11-16T12:00:00Z").0 + 1),
        }
    }

    fn in_repo(name: &str, mut pr: PrRow) -> PrRow {
        pr.repo = repo(name);
        pr
    }

    fn merge(name: &str, when: &str) -> MergeRow {
        MergeRow {
            repo: repo(name),
            external: true,
            at: at(when),
            stars: None,
        }
    }

    #[test]
    fn nothing_merged_gives_zeros_and_no_ranking() {
        let got = trust(&[], &[], span());
        assert_eq!(
            (got.new_repos, got.repeat_repos, got.star_merges),
            (0, 0, 0)
        );
        assert!(got.ranking.is_empty() && got.languages.is_empty());
    }

    #[test]
    fn a_repo_is_new_only_when_its_first_merge_ever_is_in_the_span() {
        let merges = [
            merge("a/old", "2025-06-01T00:00:00Z"),
            merge("a/old", "2026-02-01T00:00:00Z"),
            merge("b/new", "2026-03-01T00:00:00Z"),
        ];
        let got = trust(&[], &merges, span());
        assert_eq!(got.new_repos, 1);
    }

    #[test]
    fn repeat_repos_have_two_or_more_merges_and_the_ranking_sorts() {
        let busy = |n| in_repo("a/busy", merged("2026-03-01T00:00:00Z", n));
        let prs = [
            busy("2026-03-02T00:00:00Z"),
            busy("2026-03-03T00:00:00Z"),
            in_repo(
                "b/once",
                merged("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z"),
            ),
            in_repo(
                "c/once",
                merged("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z"),
            ),
            in_repo(
                "c/once",
                closed("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z"),
            ),
        ];
        let got = trust(&prs, &[], span());
        assert_eq!(got.repeat_repos, 1);
        let order: Vec<_> = got.ranking.iter().map(|r| r.repo.to_string()).collect();
        assert_eq!(order, ["a/busy", "b/once", "c/once"]);
        assert_eq!(got.ranking[2].merge_rate, Some(0.5));
    }

    #[test]
    fn first_response_medians_break_ties() {
        let mut slow = in_repo(
            "a/slow",
            merged("2026-03-01T00:00:00Z", "2026-03-05T00:00:00Z"),
        );
        slow.first_response_at = Some(at("2026-03-03T00:00:00Z"));
        let mut fast = in_repo(
            "b/fast",
            merged("2026-03-01T00:00:00Z", "2026-03-05T00:00:00Z"),
        );
        fast.first_response_at = Some(at("2026-03-01T06:00:00Z"));
        let got = trust(&[slow, fast], &[], span());
        assert_eq!(got.ranking[0].repo.to_string(), "b/fast");
        assert_eq!(got.ranking[0].median_first_response_hours, Some(6.0));
    }

    #[test]
    fn star_merges_and_languages_count_merged_prs_only_in_external_repos() {
        let mut big = merged("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z");
        big.stars = Some(1_000);
        big.language = Some("Rust".into());
        let mut small = merged("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z");
        small.stars = Some(999);
        small.language = Some("Go".into());
        let mut open = pr("2026-03-01T00:00:00Z");
        open.stars = Some(5_000);
        open.language = Some("Rust".into());
        let mut own = merged("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z");
        own.external = false;
        own.stars = Some(5_000);
        own.language = Some("Zig".into());
        let got = trust(&[big, small, open, own], &[], span());
        assert_eq!(got.star_merges, 1);
        let langs: Vec<_> = got
            .languages
            .iter()
            .map(|l| (l.language.as_str(), l.merged))
            .collect();
        assert_eq!(langs, [("Go", 1), ("Rust", 1)]);
        assert!(got.ranking.iter().all(|r| r.merged > 0));
    }

    #[test]
    fn an_own_repo_never_counts_as_new() {
        let mut own = merge("me/mine", "2026-03-01T00:00:00Z");
        own.external = false;
        assert_eq!(trust(&[], &[own], span()).new_repos, 0);
    }
}
