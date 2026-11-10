//! The selects behind `GET /api/feed`. Every value is a bound parameter. The
//! repo and cursor clauses are separate statements so a filtered read can use
//! `idx_events_repo_feed` and an unfiltered one `idx_events_feed`.

use domain::activity::ReviewState;
use domain::event::{Event, EventActor, EventId};
use domain::feed::{FeedEvent, FeedKind, FeedQuery, FeedRepo, PAGE_SIZE};
use domain::ids::{Login, RepoName};
use serde::Deserialize;
use worker::{D1Database, Result};

use super::rows::{self, untag_opt, RowError};
use super::sql::{run, Param, Statement};

macro_rules! page_sql {
    ($repo:literal, $cursor:literal) => {
        concat!(
            "SELECT events.id, events.item_node_id, events.repo, events.kind, \
             events.review_state, events.actor_login, events.actor_is_me, \
             events.actor_is_bot, events.occurred_at, \
             COALESCE(items.title, '') AS title, COALESCE(items.number, 0) AS number, \
             COALESCE(items.url, '') AS url \
             FROM events LEFT JOIN items ON items.node_id = events.item_node_id \
             WHERE (?1 IS NULL OR events.kind = ?1)",
            $repo,
            $cursor,
            " ORDER BY events.occurred_at DESC, events.id DESC LIMIT 51"
        )
    };
}

const PAGE: &str = page_sql!("", "");
const PAGE_REPO: &str = page_sql!(" AND events.repo = ?2", "");
const PAGE_BEFORE: &str = page_sql!("", " AND (events.occurred_at, events.id) < (?2, ?3)");
const PAGE_REPO_BEFORE: &str = page_sql!(
    " AND events.repo = ?2",
    " AND (events.occurred_at, events.id) < (?3, ?4)"
);

const REPOS: &str = "SELECT repo, COUNT(*) AS n FROM events GROUP BY repo ORDER BY repo";

/// One more row than a page, so the caller can tell whether another page exists.
pub const READ_LIMIT: usize = PAGE_SIZE + 1;

pub fn page(query: &FeedQuery) -> Statement {
    let mut params = vec![Param::opt_text(query.kind.map(FeedKind::tag))];
    let repo = query.repo.as_ref().map(ToString::to_string);
    if let Some(repo) = &repo {
        params.push(Param::text(repo));
    }
    if let Some(cursor) = &query.before {
        params.push(Param::ts(cursor.at));
        params.push(Param::text(&cursor.id));
    }
    let sql = match (repo.is_some(), query.before.is_some()) {
        (false, false) => PAGE,
        (true, false) => PAGE_REPO,
        (false, true) => PAGE_BEFORE,
        (true, true) => PAGE_REPO_BEFORE,
    };
    Statement { sql, params }
}

pub fn repos() -> Statement {
    Statement {
        sql: REPOS,
        params: Vec::new(),
    }
}

#[derive(Debug, Deserialize)]
pub struct RawFeedRow {
    id: String,
    item_node_id: String,
    repo: String,
    kind: String,
    review_state: Option<String>,
    actor_login: Option<String>,
    actor_is_me: i64,
    actor_is_bot: i64,
    occurred_at: i64,
    title: String,
    number: i64,
    url: String,
}

impl RawFeedRow {
    pub fn parse(self) -> std::result::Result<FeedEvent, RowError> {
        let bad_id = |column, source| RowError::BadId { column, source };
        let review_state: Option<ReviewState> =
            untag_opt("review_state", self.review_state.as_deref())?;
        let kind = FeedKind::parse(&self.kind)
            .and_then(|kind| kind.event_kind(review_state))
            .ok_or_else(|| RowError::UnknownTag {
                column: "kind",
                value: self.kind.clone(),
            })?;
        let actor = match self.actor_login {
            Some(login) => Some(EventActor {
                login: Login::parse(&login).map_err(|e| bad_id("actor_login", e))?,
                is_me: rows::flag("actor_is_me", self.actor_is_me)?,
                is_bot: rows::flag("actor_is_bot", self.actor_is_bot)?,
            }),
            None => None,
        };
        let number = u32::try_from(self.number).map_err(|_| RowError::BadNumber {
            column: "number",
            value: self.number,
        })?;
        Ok(FeedEvent {
            event: Event {
                id: EventId::from_stored(&self.id),
                item: self.item_node_id,
                repo: RepoName::parse(&self.repo).map_err(|e| bad_id("repo", e))?,
                kind,
                actor,
                occurred_at: rows::ts(self.occurred_at),
            },
            title: self.title,
            number,
            url: self.url,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct RawRepo {
    repo: String,
    n: i64,
}

impl RawRepo {
    pub fn parse(self) -> std::result::Result<FeedRepo, RowError> {
        Ok(FeedRepo {
            repo: RepoName::parse(&self.repo).map_err(|source| RowError::BadId {
                column: "repo",
                source,
            })?,
            events: rows::count("repo_events", self.n)?,
        })
    }
}

#[derive(Debug, Default)]
pub struct RawFeed {
    pub rows: Vec<RawFeedRow>,
    pub repos: Vec<RawRepo>,
}

/// The first page also reads the repo list, in the same D1 batch.
pub async fn read(db: &D1Database, query: &FeedQuery) -> Result<RawFeed> {
    let mut statements = vec![page(query)];
    if query.before.is_none() {
        statements.push(repos());
    }
    let results = run(db, statements).await?;
    let mut results = results.iter();
    let rows = results
        .next()
        .ok_or_else(|| worker::Error::RustError("feed batch returned no page".into()))?
        .results()?;
    let repos = match results.next() {
        Some(result) => result.results()?,
        None => Vec::new(),
    };
    Ok(RawFeed { rows, repos })
}

#[cfg(test)]
mod tests {
    use domain::feed::FeedCursor;
    use domain::ids::Timestamp;

    use super::*;
    use crate::db::testing::{memory_db, query_json};

    fn seed(conn: &rusqlite::Connection, id: &str, repo: &str, kind: &str, at: i64) {
        conn.execute(
            "INSERT INTO events (id, item_node_id, repo, kind, actor_login, actor_is_me, \
             actor_is_bot, occurred_at, source, ingested_at) \
             VALUES (?1, 'PR_1', ?2, ?3, 'alice', 0, 0, ?4, 'poll', 0)",
            rusqlite::params![id, repo, kind, at],
        )
        .expect("event");
    }

    fn ids(conn: &rusqlite::Connection, query: &FeedQuery) -> Vec<String> {
        let stmt = page(query);
        query_json(conn, stmt.sql, &stmt.params)
            .iter()
            .map(|row| row["id"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    fn query(text: &[(&str, &str)]) -> FeedQuery {
        FeedQuery::parse(text.iter().copied()).expect("query")
    }

    #[test]
    fn newest_first_with_ties_broken_by_id() {
        let conn = memory_db();
        seed(&conn, "b", "o/one", "commented", 100);
        seed(&conn, "a", "o/one", "commented", 100);
        seed(&conn, "c", "o/one", "commented", 200);
        assert_eq!(ids(&conn, &query(&[])), ["c", "b", "a"]);
    }

    #[test]
    fn every_filter_combination_reads_the_right_rows() {
        let conn = memory_db();
        seed(&conn, "a", "o/one", "commented", 100);
        seed(&conn, "b", "o/one", "merged", 200);
        seed(&conn, "c", "o/two", "merged", 300);
        seed(&conn, "d", "o/two", "commented", 400);
        assert_eq!(ids(&conn, &query(&[("repo", "o/two")])), ["d", "c"]);
        assert_eq!(ids(&conn, &query(&[("kind", "merged")])), ["c", "b"]);
        assert_eq!(
            ids(&conn, &query(&[("repo", "o/two"), ("kind", "merged")])),
            ["c"]
        );
        assert_eq!(
            ids(&conn, &query(&[("repo", "o/two"), ("kind", "closed")])),
            Vec::<String>::new()
        );
        assert_eq!(ids(&conn, &query(&[("before", "300_c")])), ["b", "a"]);
        assert_eq!(
            ids(&conn, &query(&[("repo", "o/one"), ("before", "200_b")])),
            ["a"]
        );
    }

    #[test]
    fn a_cursor_between_equal_times_neither_repeats_nor_skips() {
        let conn = memory_db();
        for id in ["a", "b", "c", "d"] {
            seed(&conn, id, "o/one", "commented", 100);
        }
        let cursor = FeedCursor {
            at: Timestamp(100),
            id: "c".into(),
        };
        let q = FeedQuery {
            before: Some(cursor),
            ..FeedQuery::default()
        };
        assert_eq!(ids(&conn, &q), ["b", "a"]);
    }

    #[test]
    fn a_page_holds_one_row_more_than_the_page_size() {
        let conn = memory_db();
        for n in 0..60 {
            seed(&conn, &format!("e{n:02}"), "o/one", "commented", n);
        }
        assert_eq!(ids(&conn, &query(&[])).len(), READ_LIMIT);
    }

    #[test]
    fn an_injected_repo_never_reaches_the_sql() {
        let conn = memory_db();
        seed(&conn, "a", "o/one", "commented", 1);
        assert!(FeedQuery::parse([("repo", "x' OR 1=1--")]).is_err());
        let stmt = page(&query(&[("repo", "o/one")]));
        assert!(!stmt.sql.contains("o/one"));
        let repos = repos();
        assert_eq!(query_json(&conn, repos.sql, &repos.params).len(), 1);
    }

    #[test]
    fn rows_parse_into_feed_events() {
        let row = RawFeedRow {
            id: "IC_1".into(),
            item_node_id: "PR_1".into(),
            repo: "o/one".into(),
            kind: "reviewed".into(),
            review_state: Some("approved".into()),
            actor_login: Some("dependabot[bot]".into()),
            actor_is_me: 0,
            actor_is_bot: 1,
            occurred_at: 5,
            title: "Fix".into(),
            number: 7,
            url: "https://github.com/o/one/pull/7".into(),
        };
        let parsed = row.parse().expect("row");
        assert_eq!(parsed.event.kind.tag(), "reviewed");
        assert!(parsed.event.actor.is_some_and(|a| a.is_bot));
        let bad = RawFeedRow {
            kind: "nope".into(),
            review_state: None,
            ..RawFeedRow {
                id: "x".into(),
                item_node_id: "PR_1".into(),
                repo: "o/one".into(),
                kind: String::new(),
                review_state: None,
                actor_login: None,
                actor_is_me: 0,
                actor_is_bot: 0,
                occurred_at: 0,
                title: String::new(),
                number: 0,
                url: String::new(),
            }
        };
        assert!(bad.parse().is_err());
    }
}
