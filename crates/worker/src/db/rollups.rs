//! `rollups_weekly` and the `events` selects and delete that feed it.

pub use domain::rollup::WeekStart;
use serde::Deserialize;

use super::sql::{Param, Statement};

/// A week folded in twice adds up, so an event that arrives late for a rolled
/// week is not lost. `repos_touched` keeps the larger count, because two
/// distinct counts cannot be added. A seeded row is an estimate from GitHub's
/// history, so the first real fold replaces it.
const UPSERT: &str = "INSERT INTO rollups_weekly (week_start, prs_opened, prs_merged, \
     repos_touched, events, source) VALUES (?1, ?2, ?3, ?4, ?5, 'rolled') \
     ON CONFLICT (week_start) DO UPDATE SET \
     prs_opened = CASE WHEN source = 'seeded' THEN excluded.prs_opened \
       ELSE prs_opened + excluded.prs_opened END, \
     prs_merged = CASE WHEN source = 'seeded' THEN excluded.prs_merged \
       ELSE prs_merged + excluded.prs_merged END, \
     repos_touched = CASE WHEN source = 'seeded' THEN excluded.repos_touched \
       ELSE MAX(repos_touched, excluded.repos_touched) END, \
     events = CASE WHEN source = 'seeded' THEN excluded.events \
       ELSE events + excluded.events END, \
     source = 'rolled'";

/// Pages of one seeded week add up. A rolled row is never touched, because its
/// counts come from real events.
const SEED: &str = "INSERT INTO rollups_weekly (week_start, prs_opened, prs_merged, \
     repos_touched, events, source) VALUES (?1, ?2, ?3, ?4, 0, 'seeded') \
     ON CONFLICT (week_start) DO UPDATE SET \
     prs_opened = prs_opened + excluded.prs_opened, \
     prs_merged = prs_merged + excluded.prs_merged, \
     repos_touched = MAX(repos_touched, excluded.repos_touched) \
     WHERE source = 'seeded'";

const OLDEST_BEFORE: &str = "SELECT MIN(occurred_at) AS at FROM events WHERE occurred_at < ?1";

/// Counts only my events on my authored pull requests, as `db/trends.rs` does,
/// so a rolled week matches the live chart. `DELETE_WEEK` still removes every
/// event in the week.
const WEEK_TOTALS: &str = "SELECT COUNT(*) AS events, \
     COALESCE(SUM(kind = 'opened' AND item_node_id IN \
       (SELECT node_id FROM items WHERE kind = 'pr' AND authored_by_me = 1)), 0) AS prs_opened, \
     COALESCE(SUM(kind = 'merged' AND item_node_id IN \
       (SELECT node_id FROM items WHERE kind = 'pr' AND authored_by_me = 1)), 0) AS prs_merged, \
     COUNT(DISTINCT repo) AS repos_touched \
     FROM events WHERE actor_is_me = 1 AND occurred_at >= ?1 AND occurred_at < ?2";

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

pub fn seed(week: WeekStart, totals: RollupTotals) -> Statement {
    Statement {
        sql: SEED,
        params: vec![
            Param::ts(week.timestamp()),
            Param::Int(i64::from(totals.prs_opened)),
            Param::Int(i64::from(totals.prs_merged)),
            Param::Int(i64::from(totals.repos_touched)),
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
    fn a_fold_replaces_a_seeded_week_instead_of_adding_to_it() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO rollups_weekly (week_start, prs_opened, prs_merged, repos_touched, \
             events, source) VALUES (?1, 9, 9, 9, 0, 'seeded')",
            [FIRST_MONDAY],
        )
        .expect("seeded");
        exec(
            &conn,
            &upsert(&RollupRow {
                week_start: week(),
                totals: totals(10),
            }),
        )
        .expect("fold");
        let rows = query_json(
            &conn,
            "SELECT prs_opened, repos_touched, source FROM rollups_weekly",
            &[],
        );
        assert_eq!(rows[0]["prs_opened"], 3);
        assert_eq!(rows[0]["repos_touched"], 1);
        assert_eq!(rows[0]["source"], "rolled");
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
        seed_by(conn, id, repo, kind, item, at, true);
    }

    fn seed_by(
        conn: &rusqlite::Connection,
        id: &str,
        repo: &str,
        kind: &str,
        item: &str,
        at: i64,
        mine: bool,
    ) {
        conn.execute(
            "INSERT INTO events (id, item_node_id, repo, kind, actor_login, actor_is_me, \
             actor_is_bot, occurred_at, source, ingested_at) \
             VALUES (?1, ?2, ?3, ?4, 'alice', ?6, 0, ?5, 'poll', 0)",
            rusqlite::params![id, item, repo, kind, at, i64::from(mine)],
        )
        .expect("event");
    }

    fn seed_pr(conn: &rusqlite::Connection, node_id: &str) {
        seed_pr_by(conn, node_id, true);
    }

    fn seed_pr_by(conn: &rusqlite::Connection, node_id: &str, mine: bool) {
        let mut row = crate::db::synthetic::items(8)
            .expect("items")
            .into_iter()
            .find(|row| row.item_ref.kind == domain::ids::ItemKind::Pr)
            .expect("a pull request");
        row.node_id = node_id.to_owned();
        row.item_ref.number = 1000 + u32::from(mine);
        exec(conn, &crate::db::items::upsert(&row)).expect("item");
        conn.execute(
            "UPDATE items SET authored_by_me = ?2 WHERE node_id = ?1",
            rusqlite::params![node_id, i64::from(mine)],
        )
        .expect("authorship");
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
    fn totals_leave_out_other_peoples_work() {
        let conn = memory_db();
        seed_pr(&conn, "PR_1");
        seed_pr_by(&conn, "PR_2", false);
        let day = 86_400;
        let start = FIRST_MONDAY;
        seed(&conn, "a", "o/one", "opened", "PR_1", start);
        seed(&conn, "b", "o/two", "opened", "PR_2", start + day);
        seed(&conn, "c", "o/two", "merged", "PR_2", start + day);
        seed_by(&conn, "d", "o/three", "opened", "PR_1", start + day, false);
        seed_by(
            &conn,
            "e",
            "o/three",
            "commented",
            "I_1",
            start + day,
            false,
        );
        let rows = query_json(&conn, WEEK_TOTALS, &week_totals(week()).params);
        assert_eq!(rows[0]["events"], 3);
        assert_eq!(rows[0]["prs_opened"], 1);
        assert_eq!(rows[0]["prs_merged"], 0);
        assert_eq!(rows[0]["repos_touched"], 2);
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
