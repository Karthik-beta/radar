//! S4, Agents: the S1 numbers for pull requests from `agent/*` branches
//! against the rest.

use serde::{Deserialize, Serialize};

use super::period::Span;
use super::{Outcomes, PrRow, Summary};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agents {
    pub agent: Summary,
    pub other: Summary,
}

pub(crate) fn agents(prs: &[PrRow], span: Span) -> Agents {
    let (mut agent, mut other) = (Outcomes::default(), Outcomes::default());
    for pr in prs
        .iter()
        .filter(|pr| pr.external && span.contains(pr.created_at))
    {
        let side = if pr.agent { &mut agent } else { &mut other };
        side.add(pr);
    }
    Agents {
        agent: agent.summary(),
        other: other.summary(),
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
    fn empty_input_has_no_rates() {
        let got = agents(&[], span());
        assert_eq!((got.agent.prs, got.other.prs), (0, 0));
        assert_eq!(got.agent.merge_rate, None);
        assert_eq!(got.other.median_hours_to_merge, None);
    }

    #[test]
    fn the_split_sums_to_the_whole_and_open_is_not_a_failure() {
        let mut a = merged("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z");
        a.agent = true;
        let mut b = pr("2026-03-01T00:00:00Z");
        b.agent = true;
        let c = closed("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z");
        let mut own = merged("2026-03-01T00:00:00Z", "2026-03-02T00:00:00Z");
        own.external = false;
        own.agent = true;
        let got = agents(&[a, b, c, own], span());
        assert_eq!((got.agent.prs, got.other.prs), (2, 1));
        assert_eq!(got.agent.merge_rate, Some(1.0));
        assert_eq!(got.other.merge_rate, Some(0.0));
        assert_eq!(got.agent.median_hours_to_merge, Some(24.0));
    }
}
