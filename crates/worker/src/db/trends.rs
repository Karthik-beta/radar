//! The batch behind `GET /api/trends`: my pull requests, my events, and the
//! weekly rollups, each an indexed select over the 52 week window.

use domain::ids::{RepoName, Timestamp};
use domain::rollup::WeekStart;
use domain::trends::{window_start, MyEvent, PrFact, RolledWeek, WeekSource};
use serde::Deserialize;
use worker::{D1Database, Result};

use super::rows::{self, RowError};
use super::sql::{run, Param, Statement};

const PRS: &str = "SELECT created_at, CASE WHEN state = 'merged' THEN closed_at END AS merged_at \
     FROM items WHERE kind = 'pr' AND authored_by_me = 1 \
     AND (created_at >= ?1 OR closed_at >= ?1)";

const EVENTS: &str =
    "SELECT repo, occurred_at FROM events WHERE actor_is_me = 1 AND occurred_at >= ?1";

const ROLLUPS: &str = "SELECT week_start, prs_opened, prs_merged, repos_touched, source \
     FROM rollups_weekly WHERE week_start >= ?1";

pub fn statements(now: Timestamp) -> Vec<Statement> {
    let since = window_start(now);
    [PRS, EVENTS, ROLLUPS]
        .into_iter()
        .map(|sql| Statement {
            sql,
            params: vec![Param::ts(since)],
        })
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct RawPr {
    created_at: i64,
    merged_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct RawEvent {
    repo: String,
    occurred_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct RawRollup {
    week_start: i64,
    prs_opened: i64,
    prs_merged: i64,
    repos_touched: i64,
    source: String,
}

impl RawPr {
    pub fn parse(self) -> PrFact {
        PrFact {
            opened_at: rows::ts(self.created_at),
            merged_at: self.merged_at.map(rows::ts),
        }
    }
}

impl RawEvent {
    pub fn parse(self) -> Result<MyEvent, RowError> {
        Ok(MyEvent {
            repo: RepoName::parse(&self.repo).map_err(|source| RowError::BadId {
                column: "repo",
                source,
            })?,
            at: rows::ts(self.occurred_at),
        })
    }
}

impl RawRollup {
    pub fn parse(self) -> Result<RolledWeek, RowError> {
        let week = WeekStart::new(rows::ts(self.week_start)).map_err(|_| RowError::BadNumber {
            column: "week_start",
            value: self.week_start,
        })?;
        let source = WeekSource::parse(&self.source).ok_or(RowError::UnknownTag {
            column: "source",
            value: self.source,
        })?;
        Ok(RolledWeek {
            week,
            opened: rows::count("prs_opened", self.prs_opened)?,
            merged: rows::count("prs_merged", self.prs_merged)?,
            repos: rows::count("repos_touched", self.repos_touched)?,
            source,
        })
    }
}

#[derive(Debug, Default)]
pub struct RawTrends {
    pub prs: Vec<RawPr>,
    pub events: Vec<RawEvent>,
    pub rollups: Vec<RawRollup>,
}

pub async fn read(db: &D1Database, now: Timestamp) -> Result<RawTrends> {
    let results = run(db, statements(now)).await?;
    let missing = || worker::Error::RustError("trends batch returned too few results".into());
    Ok(RawTrends {
        prs: results.first().ok_or_else(missing)?.results()?,
        events: results.get(1).ok_or_else(missing)?.results()?,
        rollups: results.get(2).ok_or_else(missing)?.results()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{memory_db, query_json};

    const NOW: Timestamp = Timestamp(1_792_000_000);

    fn insert_item(conn: &rusqlite::Connection, id: &str, mine: i64, state: &str, created: i64) {
        let closed = if state == "merged" || state == "closed" {
            created + 100
        } else {
            0
        };
        conn.execute(
            "INSERT INTO items (node_id, repo, number, kind, title, url, author_login, \
             author_kind, authored_by_me, state, created_at, updated_at, closed_at, turn, since, \
             synced_at) VALUES (?1, 'o/one', abs(random() % 100000) + 1, 'pr', 't', 'u', 'me', \
             'user', ?2, ?3, ?4, ?4, NULLIF(?5, 0), 'them', 0, 0)",
            rusqlite::params![id, mine, state, created, closed],
        )
        .expect("item");
    }

    #[test]
    fn the_prs_select_reads_my_pull_requests_and_only_merged_ones_get_a_merge_time() {
        let conn = memory_db();
        let since = window_start(NOW).0;
        insert_item(&conn, "a", 1, "merged", since + 10);
        insert_item(&conn, "b", 1, "closed", since + 10);
        insert_item(&conn, "c", 0, "merged", since + 10);
        insert_item(&conn, "d", 1, "open", since - 1_000);
        let stmt = &statements(NOW)[0];
        let got = query_json(&conn, stmt.sql, &stmt.params);
        assert_eq!(got.len(), 2);
        let merged: Vec<_> = got.iter().filter(|r| !r["merged_at"].is_null()).collect();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["merged_at"], since + 110);
    }

    #[test]
    fn events_by_me_only() {
        let conn = memory_db();
        let since = window_start(NOW).0;
        for (id, mine, at) in [("a", 1, since), ("b", 0, since + 5), ("c", 1, since - 1)] {
            conn.execute(
                "INSERT INTO events (id, item_node_id, repo, kind, actor_login, actor_is_me, \
                 actor_is_bot, occurred_at, source, ingested_at) \
                 VALUES (?1, 'PR_1', 'o/one', 'commented', 'me', ?2, 0, ?3, 'poll', 0)",
                rusqlite::params![id, mine, at],
            )
            .expect("event");
        }
        let stmt = &statements(NOW)[1];
        let got = query_json(&conn, stmt.sql, &stmt.params);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["occurred_at"], since);
    }

    #[test]
    fn rollup_rows_parse_with_their_source() {
        let conn = memory_db();
        let since = window_start(NOW).0;
        conn.execute(
            "INSERT INTO rollups_weekly (week_start, prs_opened, prs_merged, repos_touched, \
             events, source) VALUES (?1, 3, 2, 1, 9, 'seeded'), (?2, 1, 1, 1, 1, 'rolled')",
            rusqlite::params![since, since - 604_800],
        )
        .expect("rollups");
        let stmt = &statements(NOW)[2];
        let got = query_json(&conn, stmt.sql, &stmt.params);
        assert_eq!(got.len(), 1);
        let raw: RawRollup =
            serde_json::from_value(serde_json::Value::Object(got[0].clone())).expect("row");
        let week = raw.parse().expect("parse");
        assert_eq!((week.opened, week.source), (3, WeekSource::Seeded));
    }

    #[test]
    fn a_bad_source_is_a_row_error() {
        let raw = RawRollup {
            week_start: 345_600,
            prs_opened: 0,
            prs_merged: 0,
            repos_touched: 0,
            source: "other".into(),
        };
        assert!(raw.parse().is_err());
    }
}
