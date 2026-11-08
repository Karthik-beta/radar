//! The `work` and `sync_state` tables in the Syncer's own SQLite. Every write is
//! a `Statement`, so tests run the same SQL against SQLite natively.

use domain::ids::Timestamp;
use domain::sync_status::{Rate, SyncKind};
use serde::Deserialize;

use super::poll::PollPhase;
use super::state::{SyncState, Throttle};
use super::work::{RowState, WorkError, WorkKind, WorkRow};
use crate::db::{Param, Statement};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS work (
  kind TEXT NOT NULL CHECK (kind IN ('poll_notifications', 'enrich', 'backfill_page', 'rollup')),
  key TEXT NOT NULL CHECK (key <> ''),
  payload TEXT,
  due_at INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  dead INTEGER NOT NULL DEFAULT 0 CHECK (dead IN (0, 1)),
  last_error TEXT,
  PRIMARY KEY (kind, key)
);
CREATE TABLE IF NOT EXISTS sync_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  last_modified TEXT,
  next_poll_at INTEGER,
  rl_limit INTEGER,
  rl_remaining INTEGER,
  rl_reset INTEGER,
  scopes TEXT,
  strikes INTEGER NOT NULL DEFAULT 0 CHECK (strikes >= 0),
  retry_at INTEGER,
  no_token INTEGER NOT NULL DEFAULT 0 CHECK (no_token IN (0, 1)),
  last_kick_at INTEGER,
  last_ok_poll_notifications INTEGER,
  last_ok_enrich INTEGER,
  last_ok_backfill_page INTEGER,
  last_ok_rollup INTEGER
);
INSERT OR IGNORE INTO sync_state (id) VALUES (1);
CREATE TABLE IF NOT EXISTS kv (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
);
";

const POLL_PHASE_KEY: &str = "poll_phase";

pub const SELECT_PHASE: &str = "SELECT value FROM kv WHERE key = 'poll_phase'";

pub const SELECT_ROWS: &str =
    "SELECT kind, key, payload, due_at, attempts, dead, last_error FROM work";

pub const SELECT_STATE: &str = "SELECT last_modified, next_poll_at, rl_limit, rl_remaining, \
     rl_reset, scopes, strikes, retry_at, no_token, last_kick_at, last_ok_poll_notifications, \
     last_ok_enrich, last_ok_backfill_page, last_ok_rollup FROM sync_state WHERE id = 1";

#[derive(Debug, Deserialize)]
pub struct RawRow {
    kind: String,
    key: String,
    payload: Option<String>,
    due_at: i64,
    attempts: i64,
    dead: i64,
    last_error: Option<String>,
}

impl RawRow {
    pub fn parse(self) -> Result<WorkRow, WorkError> {
        let kind = WorkKind::from_columns(&self.kind, &self.key, self.payload.as_deref())?;
        Ok(WorkRow {
            kind,
            due_at: Timestamp(self.due_at),
            attempts: u32::try_from(self.attempts).unwrap_or(u32::MAX),
            state: if self.dead == 1 {
                RowState::Dead
            } else {
                RowState::Pending
            },
            last_error: self.last_error,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct RawState {
    last_modified: Option<String>,
    next_poll_at: Option<i64>,
    rl_limit: Option<i64>,
    rl_remaining: Option<i64>,
    rl_reset: Option<i64>,
    scopes: Option<String>,
    strikes: i64,
    retry_at: Option<i64>,
    no_token: i64,
    last_kick_at: Option<i64>,
    last_ok_poll_notifications: Option<i64>,
    last_ok_enrich: Option<i64>,
    last_ok_backfill_page: Option<i64>,
    last_ok_rollup: Option<i64>,
}

impl RawState {
    pub fn parse(self) -> SyncState {
        let count = |n: Option<i64>| n.and_then(|n| u32::try_from(n).ok());
        let last_ok = [
            (SyncKind::PollNotifications, self.last_ok_poll_notifications),
            (SyncKind::Enrich, self.last_ok_enrich),
            (SyncKind::BackfillPage, self.last_ok_backfill_page),
            (SyncKind::Rollup, self.last_ok_rollup),
        ];
        SyncState {
            last_modified: self.last_modified,
            next_poll_at: self.next_poll_at.map(Timestamp),
            rate: Rate {
                limit: count(self.rl_limit),
                remaining: count(self.rl_remaining),
                reset: self.rl_reset.map(Timestamp),
            },
            scopes: self.scopes,
            throttle: Throttle {
                strikes: count(Some(self.strikes)).unwrap_or(0),
                retry_at: self.retry_at.map(Timestamp),
                no_token: self.no_token == 1,
            },
            last_kick_at: self.last_kick_at.map(Timestamp),
            last_ok: last_ok
                .into_iter()
                .filter_map(|(kind, at)| Some((kind, Timestamp(at?))))
                .collect(),
            poll_phase: PollPhase::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RawKv {
    value: String,
}

impl RawKv {
    pub fn parse_phase(&self) -> Option<PollPhase> {
        PollPhase::decode(&self.value)
    }
}

pub fn save_phase(phase: &PollPhase) -> Statement {
    Statement {
        sql: "INSERT INTO kv (key, value) VALUES (?1, ?2) \
              ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params: vec![Param::text(POLL_PHASE_KEY), Param::text(phase.encode())],
    }
}

pub fn upsert(row: &WorkRow) -> Statement {
    let columns = row.kind.columns();
    Statement {
        sql: "INSERT INTO work (kind, key, payload, due_at, attempts, dead, last_error) \
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
              ON CONFLICT (kind, key) DO UPDATE SET payload = excluded.payload, \
              due_at = excluded.due_at, attempts = excluded.attempts, dead = excluded.dead, \
              last_error = excluded.last_error",
        params: vec![
            Param::text(columns.kind.as_str()),
            Param::text(columns.key),
            Param::opt_text(columns.payload),
            Param::ts(row.due_at),
            Param::Int(i64::from(row.attempts)),
            Param::flag(row.is_dead()),
            Param::opt_text(row.last_error.clone()),
        ],
    }
}

pub fn delete(kind: &WorkKind) -> Statement {
    Statement {
        sql: "DELETE FROM work WHERE kind = ?1 AND key = ?2",
        params: vec![Param::text(kind.kind().as_str()), Param::text(kind.key())],
    }
}

pub fn save_state(state: &SyncState) -> Statement {
    let count = |n: Option<u32>| n.map_or(Param::Null, |n| Param::Int(i64::from(n)));
    let last_ok = |kind| Param::opt_ts(state.last_ok.get(&kind).copied());
    Statement {
        sql: "UPDATE sync_state SET last_modified = ?1, next_poll_at = ?2, rl_limit = ?3, \
              rl_remaining = ?4, rl_reset = ?5, scopes = ?6, strikes = ?7, retry_at = ?8, \
              no_token = ?9, last_kick_at = ?10, last_ok_poll_notifications = ?11, \
              last_ok_enrich = ?12, last_ok_backfill_page = ?13, last_ok_rollup = ?14 \
              WHERE id = 1",
        params: vec![
            Param::opt_text(state.last_modified.clone()),
            Param::opt_ts(state.next_poll_at),
            count(state.rate.limit),
            count(state.rate.remaining),
            Param::opt_ts(state.rate.reset),
            Param::opt_text(state.scopes.clone()),
            Param::Int(i64::from(state.throttle.strikes)),
            Param::opt_ts(state.throttle.retry_at),
            Param::flag(state.throttle.no_token),
            Param::opt_ts(state.last_kick_at),
            last_ok(SyncKind::PollNotifications),
            last_ok(SyncKind::Enrich),
            last_ok(SyncKind::BackfillPage),
            last_ok(SyncKind::Rollup),
        ],
    }
}

/// Rows that fail to parse are left out, so one bad row cannot stop the queue.
pub fn parse_rows(raw: Vec<RawRow>) -> (Vec<WorkRow>, Vec<WorkError>) {
    let mut rows = Vec::with_capacity(raw.len());
    let mut errors = Vec::new();
    for row in raw {
        match row.parse() {
            Ok(row) => rows.push(row),
            Err(err) => errors.push(err),
        }
    }
    (rows, errors)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rusqlite::Connection;
    use serde_json::Value;

    use super::*;
    use crate::db::testing::{count_rows, exec, query_json};
    use crate::sync::work::{Cursor, EnrichBatch, NodeId};

    fn open() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    fn rows(conn: &Connection) -> Vec<WorkRow> {
        let raw = query_json(conn, SELECT_ROWS, &[])
            .into_iter()
            .map(|m| serde_json::from_value::<RawRow>(Value::Object(m)).unwrap())
            .collect();
        let (rows, errors) = parse_rows(raw);
        assert!(errors.is_empty(), "{errors:?}");
        rows
    }

    fn state(conn: &Connection) -> SyncState {
        let mut found = query_json(conn, SELECT_STATE, &[]);
        assert_eq!(found.len(), 1);
        serde_json::from_value::<RawState>(Value::Object(found.remove(0)))
            .unwrap()
            .parse()
    }

    fn enrich() -> WorkKind {
        let ids = vec![
            NodeId::parse("PR_b").unwrap(),
            NodeId::parse("I_a").unwrap(),
        ];
        WorkKind::Enrich(EnrichBatch::new(ids).unwrap())
    }

    #[test]
    fn schema_applies_twice_and_holds_one_state_row() {
        let conn = open();
        conn.execute_batch(SCHEMA).unwrap();
        assert_eq!(count_rows(&conn, "sync_state"), 1);
        assert_eq!(state(&conn), SyncState::default());
        assert!(conn
            .execute("INSERT INTO sync_state (id) VALUES (2)", [])
            .is_err());
    }

    #[test]
    fn upserting_the_same_unit_keeps_one_row() {
        let conn = open();
        let first = WorkRow::pending(WorkKind::PollNotifications, Timestamp(10));
        exec(&conn, &upsert(&first)).unwrap();
        let retried = WorkRow {
            attempts: 1,
            due_at: Timestamp(20),
            last_error: Some("server status=500".into()),
            ..first
        };
        exec(&conn, &upsert(&retried)).unwrap();
        assert_eq!(rows(&conn), vec![retried]);
    }

    #[test]
    fn every_kind_round_trips_and_deletes_by_key() {
        let conn = open();
        let all = vec![
            WorkRow::pending(WorkKind::PollNotifications, Timestamp(1)),
            WorkRow::pending(enrich(), Timestamp(2)),
            WorkRow {
                attempts: 3,
                state: RowState::Dead,
                last_error: Some("transport".into()),
                ..WorkRow::pending(
                    WorkKind::BackfillPage(Cursor::After("Y3I=".into())),
                    Timestamp(3),
                )
            },
            WorkRow::pending(WorkKind::Rollup, Timestamp(4)),
        ];
        for row in &all {
            exec(&conn, &upsert(row)).unwrap();
        }
        let mut back = rows(&conn);
        back.sort_by_key(|r| r.due_at);
        assert_eq!(back, all);
        exec(&conn, &delete(&enrich())).unwrap();
        assert_eq!(count_rows(&conn, "work"), 3);
        assert_eq!(count_rows(&conn, "work WHERE dead = 1"), 1);
    }

    #[test]
    fn dead_is_stored_as_zero_or_one() {
        let conn = open();
        let bad = conn.execute(
            "INSERT INTO work (kind, key, due_at, dead) VALUES ('rollup', 'nightly', 0, 2)",
            [],
        );
        assert!(bad.is_err());
        let unknown = conn.execute(
            "INSERT INTO work (kind, key, due_at) VALUES ('seed_history', 'x', 0)",
            [],
        );
        assert!(unknown.is_err());
    }

    #[test]
    fn state_round_trips_every_column() {
        let conn = open();
        let saved = SyncState {
            last_modified: Some("Thu, 05 Nov 2026 10:00:00 GMT".into()),
            next_poll_at: Some(Timestamp(105)),
            rate: Rate {
                limit: Some(5000),
                remaining: Some(400),
                reset: Some(Timestamp(120)),
            },
            scopes: Some("notifications,read:user".into()),
            throttle: Throttle {
                strikes: 2,
                retry_at: Some(Timestamp(220)),
                no_token: true,
            },
            last_kick_at: Some(Timestamp(100)),
            last_ok: BTreeMap::from([
                (SyncKind::PollNotifications, Timestamp(90)),
                (SyncKind::Rollup, Timestamp(80)),
            ]),
            poll_phase: PollPhase::default(),
        };
        exec(&conn, &save_state(&saved)).unwrap();
        assert_eq!(state(&conn), saved);
    }

    fn phase(conn: &Connection) -> Option<PollPhase> {
        let found = query_json(conn, SELECT_PHASE, &[]);
        assert!(found.len() <= 1);
        let raw =
            serde_json::from_value::<RawKv>(Value::Object(found.into_iter().next()?)).unwrap();
        Some(raw.parse_phase().unwrap())
    }

    #[test]
    fn a_stored_phase_that_does_not_parse_is_none_not_a_panic() {
        let bad = RawKv {
            value: "scan|only|three".into(),
        };
        assert_eq!(bad.parse_phase(), None);
    }

    #[test]
    fn the_poll_phase_is_absent_then_saved_then_replaced() {
        let conn = open();
        assert_eq!(phase(&conn), None);
        let scanning = PollPhase::Scanning {
            committed: Some("old".into()),
            incoming: Some("new".into()),
            next_page: 2,
            started_at: Timestamp(100),
        };
        exec(&conn, &save_phase(&scanning)).unwrap();
        assert_eq!(phase(&conn), Some(scanning));
        let idle = PollPhase::Idle {
            last_modified: Some("new".into()),
        };
        exec(&conn, &save_phase(&idle)).unwrap();
        assert_eq!(phase(&conn), Some(idle));
        assert_eq!(count_rows(&conn, "kv"), 1);
    }

    #[test]
    fn a_row_that_does_not_parse_is_left_out() {
        let conn = open();
        conn.execute(
            "INSERT INTO work (kind, key, due_at) VALUES ('poll_notifications', 'other', 0)",
            [],
        )
        .unwrap();
        exec(
            &conn,
            &upsert(&WorkRow::pending(WorkKind::Rollup, Timestamp(1))),
        )
        .unwrap();
        let raw = query_json(&conn, SELECT_ROWS, &[])
            .into_iter()
            .map(|m| serde_json::from_value::<RawRow>(Value::Object(m)).unwrap())
            .collect();
        let (rows, errors) = parse_rows(raw);
        assert_eq!(rows.len(), 1);
        assert_eq!(errors.len(), 1);
    }
}
