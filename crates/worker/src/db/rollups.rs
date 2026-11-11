//! `rollups_weekly` and the `events` selects and delete that feed it.

pub use domain::rollup::WeekStart;
use serde::Deserialize;

use super::sql::{Param, Statement};

/// A week folded in twice adds up, so an event that arrives late for a rolled
/// week is not lost. `repos_touched` keeps the larger count, because two
/// distinct counts cannot be added.
const UPSERT: &str = "INSERT INTO rollups_weekly (week_start, prs_opened, prs_merged, \
     repos_touched, events) VALUES (?1, ?2, ?3, ?4, ?5) \
     ON CONFLICT (week_start) DO UPDATE SET \
     prs_opened = prs_opened + excluded.prs_opened, \
     prs_merged = prs_merged + excluded.prs_merged, \
     repos_touched = MAX(repos_touched, excluded.repos_touched), \
     events = events + excluded.events";

const OLDEST_BEFORE: &str = "SELECT MIN(occurred_at) AS at FROM events WHERE occurred_at < ?1";

const WEEK_TOTALS: &str = "SELECT COUNT(*) AS events, \
     COALESCE(SUM(kind = 'opened' AND item_node_id IN \
       (SELECT node_id FROM items WHERE kind = 'pr')), 0) AS prs_opened, \
     COALESCE(SUM(kind = 'merged' AND item_node_id IN \
       (SELECT node_id FROM items WHERE kind = 'pr')), 0) AS prs_merged, \
     COUNT(DISTINCT repo) AS repos_touched \
     FROM events WHERE occurred_at >= ?1 AND occurred_at < ?2";

const DELETE_WEEK: &str = "DELETE FROM events WHERE occurred_at >= ?1 AND occurred_at < ?2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct RollupTotals {
    pub events: u32,
    pub prs_opened: u32,
    pub prs_merged: u32,
    pub repos_touched: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollupRow {
    pub week_start: WeekStart,
    pub totals: RollupTotals,
}

pub fn upsert(row: &RollupRow) -> Statement {
    let totals = row.totals;
    Statement {
        sql: UPSERT,
        params: vec![
            Param::ts(row.week_start.timestamp()),
            Param::Int(i64::from(totals.prs_opened)),
            Param::Int(i64::from(totals.prs_merged)),
            Param::Int(i64::from(totals.repos_touched)),
            Param::Int(i64::from(totals.events)),
        ],
    }
}

/// The time of the oldest event before `cutoff`, if any.
pub fn oldest_before(cutoff: domain::ids::Timestamp) -> Statement {
    Statement {
        sql: OLDEST_BEFORE,
        params: vec![Param::ts(cutoff)],
    }
}

pub fn week_totals(week: WeekStart) -> Statement {
    Statement {
        sql: WEEK_TOTALS,
        params: vec![Param::ts(week.timestamp()), Param::ts(week.end())],
    }
}

pub fn delete_week(week: WeekStart) -> Statement {
    Statement {
        sql: DELETE_WEEK,
        params: vec![Param::ts(week.timestamp()), Param::ts(week.end())],
    }
}

#[cfg(test)]
mod tests {
    use domain::ids::Timestamp;

    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};

    const FIRST_MONDAY: i64 = 345_600;

    fn week() -> WeekStart {
        WeekStart::new(Timestamp(FIRST_MONDAY)).expect("monday")
    }

    fn totals(events: u32) -> RollupTotals {
        RollupTotals {
            events,
            prs_opened: 3,
            prs_merged: 2,
            repos_touched: 1,
        }
    }

    #[test]
    fn a_second_fold_adds_to_the_week() {
        let conn = memory_db();
        let row = RollupRow {
            week_start: week(),
            totals: totals(10),
        };
        exec(&conn, &upsert(&row)).expect("first");
        exec(
            &conn,
            &upsert(&RollupRow {
                totals: RollupTotals {
                    repos_touched: 4,
                    ..totals(2)
                },
                ..row
            }),
        )
        .expect("second");
        assert_eq!(count_rows(&conn, "rollups_weekly"), 1);
        let rows = query_json(
            &conn,
            "SELECT events, prs_opened, repos_touched FROM rollups_weekly",
            &[],
        );
        assert_eq!(rows[0]["events"], 12);
        assert_eq!(rows[0]["prs_opened"], 6);
        assert_eq!(rows[0]["repos_touched"], 4);
    }

    #[test]
    fn the_table_refuses_a_week_that_starts_midweek() {
        let conn = memory_db();
        let mut stmt = upsert(&RollupRow {
            week_start: week(),
            totals: totals(0),
        });
        stmt.params[0] = Param::Int(FIRST_MONDAY + 1);
        assert!(exec(&conn, &stmt).is_err());
    }

    fn seed(conn: &rusqlite::Connection, id: &str, repo: &str, kind: &str, item: &str, at: i64) {
        conn.execute(
            "INSERT INTO events (id, item_node_id, repo, kind, actor_login, actor_is_me, \
             actor_is_bot, occurred_at, source, ingested_at) \
             VALUES (?1, ?2, ?3, ?4, 'alice', 0, 0, ?5, 'poll', 0)",
            rusqlite::params![id, item, repo, kind, at],
        )
        .expect("event");
    }

    fn seed_pr(conn: &rusqlite::Connection, node_id: &str) {
        let mut row = crate::db::synthetic::items(8)
            .expect("items")
            .into_iter()
            .find(|row| row.item_ref.kind == domain::ids::ItemKind::Pr)
            .expect("a pull request");
        row.node_id = node_id.to_owned();
        exec(conn, &crate::db::items::upsert(&row)).expect("item");
    }

    #[test]
    fn totals_count_one_week_and_only_pull_requests() {
        let conn = memory_db();
        seed_pr(&conn, "PR_1");
        let day = 86_400;
        let (start, next) = (FIRST_MONDAY, FIRST_MONDAY + 7 * day);
        seed(&conn, "a", "o/one", "opened", "PR_1", start);
        seed(&conn, "b", "o/one", "merged", "PR_1", start + day);
        seed(&conn, "c", "o/two", "opened", "I_1", start + 2 * day);
        seed(&conn, "d", "o/two", "commented", "I_1", next - 1);
        seed(&conn, "e", "o/two", "commented", "I_1", next);
        let rows = query_json(&conn, WEEK_TOTALS, &week_totals(week()).params);
        assert_eq!(rows[0]["events"], 4);
        assert_eq!(rows[0]["prs_opened"], 1);
        assert_eq!(rows[0]["prs_merged"], 1);
        assert_eq!(rows[0]["repos_touched"], 2);
        exec(&conn, &delete_week(week())).expect("delete");
        assert_eq!(count_rows(&conn, "events"), 1);
    }

    #[test]
    fn the_oldest_event_before_the_cutoff_names_the_next_week() {
        let conn = memory_db();
        let none = query_json(
            &conn,
            OLDEST_BEFORE,
            &oldest_before(Timestamp(i64::from(i32::MAX))).params,
        );
        assert!(none[0]["at"].is_null());
        seed(&conn, "a", "o/one", "commented", "PR_1", 5_000_000);
        seed(&conn, "b", "o/one", "commented", "PR_1", 9_000_000);
        let rows = query_json(
            &conn,
            OLDEST_BEFORE,
            &oldest_before(Timestamp(8_000_000)).params,
        );
        assert_eq!(rows[0]["at"], 5_000_000);
        let rows = query_json(
            &conn,
            OLDEST_BEFORE,
            &oldest_before(Timestamp(5_000_000)).params,
        );
        assert!(rows[0]["at"].is_null());
    }
}
