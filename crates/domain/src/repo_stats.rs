//! Per-repo cards (F7): how my pull requests fare in each repo and how fast
//! maintainers first reply. Medians are computed on read and never stored.

use serde::{Deserialize, Serialize};

use crate::event::{Event, EventKind};
use crate::ids::{RepoName, Timestamp};

/// A whole percent, 0 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Percent(pub u8);

/// `merge_rate` and `median_first_response_secs` are `None` when there is
/// nothing to measure, so a card never shows a false 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RepoCard {
    pub repo: RepoName,
    pub open: u32,
    pub merged: u32,
    pub closed: u32,
    pub merge_rate: Option<Percent>,
    pub median_first_response_secs: Option<i64>,
    pub last_activity: Timestamp,
}

/// My pull request counts in one repo, as the database reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoCounts {
    pub repo: RepoName,
    pub open: u32,
    pub merged: u32,
    pub closed: u32,
    pub last_activity: Timestamp,
}

/// Merged over merged plus closed without merge. Open pull requests do not count.
pub fn merge_rate(merged: u32, closed: u32) -> Option<Percent> {
    let ended = u64::from(merged) + u64::from(closed);
    if ended == 0 {
        return None;
    }
    let percent = (u64::from(merged) * 100 + ended / 2) / ended;
    Some(Percent(u8::try_from(percent).unwrap_or(100)))
}

/// An empty list gives `None`, an even count the mean of the two middle values.
pub fn median(values: &[i64]) -> Option<i64> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        n if n % 2 == 1 => Some(sorted[mid]),
        _ => Some((sorted[mid - 1] + sorted[mid]) / 2),
    }
}

/// Seconds from opening to the first reply by a human who is not me. A bot, my
/// own event, and anything dated before `opened_at` never count.
pub fn first_response(opened_at: Timestamp, events: &[Event]) -> Option<i64> {
    events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::Commented | EventKind::Reviewed(_)))
        .filter(|event| {
            event
                .actor
                .as_ref()
                .is_some_and(|actor| !actor.is_me && !actor.is_bot)
        })
        .map(|event| event.occurred_at.0)
        .filter(|at| *at >= opened_at.0)
        .min()
        .map(|at| at - opened_at.0)
}

/// One card per repo, most recently active first. `responses` holds one
/// `(repo, first_response_secs)` pair per pull request that has a reply.
pub fn cards(counts: Vec<RepoCounts>, responses: &[(RepoName, i64)]) -> Vec<RepoCard> {
    let mut cards: Vec<RepoCard> = counts
        .into_iter()
        .map(|c| {
            let secs: Vec<i64> = responses
                .iter()
                .filter(|(repo, _)| *repo == c.repo)
                .map(|(_, secs)| *secs)
                .collect();
            RepoCard {
                merge_rate: merge_rate(c.merged, c.closed),
                median_first_response_secs: median(&secs),
                repo: c.repo,
                open: c.open,
                merged: c.merged,
                closed: c.closed,
                last_activity: c.last_activity,
            }
        })
        .collect();
    cards.sort_by(|a, b| {
        b.last_activity
            .0
            .cmp(&a.last_activity.0)
            .then_with(|| a.repo.to_string().cmp(&b.repo.to_string()))
    });
    cards
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::ReviewState;
    use crate::event::{EventActor, EventId};
    use crate::ids::Login;

    const DAY: i64 = 86_400;
    const OPENED: Timestamp = Timestamp(1_000_000);

    fn repo(name: &str) -> RepoName {
        RepoName::parse(name).expect("repo")
    }

    fn event(kind: EventKind, login: &str, me: bool, bot: bool, at: i64) -> Event {
        Event {
            id: EventId::from_stored(&format!("e{at}")),
            item: "PR_1".into(),
            repo: repo("o/r"),
            kind,
            actor: Some(EventActor {
                login: Login::parse(login).expect("login"),
                is_me: me,
                is_bot: bot,
            }),
            occurred_at: Timestamp(at),
        }
    }

    #[test]
    fn merge_rate_counts_only_ended_pull_requests() {
        assert_eq!(merge_rate(3, 1), Some(Percent(75)));
        assert_eq!(merge_rate(0, 0), None);
        assert_eq!(merge_rate(0, 2), Some(Percent(0)));
        assert_eq!(merge_rate(2, 0), Some(Percent(100)));
    }

    #[test]
    fn median_handles_empty_odd_and_even_counts() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[9, 1, 4]), Some(4));
        assert_eq!(median(&[DAY, 2 * DAY, 4 * DAY, 9 * DAY]), Some(3 * DAY));
    }

    #[test]
    fn first_response_skips_bots_and_me_and_takes_the_first_human() {
        let events = [
            event(EventKind::Commented, "me", true, false, OPENED.0 + 10),
            event(
                EventKind::Reviewed(ReviewState::Approved),
                "ci-bot",
                false,
                true,
                OPENED.0 + 20,
            ),
            event(EventKind::Commented, "alice", false, false, OPENED.0 + 500),
            event(EventKind::Commented, "bob", false, false, OPENED.0 + 100),
            event(EventKind::Committed, "carol", false, false, OPENED.0 + 5),
        ];
        assert_eq!(first_response(OPENED, &events), Some(100));
    }

    #[test]
    fn first_response_is_none_without_a_human_reply() {
        let events = [
            event(EventKind::Commented, "me", true, false, OPENED.0 + 10),
            event(EventKind::Commented, "bot", false, true, OPENED.0 + 20),
        ];
        assert_eq!(first_response(OPENED, &events), None);
        assert_eq!(first_response(OPENED, &[]), None);
    }

    #[test]
    fn first_response_ignores_events_dated_before_opening() {
        let events = [
            event(EventKind::Commented, "alice", false, false, OPENED.0 - 50),
            event(EventKind::Commented, "bob", false, false, OPENED.0 + 70),
        ];
        assert_eq!(first_response(OPENED, &events), Some(70));
        assert_eq!(first_response(OPENED, &events[..1]), None);
    }

    #[test]
    fn cards_fill_medians_per_repo_and_order_by_activity() {
        let counts = vec![
            RepoCounts {
                repo: repo("o/old"),
                open: 2,
                merged: 3,
                closed: 1,
                last_activity: Timestamp(10),
            },
            RepoCounts {
                repo: repo("o/new"),
                open: 0,
                merged: 0,
                closed: 0,
                last_activity: Timestamp(20),
            },
        ];
        let responses: Vec<_> = [DAY, 2 * DAY, 4 * DAY, 9 * DAY]
            .into_iter()
            .map(|s| (repo("o/old"), s))
            .collect();
        let cards = cards(counts, &responses);
        assert_eq!(cards[0].repo, repo("o/new"));
        assert_eq!(cards[0].merge_rate, None);
        assert_eq!(cards[0].median_first_response_secs, None);
        assert_eq!(cards[1].merge_rate, Some(Percent(75)));
        assert_eq!(cards[1].median_first_response_secs, Some(3 * DAY));
    }
}
