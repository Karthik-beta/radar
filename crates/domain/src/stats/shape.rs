//! S7, Shape: merge rate by size of change, and the issues my merged pull
//! requests closed.

use serde::{Deserialize, Serialize};

use super::period::Span;
use super::{Outcomes, PrRow, Summary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    Under50,
    From50To199,
    From200To499,
    From500Up,
}

impl SizeClass {
    const ALL: [Self; 4] = [
        Self::Under50,
        Self::From50To199,
        Self::From200To499,
        Self::From500Up,
    ];

    fn of(lines: u32) -> Self {
        match lines {
            0..=49 => Self::Under50,
            50..=199 => Self::From50To199,
            200..=499 => Self::From200To499,
            _ => Self::From500Up,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SizeBucket {
    pub size: SizeClass,
    pub summary: Summary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub buckets: Vec<SizeBucket>,
    pub issues_closed: u32,
}

pub(crate) fn shape(prs: &[PrRow], span: Span, issues_closed: u32) -> Shape {
    let mut tallies: [Outcomes; 4] = Default::default();
    for pr in prs
        .iter()
        .filter(|pr| pr.external && span.contains(pr.created_at))
    {
        let class = SizeClass::of(pr.lines);
        if let Some(tally) = SizeClass::ALL
            .iter()
            .position(|c| *c == class)
            .and_then(|i| tallies.get_mut(i))
        {
            tally.add(pr);
        }
    }
    Shape {
        buckets: SizeClass::ALL
            .iter()
            .zip(&tallies)
            .map(|(size, tally)| SizeBucket {
                size: *size,
                summary: tally.summary(),
            })
            .collect(),
        issues_closed,
    }
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

    #[test]
    fn empty_input_gives_four_empty_buckets() {
        let got = shape(&[], span(), 0);
        assert_eq!(got.buckets.len(), 4);
        assert!(got.buckets.iter().all(|b| b.summary.merge_rate.is_none()));
        assert_eq!(got.issues_closed, 0);
    }

    #[test]
    fn lines_fall_in_the_documented_buckets() {
        let sizes = [0, 49, 50, 199, 200, 499, 500, 9000];
        let prs: Vec<PrRow> = sizes
            .iter()
            .map(|lines| PrRow {
                lines: *lines,
                ..merged("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z")
            })
            .collect();
        let got = shape(&prs, span(), 3);
        let merged: Vec<_> = got.buckets.iter().map(|b| b.summary.merged).collect();
        assert_eq!(merged, [2, 2, 2, 2]);
        assert_eq!(got.issues_closed, 3);
    }

    #[test]
    fn an_open_pr_is_not_a_failure_and_an_own_repo_is_left_out() {
        let mut own = closed("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z");
        own.external = false;
        let got = shape(&[pr("2026-03-01T00:00:00Z"), own], span(), 0);
        let first = &got.buckets[0].summary;
        assert_eq!((first.prs, first.open, first.merge_rate), (1, 1, None));
    }
}
