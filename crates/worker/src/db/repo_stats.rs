//! The reads behind the repo cards. Counts and first-reply times come from
//! `items` and `events` on every read, so they cannot drift from the events.

use domain::ids::RepoName;
use domain::repo_stats::{cards, RepoCard, RepoCounts};
use serde::Deserialize;
use worker::{D1Database, Result};

use super::rows::{self, RowError};
use super::sql::{run, Statement};

/// My pull requests by outcome in each repo. `last_activity` is the latest
/// update of any item Radar holds in the repo.
const COUNTS: &str = "SELECT repo, \
     COALESCE(SUM(kind = 'pr' AND authored_by_me = 1 AND state IN ('open', 'draft')), 0) AS open, \
     COALESCE(SUM(kind = 'pr' AND authored_by_me = 1 AND state = 'merged'), 0) AS merged, \
     COALESCE(SUM(kind = 'pr' AND authored_by_me = 1 AND state = 'closed'), 0) AS closed, \
     MAX(updated_at) AS last_activity \
     FROM items GROUP BY repo";

/// One row per pull request of mine with a reply: seconds from opening to the
/// first comment or review by a human who is not me. It applies the rule of
/// `domain::repo_stats::first_response`.
const RESPONSES: &str = "SELECT items.repo AS repo, \
     MIN(events.occurred_at) - items.created_at AS secs \
     FROM items JOIN events ON events.item_node_id = items.node_id \
     WHERE items.kind = 'pr' AND items.authored_by_me = 1 \
     AND events.kind IN ('commented', 'reviewed') \
     AND events.actor_is_me = 0 AND events.actor_is_bot = 0 \
     AND events.occurred_at >= items.created_at \
     GROUP BY items.node_id";

pub fn statements() -> Vec<Statement> {
    [COUNTS, RESPONSES]
        .into_iter()
        .map(|sql| Statement {
            sql,
            params: Vec::new(),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct RawCounts {
    repo: String,
    open: i64,
    merged: i64,
    closed: i64,
    last_activity: i64,
}

#[derive(Debug, Deserialize)]
pub struct RawResponse {
    repo: String,
    secs: i64,
}

#[derive(Debug, Default)]
pub struct RawRepoStats {
    pub counts: Vec<RawCounts>,
    pub responses: Vec<RawResponse>,
}

impl RawRepoStats {
    /// Validates every row and drops the ones that do not parse, so one bad
    /// row cannot blank the page.
    pub fn parse(self) -> (Vec<RepoCard>, Vec<RowError>) {
        let mut errors = Vec::new();
        let bad_repo = |source| RowError::BadId {
            column: "repo",
            source,
        };
        let counts = self
            .counts
            .into_iter()
            .filter_map(|c| {
                let parsed = (|| {
                    Ok(RepoCounts {
                        repo: RepoName::parse(&c.repo).map_err(bad_repo)?,
                        open: rows::count("open", c.open)?,
                        merged: rows::count("merged", c.merged)?,
                        closed: rows::count("closed", c.closed)?,
                        last_activity: rows::ts(c.last_activity),
                    })
                })();
                parsed.map_err(|e: RowError| errors.push(e)).ok()
            })
            .collect();
        let responses: Vec<(RepoName, i64)> = self
            .responses
            .into_iter()
            .filter_map(|r| {
                let repo = RepoName::parse(&r.repo).map_err(bad_repo);
                match repo {
                    Ok(repo) if r.secs >= 0 => Some((repo, r.secs)),
                    Ok(_) => {
                        errors.push(RowError::BadNumber {
                            column: "secs",
                            value: r.secs,
                        });
                        None
                    }
                    Err(e) => {
                        errors.push(e);
                        None
                    }
                }
            })
            .collect();
        (cards(counts, &responses), errors)
    }
}

pub async fn read(db: &D1Database) -> Result<RawRepoStats> {
    let results = run(db, statements()).await?;
    let mut results = results.iter();
    let mut next = || {
        results
            .next()
            .ok_or_else(|| worker::Error::RustError("repo stats batch came back short".into()))
    };
    let counts = next()?.results()?;
    let responses = next()?.results()?;
    Ok(RawRepoStats { counts, responses })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{explain, memory_db, query_json};

    const DAY: i64 = 86_400;

    fn item(conn: &rusqlite::Connection, n: i64, repo: &str, state: &str, created: i64) {
        let closed = (state == "merged" || state == "closed").then_some(created + 1);
        conn.execute(
            "INSERT INTO items (node_id, repo, number, kind, title, url, author_login, \
             author_kind, authored_by_me, state, created_at, updated_at, closed_at, turn, \
             since, synced_at) \
             VALUES (?1, ?2, ?3, 'pr', 't', 'https://github.com/o/r/pull/1', 'me', 'user', 1, \
             ?4, ?5, ?5, ?6, 'them', ?5, 0)",
            rusqlite::params![format!("PR_{n}"), repo, n, state, created, closed],
        )
        .expect("item");
    }

    fn event(
        conn: &rusqlite::Connection,
        id: &str,
        item: i64,
        kind: &str,
        who: (&str, i64, i64),
        at: i64,
    ) {
        conn.execute(
            "INSERT INTO events (id, item_node_id, repo, kind, actor_login, actor_is_me, \
             actor_is_bot, occurred_at, source, ingested_at) \
             VALUES (?1, ?2, 'o/r', ?3, ?4, ?5, ?6, ?7, 'poll', 0)",
            rusqlite::params![id, format!("PR_{item}"), kind, who.0, who.1, who.2, at],
        )
        .expect("event");
    }

    fn raw(conn: &rusqlite::Connection) -> RawRepoStats {
        fn decode<T: serde::de::DeserializeOwned>(
            conn: &rusqlite::Connection,
            stmt: &Statement,
        ) -> Vec<T> {
            query_json(conn, stmt.sql, &stmt.params)
                .into_iter()
                .map(|row| serde_json::from_value(serde_json::Value::Object(row)).expect("row"))
                .collect()
        }
        let statements = statements();
        RawRepoStats {
            counts: decode(conn, &statements[0]),
            responses: decode(conn, &statements[1]),
        }
    }

    #[test]
    fn a_repo_with_three_merged_one_closed_two_open_reads_75_percent() {
        let conn = memory_db();
        for (n, state) in [
            (1, "merged"),
            (2, "merged"),
            (3, "merged"),
            (4, "closed"),
            (5, "open"),
            (6, "draft"),
        ] {
            item(&conn, n, "o/r", state, 1_000 + n);
        }
        let (cards, errors) = raw(&conn).parse();
        assert!(errors.is_empty());
        let card = &cards[0];
        assert_eq!((card.open, card.merged, card.closed), (2, 3, 1));
        assert_eq!(card.merge_rate, Some(domain::repo_stats::Percent(75)));
        assert_eq!(card.median_first_response_secs, None);
    }

    #[test]
    fn replies_of_one_two_four_and_nine_days_read_a_median_of_three() {
        let conn = memory_db();
        for (n, days) in [(1, 1), (2, 2), (3, 4), (4, 9)] {
            item(&conn, n, "o/r", "merged", 1_000);
            event(
                &conn,
                &format!("late{n}"),
                n,
                "commented",
                ("alice", 0, 0),
                1_000 + days * DAY,
            );
            event(
                &conn,
                &format!("later{n}"),
                n,
                "reviewed",
                ("bob", 0, 0),
                1_000 + 20 * DAY,
            );
            event(
                &conn,
                &format!("bot{n}"),
                n,
                "commented",
                ("ci", 0, 1),
                1_001,
            );
            event(
                &conn,
                &format!("me{n}"),
                n,
                "commented",
                ("me", 1, 0),
                1_002,
            );
            event(
                &conn,
                &format!("early{n}"),
                n,
                "commented",
                ("dan", 0, 0),
                500,
            );
        }
        item(&conn, 5, "o/r", "open", 1_000);
        let (cards, _) = raw(&conn).parse();
        assert_eq!(cards[0].median_first_response_secs, Some(3 * DAY));
    }

    #[test]
    fn only_my_pull_requests_count() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO items (node_id, repo, number, kind, title, url, author_login, \
             author_kind, authored_by_me, state, created_at, updated_at, closed_at, turn, \
             reason, since, synced_at) \
             VALUES ('PR_9', 'o/r', 9, 'pr', 't', 'https://github.com/o/r/pull/9', 'zed', \
             'user', 0, 'merged', 1, 1, 2, 'me', 'mentioned', 1, 0)",
            [],
        )
        .expect("theirs");
        let (cards, _) = raw(&conn).parse();
        assert_eq!((cards[0].merged, cards[0].merge_rate), (0, None));
    }

    #[test]
    fn the_reply_join_uses_an_index_on_events() {
        let conn = memory_db();
        let plan = explain(&conn, &statements()[1]).join(" | ");
        assert!(plan.contains("idx_events_item"), "{plan}");
    }
}
