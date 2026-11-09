//! `email_state`, the time of the last GitHub notification email. The row holds
//! a time and a reason word, never the message. `domain::email_health` reads it.

use domain::email::EmailReason;
use domain::ids::Timestamp;
use serde::Deserialize;
use worker::{D1Database, Result};

use super::sql::{run, Param, Statement};

/// `MAX` keeps a repeated or late write from moving the time backwards, and the
/// reason follows the time.
const RECORD: &str = "INSERT INTO email_state (id, last_email_at, last_reason) \
     VALUES (1, ?1, ?2) \
     ON CONFLICT (id) DO UPDATE SET \
     last_reason = CASE WHEN excluded.last_email_at >= email_state.last_email_at \
       THEN excluded.last_reason ELSE email_state.last_reason END, \
     last_email_at = MAX(email_state.last_email_at, excluded.last_email_at)";

const READ: &str = "SELECT last_email_at FROM email_state WHERE id = 1";

const THREADS_AFTER: &str = "SELECT COUNT(*) AS n FROM notification_threads \
     WHERE ?1 IS NULL OR updated_at > ?1";

pub fn record_email(at: Timestamp, reason: EmailReason) -> Statement {
    Statement {
        sql: RECORD,
        params: vec![Param::ts(at), Param::text(reason.as_str())],
    }
}

pub fn read_statement() -> Statement {
    Statement {
        sql: READ,
        params: Vec::new(),
    }
}

/// Counts the threads whose `updated_at` is later than `at`, or all of them
/// when no email has come.
pub fn threads_after_statement(at: Option<Timestamp>) -> Statement {
    Statement {
        sql: THREADS_AFTER,
        params: vec![Param::opt_ts(at)],
    }
}

#[derive(Deserialize)]
struct LastEmail {
    last_email_at: i64,
}

#[derive(Deserialize)]
struct Count {
    n: i64,
}

pub async fn record(db: &D1Database, at: Timestamp, reason: EmailReason) -> Result<()> {
    run(db, vec![record_email(at, reason)]).await.map(|_| ())
}

pub async fn read(db: &D1Database) -> Result<Option<Timestamp>> {
    let rows = read_statement()
        .prepare(db)?
        .all()
        .await?
        .results::<LastEmail>()?;
    Ok(rows.first().map(|row| Timestamp(row.last_email_at)))
}

pub async fn threads_after(db: &D1Database, at: Option<Timestamp>) -> Result<u32> {
    let rows = threads_after_statement(at)
        .prepare(db)?
        .all()
        .await?
        .results::<Count>()?;
    let n = rows.first().map_or(0, |row| row.n);
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};
    use crate::db::threads::{self, SubjectKind, ThreadRow};
    use domain::ids::RepoName;
    use rusqlite::Connection;

    fn thread(id: &str, updated_at: i64) -> ThreadRow {
        ThreadRow {
            id: id.to_owned(),
            repo: RepoName::parse("octo/widgets").expect("repo"),
            subject_kind: SubjectKind::Issue,
            subject_title: "t".to_owned(),
            subject_number: Some(1),
            item_node_id: None,
            reason: "mention".to_owned(),
            updated_at: Timestamp(updated_at),
            synced_at: Timestamp(updated_at),
        }
    }

    fn stored(conn: &Connection) -> (i64, String) {
        let rows = query_json(
            conn,
            "SELECT last_email_at, last_reason FROM email_state",
            &[],
        );
        (
            rows[0]["last_email_at"].as_i64().expect("time"),
            rows[0]["last_reason"].as_str().expect("reason").to_owned(),
        )
    }

    fn after(conn: &Connection, at: Option<i64>) -> i64 {
        let stmt = threads_after_statement(at.map(Timestamp));
        query_json(conn, stmt.sql, &stmt.params)[0]["n"]
            .as_i64()
            .expect("count")
    }

    #[test]
    fn email_state_keeps_one_row_and_the_latest_time() {
        let conn = memory_db();
        exec(&conn, &record_email(Timestamp(100), EmailReason::Mention)).expect("first");
        exec(&conn, &record_email(Timestamp(100), EmailReason::Mention)).expect("duplicate");
        assert_eq!(count_rows(&conn, "email_state"), 1);
        assert_eq!(stored(&conn), (100, "mention".to_owned()));

        exec(
            &conn,
            &record_email(Timestamp(200), EmailReason::ReviewRequested),
        )
        .expect("newer");
        assert_eq!(stored(&conn), (200, "review_requested".to_owned()));

        exec(&conn, &record_email(Timestamp(150), EmailReason::Comment)).expect("late");
        assert_eq!(stored(&conn), (200, "review_requested".to_owned()));
        assert_eq!(count_rows(&conn, "email_state"), 1);
    }

    #[test]
    fn email_state_read_is_empty_until_an_email_is_recorded() {
        let conn = memory_db();
        let read = read_statement();
        assert!(query_json(&conn, read.sql, &read.params).is_empty());
        exec(&conn, &record_email(Timestamp(5), EmailReason::Assign)).expect("record");
        let rows = query_json(&conn, read.sql, &read.params);
        assert_eq!(rows[0]["last_email_at"].as_i64(), Some(5));
    }

    #[test]
    fn email_state_threads_after_counts_strictly_newer_threads() {
        let conn = memory_db();
        for (id, at) in [("1", 100), ("2", 200), ("3", 300)] {
            exec(&conn, &threads::upsert(&thread(id, at))).expect("thread");
        }
        assert_eq!(after(&conn, None), 3);
        assert_eq!(after(&conn, Some(99)), 3);
        assert_eq!(after(&conn, Some(100)), 2);
        assert_eq!(after(&conn, Some(300)), 0);
    }

    #[test]
    fn email_state_table_rejects_a_second_row() {
        let conn = memory_db();
        let err = conn
            .execute(
                "INSERT INTO email_state (id, last_email_at, last_reason) VALUES (2, 1, 'x')",
                [],
            )
            .expect_err("only id 1 is allowed");
        assert!(err.to_string().contains("CHECK"), "{err}");
    }
}
