//! S6, Waiting: how long my pull requests sit in each stage, and on whose side.
//!
//! The rows are `pr_transitions`, so imported pull requests, which have none,
//! never count. A segment runs from one row to the next row of the same pull
//! request, to the day it merged or closed, or to `now` while it is open.

use serde::{Deserialize, Serialize};

use super::period::Span;
use super::{hours, percentile, StageRow, DAY};
use crate::facts::Side;
use crate::ids::Timestamp;
use crate::stage::Stage;

const STAGES: [Stage; 5] = [
    Stage::CiRunning,
    Stage::CiFailing,
    Stage::AwaitingReview,
    Stage::ChangesRequested,
    Stage::Approved,
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Waiting {
    pub stages: Vec<StageTime>,
    /// Days my open pull requests waited on maintainers and CI.
    pub days_waiting_on_them: f64,
    /// Days they waited on me.
    pub days_waiting_on_me: f64,
    /// Pull requests with at least one counted segment.
    pub prs: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageTime {
    pub stage: Stage,
    pub median_hours: Option<f64>,
    pub samples: u32,
}

/// Whether any segment of an external pull request overlaps `span`, open or ended.
pub(crate) fn contributes(rows: &[StageRow], span: Span, now: Timestamp) -> bool {
    rows.iter().enumerate().any(|(i, row)| {
        let next = rows
            .get(i + 1)
            .filter(|next| next.item == row.item)
            .map(|next| next.at);
        let end = next
            .into_iter()
            .chain(row.ended_at)
            .min()
            .unwrap_or(now)
            .min(now);
        row.external
            && !matches!(row.stage, Stage::Merged | Stage::Closed)
            && end > row.at
            && end.min(span.to) > row.at.max(span.from)
    })
}

pub(crate) fn waiting(rows: &[StageRow], span: Span, now: Timestamp) -> Waiting {
    let kept: Vec<&StageRow> = rows.iter().filter(|row| row.external).collect();
    let mut samples: [Vec<f64>; 5] = Default::default();
    let (mut them, mut me) = (0i64, 0i64);
    let mut prs = 0;
    let mut counted: Option<&str> = None;
    for (i, row) in kept.iter().enumerate() {
        if matches!(row.stage, Stage::Merged | Stage::Closed) {
            continue;
        }
        let next = kept
            .get(i + 1)
            .filter(|next| next.item == row.item)
            .map(|next| next.at);
        let end = next
            .into_iter()
            .chain(row.ended_at)
            .min()
            .unwrap_or(now)
            .min(now);
        if end <= row.at {
            continue;
        }
        let (from, to) = (row.at.max(span.from), end.min(span.to));
        // Days count only while the pull request is open. Ended ones keep their stage samples.
        if to > from && row.ended_at.is_none() {
            match row.side {
                Side::Them => them += to.0 - from.0,
                Side::Me => me += to.0 - from.0,
            }
            if counted != Some(row.item.as_str()) {
                counted = Some(row.item.as_str());
                prs += 1;
            }
        }
        if span.contains(row.at) {
            if let Some(slot) = STAGES
                .iter()
                .position(|stage| *stage == row.stage)
                .and_then(|i| samples.get_mut(i))
            {
                slot.push(hours(row.at, end));
            }
        }
    }
    Waiting {
        stages: STAGES
            .iter()
            .zip(&samples)
            .map(|(stage, hours)| StageTime {
                stage: *stage,
                median_hours: percentile(hours, 50),
                samples: super::count(hours.len()),
            })
            .collect(),
        days_waiting_on_them: them as f64 / DAY as f64,
        days_waiting_on_me: me as f64 / DAY as f64,
        prs,
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::at;
    use super::*;

    const NOW: &str = "2026-11-16T00:00:00Z";

    fn span() -> Span {
        Span {
            from: at("2026-01-01T00:00:00Z"),
            to: Timestamp(at(NOW).0 + 1),
        }
    }

    fn row(item: &str, stage: Stage, side: Side, when: &str, ended: Option<&str>) -> StageRow {
        StageRow {
            item: item.into(),
            external: true,
            stage,
            side,
            at: at(when),
            ended_at: ended.map(at),
        }
    }

    /// Every row of an ended pull request carries its end, as the SQL joins it on.
    fn all_ended(rows: &mut [StageRow]) {
        let end = rows.last().and_then(|row| row.ended_at);
        for row in rows {
            row.ended_at = end;
        }
    }

    fn stage(w: &Waiting, which: Stage) -> &StageTime {
        w.stages.iter().find(|s| s.stage == which).expect("stage")
    }

    #[test]
    fn no_rows_gives_none_medians_and_zero_days() {
        let got = waiting(&[], span(), at(NOW));
        assert_eq!(got.stages.len(), 5);
        assert!(got
            .stages
            .iter()
            .all(|s| s.median_hours.is_none() && s.samples == 0));
        assert_eq!(
            (got.days_waiting_on_them, got.days_waiting_on_me, got.prs),
            (0.0, 0.0, 0)
        );
    }

    #[test]
    fn one_closed_pr_gives_stage_times_but_no_days_and_skips_the_terminal_row() {
        let mut rows = [
            row(
                "a",
                Stage::CiRunning,
                Side::Them,
                "2026-03-01T00:00:00Z",
                None,
            ),
            row(
                "a",
                Stage::AwaitingReview,
                Side::Them,
                "2026-03-01T06:00:00Z",
                None,
            ),
            row(
                "a",
                Stage::ChangesRequested,
                Side::Me,
                "2026-03-03T06:00:00Z",
                None,
            ),
            row(
                "a",
                Stage::Merged,
                Side::Them,
                "2026-03-04T06:00:00Z",
                Some("2026-03-04T06:00:00Z"),
            ),
        ];
        all_ended(&mut rows);
        let got = waiting(&rows, span(), at(NOW));
        assert_eq!(stage(&got, Stage::CiRunning).median_hours, Some(6.0));
        assert_eq!(stage(&got, Stage::AwaitingReview).median_hours, Some(48.0));
        assert_eq!(
            stage(&got, Stage::ChangesRequested).median_hours,
            Some(24.0)
        );
        // An ended pull request adds samples but no days.
        assert_eq!(
            (got.days_waiting_on_them, got.days_waiting_on_me),
            (0.0, 0.0)
        );
    }

    #[test]
    fn a_turn_only_change_splits_the_days_and_gives_two_samples() {
        let rows = [
            row(
                "a",
                Stage::AwaitingReview,
                Side::Them,
                "2026-03-01T00:00:00Z",
                None,
            ),
            row(
                "a",
                Stage::AwaitingReview,
                Side::Me,
                "2026-03-02T00:00:00Z",
                None,
            ),
        ];
        let got = waiting(&rows, span(), at(NOW));
        assert_eq!(stage(&got, Stage::AwaitingReview).samples, 2);
        assert_eq!(stage(&got, Stage::AwaitingReview).median_hours, Some(24.0));
        let to_now = (at(NOW).0 - at("2026-03-02T00:00:00Z").0) as f64 / 86_400.0;
        assert_eq!(
            (got.days_waiting_on_them, got.days_waiting_on_me),
            (1.0, to_now)
        );
    }

    #[test]
    fn an_open_pr_counts_up_to_now() {
        let rows = [row(
            "a",
            Stage::AwaitingReview,
            Side::Them,
            "2026-11-10T00:00:00Z",
            None,
        )];
        let got = waiting(&rows, span(), at(NOW));
        assert_eq!(got.days_waiting_on_them, 6.0);
        assert_eq!(stage(&got, Stage::AwaitingReview).median_hours, Some(144.0));
    }

    #[test]
    fn a_pr_with_no_rows_is_left_out_and_an_own_repo_is_skipped() {
        let mut own = row(
            "o",
            Stage::AwaitingReview,
            Side::Them,
            "2026-11-10T00:00:00Z",
            None,
        );
        own.external = false;
        let got = waiting(&[own], span(), at(NOW));
        assert_eq!(got.prs, 0);
        assert_eq!(got.days_waiting_on_them, 0.0);
    }

    #[test]
    fn days_are_clipped_to_the_span_while_the_stage_sample_keeps_its_full_length() {
        let jan = Span {
            from: at("2026-01-01T00:00:00Z"),
            to: at("2026-02-01T00:00:00Z"),
        };
        let feb = Span {
            from: at("2026-02-01T00:00:00Z"),
            to: at("2026-03-01T00:00:00Z"),
        };
        let open = [row(
            "a",
            Stage::AwaitingReview,
            Side::Them,
            "2026-01-30T00:00:00Z",
            None,
        )];
        assert_eq!(waiting(&open, jan, at(NOW)).days_waiting_on_them, 2.0);
        assert_eq!(waiting(&open, feb, at(NOW)).days_waiting_on_them, 28.0);
        let ended = [row(
            "a",
            Stage::AwaitingReview,
            Side::Them,
            "2026-01-30T00:00:00Z",
            Some("2026-02-10T00:00:00Z"),
        )];
        let got = waiting(&ended, jan, at(NOW));
        assert_eq!(got.days_waiting_on_them, 0.0);
        assert_eq!(stage(&got, Stage::AwaitingReview).median_hours, Some(264.0));
        let got = waiting(&ended, feb, at(NOW));
        assert_eq!(got.days_waiting_on_them, 0.0);
        assert_eq!(stage(&got, Stage::AwaitingReview).samples, 0);
        assert!(contributes(&ended, feb, at(NOW)));
        assert!(!contributes(
            &ended,
            Span {
                from: at("2026-03-01T00:00:00Z"),
                to: at("2026-04-01T00:00:00Z")
            },
            at(NOW)
        ));
    }
}
