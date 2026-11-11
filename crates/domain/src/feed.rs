//! The activity feed (F6): events newest first, filtered by repo and kind,
//! read a page at a time with a keyset cursor.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::activity::ReviewState;
use crate::event::{Event, EventKind};
use crate::ids::{RepoName, Timestamp};

pub const PAGE_SIZE: usize = 50;

const MAX_CURSOR_ID_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedError {
    BadKind(String),
    BadRepo(String),
    BadCursor(String),
}

impl fmt::Display for FeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadKind(s) => write!(f, "unknown kind `{s}`"),
            Self::BadRepo(s) => write!(f, "bad repo `{s}`"),
            Self::BadCursor(s) => write!(f, "bad cursor `{s}`, expected <at>_<id>"),
        }
    }
}

impl std::error::Error for FeedError {}

/// The tag words of `EventKind`, which the kind filter offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedKind {
    Opened,
    Commented,
    Reviewed,
    ReviewRequested,
    Assigned,
    Mentioned,
    Committed,
    Merged,
    Closed,
    Reopened,
    CiFailed,
    CiPassed,
}

impl FeedKind {
    pub const ALL: [FeedKind; 12] = [
        Self::Opened,
        Self::Commented,
        Self::Reviewed,
        Self::ReviewRequested,
        Self::Assigned,
        Self::Mentioned,
        Self::Committed,
        Self::Merged,
        Self::Closed,
        Self::Reopened,
        Self::CiFailed,
        Self::CiPassed,
    ];

    pub fn tag(self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Commented => "commented",
            Self::Reviewed => "reviewed",
            Self::ReviewRequested => "review_requested",
            Self::Assigned => "assigned",
            Self::Mentioned => "mentioned",
            Self::Committed => "committed",
            Self::Merged => "merged",
            Self::Closed => "closed",
            Self::Reopened => "reopened",
            Self::CiFailed => "ci_failed",
            Self::CiPassed => "ci_passed",
        }
    }

    pub fn parse(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.tag() == tag)
    }

    /// The event kind stored under this tag. A review needs its state.
    pub fn event_kind(self, review_state: Option<ReviewState>) -> Option<EventKind> {
        Some(match self {
            Self::Reviewed => EventKind::Reviewed(review_state?),
            Self::Opened => EventKind::Opened,
            Self::Commented => EventKind::Commented,
            Self::ReviewRequested => EventKind::ReviewRequested,
            Self::Assigned => EventKind::Assigned,
            Self::Mentioned => EventKind::Mentioned,
            Self::Committed => EventKind::Committed,
            Self::Merged => EventKind::Merged,
            Self::Closed => EventKind::Closed,
            Self::Reopened => EventKind::Reopened,
            Self::CiFailed => EventKind::CiFailed,
            Self::CiPassed => EventKind::CiPassed,
        })
    }
}

/// The place after the last event of a page. Never an offset, so an event
/// that arrives meanwhile moves no row between pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FeedCursor {
    pub at: Timestamp,
    pub id: String,
}

impl FeedCursor {
    /// `<at>_<id>`, with `at` in epoch seconds. An event id may hold `_`, and
    /// `at` never does, so the first `_` ends `at`.
    pub fn parse(raw: &str) -> Result<Self, FeedError> {
        let bad = || FeedError::BadCursor(raw.to_owned());
        let (at, id) = raw.split_once('_').ok_or_else(bad)?;
        let digits = !at.is_empty() && at.bytes().all(|b| b.is_ascii_digit());
        let id_ok = !id.is_empty()
            && id.len() <= MAX_CURSOR_ID_LEN
            && id.bytes().all(|b| {
                b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'=' | b':' | b'.')
            });
        if !digits || !id_ok {
            return Err(bad());
        }
        let at = at.parse::<i64>().map_err(|_| bad())?;
        Ok(Self {
            at: Timestamp(at),
            id: id.to_owned(),
        })
    }

    /// Whether an event sorts after this cursor in `occurred_at DESC, id DESC`
    /// order, which is the SQL's `(occurred_at, id) < (at, id)`.
    pub fn admits(&self, at: Timestamp, id: &str) -> bool {
        (at.0, id) < (self.at.0, self.id.as_str())
    }
}

impl fmt::Display for FeedCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", self.at.0, self.id)
    }
}

impl TryFrom<String> for FeedCursor {
    type Error = FeedError;
    fn try_from(raw: String) -> Result<Self, FeedError> {
        Self::parse(&raw)
    }
}

impl From<FeedCursor> for String {
    fn from(cursor: FeedCursor) -> Self {
        cursor.to_string()
    }
}

/// What `GET /api/feed` was asked for. Parsed once at the boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedQuery {
    pub repo: Option<RepoName>,
    pub kind: Option<FeedKind>,
    pub before: Option<FeedCursor>,
}

impl FeedQuery {
    /// Reads decoded `name`, `value` pairs. An empty value means no filter, and
    /// a name the feed does not know is ignored.
    pub fn parse<'a>(
        pairs: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self, FeedError> {
        let mut query = Self::default();
        for (name, value) in pairs {
            if value.is_empty() {
                continue;
            }
            match name {
                "repo" => {
                    query.repo = Some(
                        RepoName::parse(value).map_err(|_| FeedError::BadRepo(value.to_owned()))?,
                    );
                }
                "kind" => {
                    query.kind = Some(
                        FeedKind::parse(value)
                            .ok_or_else(|| FeedError::BadKind(value.to_owned()))?,
                    );
                }
                "before" => query.before = Some(FeedCursor::parse(value)?),
                _ => {}
            }
        }
        Ok(query)
    }
}

/// One event with the item it happened on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedEvent {
    pub event: Event,
    pub title: String,
    pub number: u32,
    pub url: String,
}

/// One repo in the filter list, with the events it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedRepo {
    pub repo: RepoName,
    pub events: u32,
}

/// `repos` is set on the first page only, and its counts add up to every event.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedPage {
    pub events: Vec<FeedEvent>,
    pub next: Option<FeedCursor>,
    pub repos: Vec<FeedRepo>,
}

/// Turns the rows of a `PAGE_SIZE + 1` read into a page. A row past the page
/// means more exist, and the cursor names the page's last event.
pub fn split_page(mut rows: Vec<FeedEvent>) -> (Vec<FeedEvent>, Option<FeedCursor>) {
    if rows.len() <= PAGE_SIZE {
        return (rows, None);
    }
    rows.truncate(PAGE_SIZE);
    let next = rows.last().map(|last| FeedCursor {
        at: last.event.occurred_at,
        id: last.event.id.as_str().to_owned(),
    });
    (rows, next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventId;

    fn pairs<'a>(query: &'a [(&'a str, &'a str)]) -> Result<FeedQuery, FeedError> {
        FeedQuery::parse(query.iter().copied())
    }

    #[test]
    fn an_empty_query_has_no_filter() {
        assert_eq!(pairs(&[]), Ok(FeedQuery::default()));
        assert_eq!(
            pairs(&[("repo", ""), ("kind", ""), ("before", ""), ("zzz", "1")]),
            Ok(FeedQuery::default())
        );
    }

    #[test]
    fn a_full_query_parses() {
        let query = pairs(&[
            ("repo", "octo/widgets"),
            ("kind", "ci_failed"),
            ("before", "1793000000_ci:PR_1:abc:failed"),
        ])
        .expect("query");
        assert_eq!(
            query.repo.map(|r| r.to_string()),
            Some("octo/widgets".into())
        );
        assert_eq!(query.kind, Some(FeedKind::CiFailed));
        let before = query.before.expect("cursor");
        assert_eq!(
            (before.at, before.id.as_str()),
            (Timestamp(1_793_000_000), "ci:PR_1:abc:failed")
        );
    }

    #[test]
    fn an_unknown_kind_is_rejected() {
        assert_eq!(
            pairs(&[("kind", "nope")]),
            Err(FeedError::BadKind("nope".into()))
        );
        assert!(pairs(&[("kind", "Merged")]).is_err());
    }

    #[test]
    fn a_repo_with_a_quote_or_a_space_is_rejected() {
        for bad in [
            "x'+OR+1=1--",
            "x' OR 1=1--",
            "octo/wid gets",
            "octo",
            "a/b/c",
            "o/r\"",
        ] {
            assert!(pairs(&[("repo", bad)]).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_cursor_must_be_at_underscore_id() {
        for bad in [
            "abc", "123", "123_", "_abc", "-5_x", "12a_x", "1_a b", "1_a'b", "1_;drop",
        ] {
            assert!(pairs(&[("before", bad)]).is_err(), "{bad}");
        }
        let long = format!("1_{}", "x".repeat(MAX_CURSOR_ID_LEN + 1));
        assert!(pairs(&[("before", &long)]).is_err());
    }

    #[test]
    fn a_cursor_round_trips_through_its_text() {
        let cursor = FeedCursor::parse("1793000000_open:PR_a_b").expect("cursor");
        assert_eq!(cursor.to_string(), "1793000000_open:PR_a_b");
        let json = serde_json::to_string(&cursor).expect("json");
        assert_eq!(json, r#""1793000000_open:PR_a_b""#);
        assert_eq!(
            serde_json::from_str::<FeedCursor>(&json).expect("back"),
            cursor
        );
    }

    #[test]
    fn every_kind_tag_round_trips() {
        for kind in FeedKind::ALL {
            assert_eq!(FeedKind::parse(kind.tag()), Some(kind));
            let event = kind.event_kind(Some(ReviewState::Approved)).expect("kind");
            assert_eq!(event.tag(), kind.tag());
        }
        assert_eq!(FeedKind::Reviewed.event_kind(None), None);
    }

    fn event(n: usize, at: i64) -> FeedEvent {
        FeedEvent {
            event: Event {
                id: EventId::for_node(&format!("IC_{n:03}")),
                item: "PR_1".into(),
                repo: RepoName::parse("octo/widgets").expect("repo"),
                kind: EventKind::Commented,
                actor: None,
                occurred_at: Timestamp(at),
            },
            title: "t".into(),
            number: 1,
            url: String::new(),
        }
    }

    /// What the SQL does: sort, keep what follows the cursor, read one extra.
    fn read(all: &[FeedEvent], before: Option<&FeedCursor>) -> Vec<FeedEvent> {
        let mut rows: Vec<FeedEvent> = all
            .iter()
            .filter(|e| before.is_none_or(|c| c.admits(e.event.occurred_at, e.event.id.as_str())))
            .cloned()
            .collect();
        rows.sort_by(|a, b| {
            (b.event.occurred_at.0, b.event.id.as_str())
                .cmp(&(a.event.occurred_at.0, a.event.id.as_str()))
        });
        rows.truncate(PAGE_SIZE + 1);
        rows
    }

    #[test]
    fn keyset_paging_has_no_duplicate_and_no_gap() {
        // Every third event shares its time with the one before.
        let all: Vec<FeedEvent> = (0..120)
            .map(|n| event(n, 1_793_000_000 + (n as i64 / 3) * 60))
            .collect();
        let mut seen = Vec::new();
        let mut sizes = Vec::new();
        let mut cursor: Option<FeedCursor> = None;
        loop {
            let (page, next) = split_page(read(&all, cursor.as_ref()));
            sizes.push(page.len());
            seen.extend(page.iter().map(|e| e.event.id.as_str().to_owned()));
            match next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(sizes, [50, 50, 20]);
        assert_eq!(seen.len(), 120);
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 120);
        let mut expected: Vec<(i64, String)> = all
            .iter()
            .map(|e| (e.event.occurred_at.0, e.event.id.as_str().to_owned()))
            .collect();
        expected.sort_by(|a, b| b.cmp(a));
        let expected: Vec<String> = expected.into_iter().map(|(_, id)| id).collect();
        assert_eq!(seen, expected);
    }

    #[test]
    fn a_page_that_ends_the_list_has_no_cursor() {
        let all: Vec<FeedEvent> = (0..PAGE_SIZE).map(|n| event(n, n as i64)).collect();
        let (page, next) = split_page(read(&all, None));
        assert_eq!((page.len(), next), (PAGE_SIZE, None));
    }
}
