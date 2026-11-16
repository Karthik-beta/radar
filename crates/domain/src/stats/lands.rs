//! S1, Lands: whether my pull requests get merged, and how fast.

use serde::{Deserialize, Serialize};

use super::period::{add_months, Span};
use super::{share, Outcomes, PrRow, Summary, DAY};
use crate::ids::Timestamp;

const STALE_DAYS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lands {
    pub summary: Summary,
    /// Open pull requests older than 30 days, whenever they opened.
    pub open_over_30_days: u32,
    pub ci_failed_share: Option<f64>,
    pub months: Vec<MonthCohort>,
}

/// Of the pull requests opened in a month, how many merged within 30 days.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonthCohort {
    pub start: Timestamp,
    pub opened: u32,
    pub merged_within_30_days: u32,
    pub share: Option<f64>,
    /// A pull request opened on the last day has not had 30 days yet.
    pub unfinished: bool,
}

/// The external pull requests opened in a span.
#[derive(Debug, Default)]
pub(crate) struct Cohort {
    pub outcomes: Outcomes,
    pub ci_live: u32,
    pub ci_hit: u32,
}

pub(crate) fn cohort(prs: &[PrRow], span: Span) -> Cohort {
    let mut cohort = Cohort::default();
    for pr in prs
        .iter()
        .filter(|pr| pr.external && span.contains(pr.created_at))
    {
        cohort.outcomes.add(pr);
        if pr.live {
            cohort.ci_live += 1;
            cohort.ci_hit += u32::from(pr.ci_failures > 0);
        }
    }
    cohort
}

pub(crate) fn lands(prs: &[PrRow], span: Span, months: &[Timestamp], now: Timestamp) -> Lands {
    let cohort = cohort(prs, span);
    let stale_before = now.0 - STALE_DAYS * DAY;
    let open_over_30_days = prs
        .iter()
        .filter(|pr| {
            pr.external
                && pr.merged_at.is_none()
                && pr.closed_at.is_none()
                && pr.created_at.0 < stale_before
        })
        .count();
    Lands {
        summary: cohort.outcomes.summary(),
        open_over_30_days: super::count(open_over_30_days),
        ci_failed_share: share(cohort.ci_hit, cohort.ci_live),
        months: months.iter().map(|start| month(prs, *start, now)).collect(),
    }
}

fn month(prs: &[PrRow], start: Timestamp, now: Timestamp) -> MonthCohort {
    let end = add_months(start, 1);
    let span = Span {
        from: start,
        to: end,
    };
    let (mut opened, mut fast) = (0, 0);
    for pr in prs
        .iter()
        .filter(|pr| pr.external && span.contains(pr.created_at))
    {
        opened += 1;
        fast += u32::from(
            pr.merged_at
                .is_some_and(|merged| merged.0 - pr.created_at.0 <= STALE_DAYS * DAY),
        );
    }
    MonthCohort {
        start,
        opened,
        merged_within_30_days: fast,
        share: share(fast, opened),
        unfinished: end.0 + STALE_DAYS * DAY > now.0,
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::*;
    use super::*;

    const NOW: &str = "2026-11-16T12:00:00Z";

    fn span() -> Span {
        Span {
            from: at("2025-12-01T00:00:00Z"),
            to: Timestamp(at(NOW).0 + 1),
        }
    }

    fn run(prs: &[PrRow]) -> Lands {
        lands(prs, span(), &[at("2026-10-01T00:00:00Z")], at(NOW))
    }

    #[test]
    fn nothing_counted_gives_none_never_zero_percent() {
        let got = run(&[]);
        assert_eq!(got.summary.prs, 0);
        assert_eq!(got.summary.merge_rate, None);
        assert_eq!(got.summary.median_hours_to_merge, None);
        assert_eq!(got.summary.p75_hours_to_merge, None);
        assert_eq!(got.summary.changes_requested_share, None);
        assert_eq!(got.ci_failed_share, None);
        assert_eq!(got.months[0].share, None);
    }

    #[test]
    fn one_merged_pr_is_a_full_merge_rate_with_its_time() {
        let got = run(&[merged("2026-10-01T00:00:00Z", "2026-10-02T12:00:00Z")]);
        assert_eq!(got.summary.merge_rate, Some(1.0));
        assert_eq!(got.summary.median_hours_to_merge, Some(36.0));
        assert_eq!(got.summary.p75_hours_to_merge, Some(36.0));
    }

    #[test]
    fn an_open_pr_is_not_a_failure() {
        let only_open = run(&[pr("2026-10-01T00:00:00Z")]);
        assert_eq!(only_open.summary.merge_rate, None);
        assert_eq!(only_open.summary.open, 1);
        let mixed = run(&[
            pr("2026-10-01T00:00:00Z"),
            merged("2026-10-01T00:00:00Z", "2026-10-03T00:00:00Z"),
            closed("2026-10-01T00:00:00Z", "2026-10-03T00:00:00Z"),
        ]);
        assert_eq!(mixed.summary.merge_rate, Some(0.5));
    }

    #[test]
    fn open_prs_older_than_30_days_count_even_when_opened_before_the_span() {
        let got = run(&[
            pr("2024-01-01T00:00:00Z"),
            pr("2026-10-10T00:00:00Z"),
            pr("2026-11-10T00:00:00Z"),
        ]);
        assert_eq!(got.open_over_30_days, 2);
    }

    #[test]
    fn shares_count_prs_with_at_least_one_and_ci_reads_live_rows_only() {
        let mut a = pr("2026-10-01T00:00:00Z");
        a.changes_requested_rounds = 3;
        a.ci_failures = 2;
        let mut imported = pr("2026-10-02T00:00:00Z");
        imported.live = false;
        let b = pr("2026-10-03T00:00:00Z");
        let got = run(&[a, imported, b]);
        assert_eq!(got.summary.changes_requested_share, Some(1.0 / 3.0));
        assert_eq!(got.ci_failed_share, Some(0.5));
    }

    #[test]
    fn an_own_repo_is_left_out() {
        let mut own = merged("2026-10-01T00:00:00Z", "2026-10-02T00:00:00Z");
        own.external = false;
        let got = run(&[own, pr("2024-01-01T00:00:00Z")]);
        assert_eq!(got.summary.prs, 0);
        assert_eq!(got.months[0].opened, 0);
    }

    #[test]
    fn the_week_boundary_is_the_span_start_and_the_month_boundary_splits_cohorts() {
        let got = lands(
            &[
                merged("2025-11-30T23:59:59Z", "2025-12-02T00:00:00Z"),
                merged("2025-12-01T00:00:00Z", "2025-12-02T00:00:00Z"),
                merged("2026-09-30T23:59:59Z", "2026-10-02T00:00:00Z"),
                merged("2026-10-01T00:00:00Z", "2026-12-15T00:00:00Z"),
            ],
            span(),
            &[at("2026-09-01T00:00:00Z"), at("2026-10-01T00:00:00Z")],
            at(NOW),
        );
        assert_eq!(got.summary.merged, 3);
        assert_eq!(got.months[0].opened, 1);
        assert_eq!(got.months[1].opened, 1);
        assert_eq!(got.months[1].merged_within_30_days, 0);
    }

    #[test]
    fn a_month_is_unfinished_until_its_last_pr_has_had_30_days() {
        let months = [
            at("2026-09-01T00:00:00Z"),
            at("2026-10-01T00:00:00Z"),
            at("2026-11-01T00:00:00Z"),
        ];
        let got = lands(&[], span(), &months, at("2026-11-16T12:00:00Z"));
        let flags: Vec<_> = got.months.iter().map(|m| m.unfinished).collect();
        assert_eq!(flags, [false, true, true]);
    }

    #[test]
    fn thirty_days_exactly_counts_as_within() {
        let got = run(&[
            merged("2026-10-01T00:00:00Z", "2026-10-31T00:00:00Z"),
            merged("2026-10-01T00:00:00Z", "2026-10-31T00:00:01Z"),
        ]);
        assert_eq!(got.months[0].merged_within_30_days, 1);
        assert_eq!(got.months[0].share, Some(0.5));
    }
}
