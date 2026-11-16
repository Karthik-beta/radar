//! The Stats page's numbers (F14, R28): eight sections, each a pure function
//! from rows to a struct. The Worker only selects the rows.
//!
//! Every metric counts external repos only, unless its section says otherwise.
//! A pull request that is still open is never a failure. A rate, median, or
//! percentile is `None` when nothing is counted, so a page never shows a false
//! 0 percent.

mod agents;
mod habits;
mod lands;
mod output;
mod period;
mod reviews;
mod shape;
mod trust;
mod waiting;

use serde::{Deserialize, Serialize};

use crate::activity::ReviewState;
use crate::facts::Side;
use crate::ids::{RepoName, Timestamp};
use crate::stage::Stage;
use crate::trends::Trends;

pub use agents::Agents;
pub use habits::{Goal, Habits, Milestone, MilestoneKind};
pub use lands::{Lands, MonthCohort};
pub use output::{Best, MonthPoint, Output, WeekPoint};
pub use period::{Grain, Period, Span};
pub use reviews::{RepoCount, ReviewBucket, Reviews};
pub use shape::{Shape, SizeBucket, SizeClass};
pub use trust::{LanguageCount, RepoRank, Trust};
pub use waiting::{StageTime, Waiting};

pub(crate) const HOUR: i64 = 3_600;
pub(crate) const DAY: i64 = 86_400;

/// A pull request I authored, with its repo's stars and language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrRow {
    pub repo: RepoName,
    pub external: bool,
    pub created_at: Timestamp,
    pub merged_at: Option<Timestamp>,
    pub closed_at: Option<Timestamp>,
    pub first_response_at: Option<Timestamp>,
    pub changes_requested_rounds: u32,
    pub ci_failures: u32,
    pub lines: u32,
    pub agent: bool,
    /// Imported rows carry no CI failures, so CI shares count live rows only.
    pub live: bool,
    pub stars: Option<u32>,
    pub language: Option<String>,
}

/// A merged pull request of mine, from the whole history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRow {
    pub repo: RepoName,
    pub external: bool,
    pub at: Timestamp,
    pub stars: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRow {
    pub repo: RepoName,
    pub external: bool,
    pub state: ReviewState,
    pub at: Timestamp,
}

/// A stage or turn change on a pull request of mine. `ended_at` is when the
/// pull request merged or closed, `None` while it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRow {
    pub item: String,
    pub external: bool,
    pub stage: Stage,
    pub side: Side,
    pub at: Timestamp,
    pub ended_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportState {
    pub facts_imported_at: Option<Timestamp>,
    pub account_created_at: Option<Timestamp>,
}

impl ImportState {
    /// Year over year works once the import has finished and the account is
    /// older than this year (ADR 0010).
    pub fn yoy_ready(&self, now: Timestamp) -> bool {
        self.facts_imported_at.is_some()
            && self
                .account_created_at
                .is_some_and(|created| created < period::year_start(now, 0))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatsInput {
    pub now: Timestamp,
    pub utc_offset_minutes: i32,
    pub goal_merged_per_month: Option<u32>,
    pub import: ImportState,
    pub prs: Vec<PrRow>,
    /// Oldest first.
    pub merges: Vec<MergeRow>,
    pub reviews: Vec<ReviewRow>,
    /// Grouped by pull request, oldest first within each.
    pub transitions: Vec<StageRow>,
    /// Issues I opened or commented on that a merged pull request of mine closed.
    pub closed_issues: u32,
    /// When I acted, from `events` and `own_events`.
    pub events: Vec<Timestamp>,
    pub trends: Trends,
}

/// The counts behind a merge rate, over the external pull requests added.
#[derive(Debug, Default)]
pub(crate) struct Outcomes {
    prs: u32,
    merged: u32,
    closed: u32,
    open: u32,
    changes_requested: u32,
    merge_hours: Vec<f64>,
}

impl Outcomes {
    pub(crate) fn add(&mut self, pr: &PrRow) {
        self.prs += 1;
        if pr.changes_requested_rounds > 0 {
            self.changes_requested += 1;
        }
        match (pr.merged_at, pr.closed_at) {
            (Some(merged), _) => {
                self.merged += 1;
                self.merge_hours.push(hours(pr.created_at, merged));
            }
            (None, Some(_)) => self.closed += 1,
            (None, None) => self.open += 1,
        }
    }

    pub(crate) fn merge_rate(&self) -> Option<f64> {
        share(self.merged, self.merged + self.closed)
    }

    pub(crate) fn summary(&self) -> Summary {
        Summary {
            prs: self.prs,
            merged: self.merged,
            closed_unmerged: self.closed,
            open: self.open,
            merge_rate: self.merge_rate(),
            median_hours_to_merge: percentile(&self.merge_hours, 50),
            p75_hours_to_merge: percentile(&self.merge_hours, 75),
            changes_requested_share: share(self.changes_requested, self.prs),
        }
    }
}

/// What S1 and S4 say about a group of pull requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub prs: u32,
    pub merged: u32,
    pub closed_unmerged: u32,
    pub open: u32,
    pub merge_rate: Option<f64>,
    pub median_hours_to_merge: Option<f64>,
    pub p75_hours_to_merge: Option<f64>,
    pub changes_requested_share: Option<f64>,
}

/// `part` over `whole` as a fraction, or `None` when `whole` is 0.
pub fn share(part: u32, whole: u32) -> Option<f64> {
    (whole > 0).then(|| f64::from(part) / f64::from(whole))
}

pub(crate) fn hours(from: Timestamp, to: Timestamp) -> f64 {
    (to.0 - from.0).max(0) as f64 / HOUR as f64
}

pub(crate) fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The first `n` of `items` in order, where `before(a, b)` says `a` ranks ahead
/// of `b`. `before` must break its own ties, as selection does not keep input
/// order. It selects instead of calling a sort, because each sorted type adds
/// about 3 KB to the Worker, which has little of its 400 KB budget left.
pub(crate) fn top<T>(mut items: Vec<T>, n: usize, before: impl Fn(&T, &T) -> bool) -> Vec<T> {
    let n = n.min(items.len());
    for k in 0..n {
        let mut best = k;
        for i in k + 1..items.len() {
            if matches!((items.get(i), items.get(best)), (Some(a), Some(b)) if before(a, b)) {
                best = i;
            }
        }
        items.swap(k, best);
    }
    items.truncate(n);
    items
}

/// The nearest-rank percentile:the value at rank `ceil(p / 100 * n)` of the
/// sorted list. `None` for an empty list.
pub fn percentile(values: &[f64], p: u32) -> Option<f64> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    sorted.sort_by(f64::total_cmp);
    let rank = (u64::from(p) * sorted.len() as u64).div_ceil(100).max(1);
    sorted
        .get(usize::try_from(rank).ok()?.checked_sub(1)?)
        .copied()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub period: Period,
    pub yoy_ready: bool,
    pub lands: Lands,
    pub reviews: Reviews,
    pub trust: Trust,
    pub agents: Agents,
    pub output: Output,
    pub waiting: Waiting,
    pub shape: Shape,
    pub habits: Habits,
    /// Filled only for `YearOverYear` once `yoy_ready` is true.
    pub yoy: Option<Yoy>,
}

/// Each month of this year next to the same month of last year.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Yoy {
    pub months: Vec<YoyMonth>,
    pub gaps: YoyGaps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YoyMonth {
    pub month: u8,
    pub this_year: MonthStats,
    pub last_year: MonthStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonthStats {
    pub opened: u32,
    pub merged: u32,
    pub merge_rate: Option<f64>,
    pub median_hours_to_merge: Option<f64>,
    pub ci_failed_share: Option<f64>,
    pub reviews: u32,
    pub awaiting_review_median_hours: Option<f64>,
    pub days_waiting_on_them: f64,
    pub days_waiting_on_me: f64,
    pub events: u32,
}

/// True when last year's months hold no data for the metric, as ADR 0010
/// says: stage and turn history, CI failures, and my events start with live data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct YoyGaps {
    pub stage_times: bool,
    pub turn_times: bool,
    pub ci_failures: bool,
    pub habits: bool,
}

pub fn build_stats(input: &StatsInput, period: Period) -> Stats {
    let now = input.now;
    let span = period.span(now);
    let buckets = period.bucket_starts(now);
    let months = period.month_starts(now);
    let yoy_ready = input.import.yoy_ready(now);
    let yoy = (period == Period::YearOverYear && yoy_ready).then(|| yoy(input));
    Stats {
        period,
        yoy_ready,
        lands: lands::lands(&input.prs, span, &months, now),
        reviews: reviews::reviews(&input.reviews, span, &buckets),
        trust: trust::trust(&input.prs, &input.merges, span),
        agents: agents::agents(&input.prs, span),
        output: output::output(&input.trends),
        waiting: waiting::waiting(&input.transitions, span, now),
        shape: shape::shape(&input.prs, span, input.closed_issues),
        habits: habits::habits(
            &input.events,
            &input.merges,
            span,
            input.utc_offset_minutes,
            input.goal_merged_per_month,
            now,
        ),
        yoy,
    }
}

fn yoy(input: &StatsInput) -> Yoy {
    let now = input.now;
    let this_year = period::year_start(now, 0);
    let last_year = period::year_start(now, -1);
    let months = (0..12)
        .map(|i| YoyMonth {
            month: u8::try_from(i + 1).unwrap_or(1),
            this_year: month_stats(input, period::add_months(this_year, i)),
            last_year: month_stats(input, period::add_months(last_year, i)),
        })
        .collect();
    let last = Span {
        from: last_year,
        to: this_year,
    };
    let history = waiting::waiting(&input.transitions, last, now);
    let no_turns = !waiting::contributes(&input.transitions, last, now);
    Yoy {
        months,
        gaps: YoyGaps {
            stage_times: history.stages.iter().all(|stage| stage.samples == 0),
            turn_times: no_turns,
            ci_failures: !input
                .prs
                .iter()
                .any(|pr| pr.external && pr.live && last.contains(pr.created_at)),
            habits: !input.events.iter().any(|at| last.contains(*at)),
        },
    }
}

fn month_stats(input: &StatsInput, start: Timestamp) -> MonthStats {
    let span = Span {
        from: start,
        to: period::add_months(start, 1),
    };
    let cohort = lands::cohort(&input.prs, span);
    let summary = cohort.outcomes.summary();
    let waiting = waiting::waiting(&input.transitions, span, input.now);
    let awaiting = waiting
        .stages
        .iter()
        .find(|stage| stage.stage == Stage::AwaitingReview)
        .and_then(|stage| stage.median_hours);
    MonthStats {
        opened: summary.prs,
        merged: count(
            input
                .prs
                .iter()
                .filter(|pr| pr.external && pr.merged_at.is_some_and(|at| span.contains(at)))
                .count(),
        ),
        merge_rate: summary.merge_rate,
        median_hours_to_merge: summary.median_hours_to_merge,
        ci_failed_share: share(cohort.ci_hit, cohort.ci_live),
        reviews: count(
            input
                .reviews
                .iter()
                .filter(|row| row.external && span.contains(row.at) && reviews::counts(row.state))
                .count(),
        ),
        awaiting_review_median_hours: awaiting,
        days_waiting_on_them: waiting.days_waiting_on_them,
        days_waiting_on_me: waiting.days_waiting_on_me,
        events: count(input.events.iter().filter(|at| span.contains(**at)).count()),
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub fn at(s: &str) -> Timestamp {
        Timestamp::parse_rfc3339(s).expect("timestamp")
    }

    pub fn repo(s: &str) -> RepoName {
        RepoName::parse(s).expect("repo")
    }

    /// An external pull request created at `created` and still open.
    pub fn pr(created: &str) -> PrRow {
        PrRow {
            repo: repo("o/one"),
            external: true,
            created_at: at(created),
            merged_at: None,
            closed_at: None,
            first_response_at: None,
            changes_requested_rounds: 0,
            ci_failures: 0,
            lines: 10,
            agent: false,
            live: true,
            stars: None,
            language: None,
        }
    }

    pub fn merged(created: &str, merged: &str) -> PrRow {
        PrRow {
            merged_at: Some(at(merged)),
            closed_at: Some(at(merged)),
            ..pr(created)
        }
    }

    pub fn closed(created: &str, closed: &str) -> PrRow {
        PrRow {
            closed_at: Some(at(closed)),
            ..pr(created)
        }
    }

    pub fn empty_input(now: &str) -> StatsInput {
        StatsInput {
            now: at(now),
            utc_offset_minutes: 0,
            goal_merged_per_month: None,
            import: ImportState::default(),
            prs: Vec::new(),
            merges: Vec::new(),
            reviews: Vec::new(),
            transitions: Vec::new(),
            closed_issues: 0,
            events: Vec::new(),
            trends: crate::trends::build_trends(&[], &[], &[], at(now)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn percentile_is_nearest_rank() {
        assert_eq!(percentile(&[], 50), None);
        assert_eq!(percentile(&[7.0], 50), Some(7.0));
        assert_eq!(percentile(&[7.0], 75), Some(7.0));
        assert_eq!(percentile(&[2.0, 1.0], 50), Some(1.0));
        assert_eq!(percentile(&[2.0, 1.0], 75), Some(2.0));
        let four = [4.0, 1.0, 3.0, 2.0];
        assert_eq!(percentile(&four, 50), Some(2.0));
        assert_eq!(percentile(&four, 75), Some(3.0));
        let nine: Vec<f64> = (1..=9).map(f64::from).collect();
        assert_eq!(percentile(&nine, 50), Some(5.0));
        assert_eq!(percentile(&nine, 75), Some(7.0));
        assert_eq!(percentile(&nine, 100), Some(9.0));
    }

    #[test]
    fn top_returns_the_best_n_in_order() {
        assert_eq!(top(vec![3, 9, 1, 7, 5], 3, |a, b| a > b), [9, 7, 5]);
        assert_eq!(top(vec![2, 1], 10, |a, b| a > b), [2, 1]);
        assert!(top(Vec::<u32>::new(), 3, |a, b| a > b).is_empty());
    }

    #[test]
    fn an_empty_database_gives_none_and_zero_everywhere() {
        let input = empty_input("2026-11-16T12:00:00Z");
        for period in [
            Period::Last12Months(Grain::Week),
            Period::Last12Months(Grain::Month),
            Period::YearOverYear,
        ] {
            let stats = build_stats(&input, period);
            assert!(!stats.yoy_ready);
            assert!(stats.yoy.is_none());
            assert_eq!(stats.lands.summary.merged, 0);
            assert_eq!(stats.lands.summary.merge_rate, None);
            assert_eq!(stats.lands.summary.median_hours_to_merge, None);
            assert_eq!(stats.lands.ci_failed_share, None);
            assert_eq!(stats.agents.agent.merge_rate, None);
            assert_eq!(stats.waiting.days_waiting_on_me, 0.0);
            assert!(stats.habits.goal.is_none());
            let json = serde_json::to_string(&stats).expect("json");
            assert!(!json.contains("NaN") && !json.contains("inf"), "{json}");
        }
    }

    #[test]
    fn year_over_year_waits_for_the_import_and_an_older_account() {
        let now = at("2026-11-16T12:00:00Z");
        let mut state = ImportState {
            facts_imported_at: None,
            account_created_at: Some(at("2020-05-01T00:00:00Z")),
        };
        assert!(!state.yoy_ready(now));
        state.facts_imported_at = Some(at("2026-11-01T00:00:00Z"));
        assert!(state.yoy_ready(now));
        state.account_created_at = Some(at("2026-03-01T00:00:00Z"));
        assert!(!state.yoy_ready(now));
        state.account_created_at = None;
        assert!(!state.yoy_ready(now));
    }

    #[test]
    fn year_over_year_pairs_each_month_and_flags_missing_history() {
        let mut input = empty_input("2026-11-16T12:00:00Z");
        input.import = ImportState {
            facts_imported_at: Some(at("2026-11-01T00:00:00Z")),
            account_created_at: Some(at("2020-01-01T00:00:00Z")),
        };
        let mut old = merged("2025-03-02T00:00:00Z", "2025-03-04T00:00:00Z");
        old.live = false;
        input.prs = vec![
            old,
            merged("2026-03-02T00:00:00Z", "2026-03-03T00:00:00Z"),
            merged("2026-03-05T00:00:00Z", "2026-03-06T12:00:00Z"),
        ];
        let stats = build_stats(&input, Period::YearOverYear);
        let yoy = stats.yoy.expect("ready");
        assert_eq!(yoy.months.len(), 12);
        let march = &yoy.months[2];
        assert_eq!((march.this_year.opened, march.this_year.merged), (2, 2));
        assert_eq!((march.last_year.opened, march.last_year.merged), (1, 1));
        assert_eq!(march.last_year.median_hours_to_merge, Some(48.0));
        assert_eq!(yoy.months[3].last_year.merge_rate, None);
        assert!(yoy.gaps.stage_times && yoy.gaps.ci_failures && yoy.gaps.habits);
    }

    #[test]
    fn year_over_year_counts_only_the_reviews_the_reviews_section_counts() {
        let mut input = empty_input("2026-11-16T12:00:00Z");
        input.import = ImportState {
            facts_imported_at: Some(at("2026-11-01T00:00:00Z")),
            account_created_at: Some(at("2020-01-01T00:00:00Z")),
        };
        for state in [
            ReviewState::Approved,
            ReviewState::Dismissed,
            ReviewState::Pending,
        ] {
            input.reviews.push(ReviewRow {
                repo: repo("o/one"),
                external: true,
                state,
                at: at("2026-03-05T00:00:00Z"),
            });
        }
        let stats = build_stats(&input, Period::YearOverYear);
        let march = stats.yoy.expect("ready").months[2].this_year.reviews;
        assert_eq!(march, 1);
    }

    #[test]
    fn a_turn_interval_that_starts_before_last_year_still_ends_the_gap() {
        let mut input = empty_input("2026-11-16T12:00:00Z");
        input.import = ImportState {
            facts_imported_at: Some(at("2026-11-01T00:00:00Z")),
            account_created_at: Some(at("2020-01-01T00:00:00Z")),
        };
        let row = |stage, when: &str| StageRow {
            item: "a".into(),
            external: true,
            stage,
            side: Side::Them,
            at: at(when),
            ended_at: Some(at("2025-02-01T00:00:00Z")),
        };
        input.transitions = vec![
            row(Stage::AwaitingReview, "2024-12-20T00:00:00Z"),
            row(Stage::Closed, "2025-02-01T00:00:00Z"),
        ];
        let gaps = build_stats(&input, Period::YearOverYear)
            .yoy
            .expect("ready")
            .gaps;
        assert!(!gaps.turn_times);
        assert!(gaps.stage_times, "no stage visit started in 2025");
        input.transitions.clear();
        let gaps = build_stats(&input, Period::YearOverYear)
            .yoy
            .expect("ready")
            .gaps;
        assert!(gaps.turn_times);
    }

    #[test]
    fn the_period_serializes_with_its_grain() {
        let json = serde_json::to_string(&Period::Last12Months(Grain::Month)).expect("json");
        assert_eq!(json, r#"{"kind":"last12_months","grain":"month"}"#);
        let json = serde_json::to_string(&Period::YearOverYear).expect("json");
        assert_eq!(json, r#"{"kind":"year_over_year"}"#);
    }
}
