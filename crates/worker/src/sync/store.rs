//! The `work` and `sync_state` tables in the Syncer's own SQLite. Every write is
//! a `Statement`, so tests run the same SQL against SQLite natively.

use domain::ids::Timestamp;
use domain::sync_status::{Rate, SyncKind};
use serde::Deserialize;

use super::poll::PollPhase;
use super::state::{SyncState, Throttle};
use super::work::{RowState, WorkError, WorkKind, WorkRow};
use crate::db::{Param, Statement};

macro_rules! work_table {
    ($name:literal) => {
        concat!(
            "CREATE TABLE ", $name, " (
  kind TEXT NOT NULL CHECK (kind IN ('poll_notifications', 'enrich', 'backfill_page', 'rollup', 'send_alerts')),
  key TEXT NOT NULL CHECK (key <> ''),
  payload TEXT,
  due_at INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  dead INTEGER NOT NULL DEFAULT 0 CHECK (dead IN (0, 1)),
  last_error TEXT,
  PRIMARY KEY (kind, key)
);
"
        )
    };
}

pub const SCHEMA: &str = concat!(
    work_table!("IF NOT EXISTS work"),
    "CREATE TABLE IF NOT EXISTS sync_state (
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
  token_rejected INTEGER NOT NULL DEFAULT 0 CHECK (token_rejected IN (0, 1)),
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
"
);

/// `CREATE TABLE IF NOT EXISTS` leaves a `work` table made by an older build with
/// its old `kind` CHECK, which rejects newer kinds. This rebuilds the table and
/// copies its rows. It runs once, then records the version, and a crash before
/// the last statement only repeats it.
pub const UPGRADE_WORK: &str = concat!(
    "DROP TABLE IF EXISTS work_new;
",
    work_table!("work_new"),
    "INSERT INTO work_new (kind, key, payload, due_at, attempts, dead, last_error)
  SELECT kind, key, payload, due_at, attempts, dead, last_error FROM work;
DROP TABLE work;
ALTER TABLE work_new RENAME TO work;
INSERT INTO kv (key, value) VALUES ('schema_version', '2')
  ON CONFLICT (key) DO UPDATE SET value = excluded.value;
"
);

/// `CREATE TABLE IF NOT EXISTS` also leaves a `sync_state` table made before
/// `token_rejected` without the column. SQLite has no `ADD COLUMN IF NOT
/// EXISTS`, so this runs only when `SELECT_TOKEN_REJECTED_COLUMN` finds none.
pub const ADD_TOKEN_REJECTED: &str =
    "ALTER TABLE sync_state ADD COLUMN token_rejected INTEGER NOT NULL DEFAULT 0 \
     CHECK (token_rejected IN (0, 1))";

pub const SELECT_TOKEN_REJECTED_COLUMN: &str =
    "SELECT COUNT(*) AS n FROM pragma_table_info('sync_state') WHERE name = 'token_rejected'";

pub const SET_SCHEMA_VERSION_3: &str =
    "INSERT INTO kv (key, value) VALUES ('schema_version', '3') \
     ON CONFLICT (key) DO UPDATE SET value = excluded.value";

pub const SELECT_SCHEMA_VERSION: &str = "SELECT value FROM kv WHERE key = 'schema_version'";

#[derive(Debug, Deserialize)]
pub struct RawCount {
    n: i64,
}

impl RawCount {
    pub fn is_zero(&self) -> bool {
        self.n == 0
    }
}

fn version(stored: Option<&RawKv>) -> i64 {
    stored
        .and_then(|kv| kv.value.parse::<i64>().ok())
        .unwrap_or(0)
}

pub fn needs_upgrade(stored: Option<&RawKv>) -> bool {
    version(stored) < 2
}

pub fn needs_token_rejected(stored: Option<&RawKv>) -> bool {
    version(stored) < 3
}

const POLL_PHASE_KEY: &str = "poll_phase";
const BACKFILL_DONE_KEY: &str = "backfill_done_at";
const HISTORY_SEEDED_KEY: &str = "history_seeded_at";
pub const FACTS_IMPORTED_KEY: &str = "facts_imported_at";
pub const REPOS_WEEK_KEY: &str = "repos_refreshed_week";
pub const PACED_UNTIL_KEY: &str = "import_paced_until";

pub const SELECT_PHASE: &str = "SELECT value FROM kv WHERE key = 'poll_phase'";
pub const SELECT_BACKFILL_DONE: &str = "SELECT value FROM kv WHERE key = 'backfill_done_at'";
pub const SELECT_HISTORY_SEEDED: &str = "SELECT value FROM kv WHERE key = 'history_seeded_at'";
pub const SELECT_FACTS_IMPORTED: &str = "SELECT value FROM kv WHERE key = 'facts_imported_at'";
pub const SELECT_REPOS_WEEK: &str = "SELECT value FROM kv WHERE key = 'repos_refreshed_week'";
pub const SELECT_PACED_UNTIL: &str = "SELECT value FROM kv WHERE key = 'import_paced_until'";

pub const SELECT_ROWS: &str =
    "SELECT kind, key, payload, due_at, attempts, dead, last_error FROM work";

pub const SELECT_STATE: &str = "SELECT last_modified, next_poll_at, rl_limit, rl_remaining, \
     rl_reset, scopes, strikes, retry_at, no_token, token_rejected, last_kick_at, \
     last_ok_poll_notifications, last_ok_enrich, last_ok_backfill_page, last_ok_rollup FROM sync_state WHERE id = 1";

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
    token_rejected: i64,
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
                token_rejected: self.token_rejected == 1,
            },
            last_kick_at: self.last_kick_at.map(Timestamp),
            last_ok: last_ok
                .into_iter()
                .filter_map(|(kind, at)| Some((kind, Timestamp(at?))))
                .collect(),
            poll_phase: PollPhase::default(),
            backfill_done_at: None,
            history_seeded_at: None,
            facts_imported_at: None,
            repos_refreshed_week: None,
            paced_until: None,
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

    pub fn parse_backfill_done(&self) -> Option<Timestamp> {
        self.value.parse::<i64>().ok().map(Timestamp)
    }
}

pub fn save_backfill_done(at: Timestamp) -> Statement {
    Statement {
        sql: "INSERT INTO kv (key, value) VALUES (?1, ?2) \
              ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params: vec![
            Param::text(BACKFILL_DONE_KEY),
            Param::text(at.0.to_string()),
        ],
    }
}

pub fn save_history_seeded(at: Timestamp) -> Statement {
    Statement {
        sql: "INSERT INTO kv (key, value) VALUES (?1, ?2) \
              ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params: vec![
            Param::text(HISTORY_SEEDED_KEY),
            Param::text(at.0.to_string()),
        ],
    }
}

/// A time kept in the `kv` table under `key`, one of the `*_KEY` constants.
pub fn save_time(key: &'static str, at: Timestamp) -> Statement {
    Statement {
        sql: "INSERT INTO kv (key, value) VALUES (?1, ?2) \
              ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params: vec![Param::text(key), Param::text(at.0.to_string())],
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
              last_ok_enrich = ?12, last_ok_backfill_page = ?13, last_ok_rollup = ?14, \
              token_rejected = ?15 WHERE id = 1",
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
            Param::flag(state.throttle.token_rejected),
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
    use crate::rollup::RollupKey;
    use crate::sync::backfill::{BackfillCursor, SearchKind, SearchWindow};
    use crate::sync::work::{EnrichBatch, NodeId};

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

    const OLD_WORK: &str = "CREATE TABLE work (
  kind TEXT NOT NULL CHECK (kind IN ('poll_notifications', 'enrich', 'backfill_page', 'rollup')),
  key TEXT NOT NULL CHECK (key <> ''),
  payload TEXT,
  due_at INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  dead INTEGER NOT NULL DEFAULT 0 CHECK (dead IN (0, 1)),
  last_error TEXT,
  PRIMARY KEY (kind, key)
);
INSERT INTO work (kind, key, due_at, attempts, last_error)
  VALUES ('rollup', 'nightly', 7, 2, 'boom');";

    fn stored_version(conn: &Connection) -> Option<RawKv> {
        query_json(conn, SELECT_SCHEMA_VERSION, &[])
            .into_iter()
            .next()
            .map(|m| serde_json::from_value(Value::Object(m)).unwrap())
    }

    #[test]
    fn an_old_work_table_is_rebuilt_with_its_rows_and_accepts_new_kinds() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(OLD_WORK).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        let insert = "INSERT INTO work (kind, key, due_at) VALUES ('send_alerts', 'main', 0)";
        assert!(conn.execute(insert, []).is_err());
        assert!(needs_upgrade(stored_version(&conn).as_ref()));
        conn.execute_batch(UPGRADE_WORK).unwrap();
        assert!(!needs_upgrade(stored_version(&conn).as_ref()));
        conn.execute(insert, []).unwrap();
        let kept = query_json(
            &conn,
            "SELECT attempts, last_error FROM work WHERE kind = 'rollup'",
            &[],
        );
        assert_eq!(kept[0]["attempts"], 2);
        assert_eq!(kept[0]["last_error"], "boom");
        assert_eq!(count_rows(&conn, "work"), 2);
        assert_eq!(
            count_rows(&conn, "sqlite_master WHERE name = 'work_new'"),
            0
        );
    }

    const OLD_SYNC_STATE: &str = "CREATE TABLE sync_state (
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
INSERT INTO sync_state (id, strikes, no_token) VALUES (1, 1, 1);
CREATE TABLE kv (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
INSERT INTO kv (key, value) VALUES ('schema_version', '2');";

    /// The steps `Syncer::upgrade_schema` runs after `SCHEMA`.
    fn upgrade(conn: &Connection) {
        let stored = stored_version(conn);
        if needs_upgrade(stored.as_ref()) {
            conn.execute_batch(UPGRADE_WORK).unwrap();
        }
        if needs_token_rejected(stored.as_ref()) {
            let count: RawCount = serde_json::from_value(Value::Object(
                query_json(conn, SELECT_TOKEN_REJECTED_COLUMN, &[]).remove(0),
            ))
            .unwrap();
            if count.is_zero() {
                conn.execute_batch(ADD_TOKEN_REJECTED).unwrap();
            }
            conn.execute_batch(SET_SCHEMA_VERSION_3).unwrap();
        }
    }

    #[test]
    fn the_upgrade_adds_token_rejected_to_an_old_sync_state_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(OLD_SYNC_STATE).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        assert!(conn.prepare(SELECT_STATE).is_err());
        upgrade(&conn);
        let mut kept = state(&conn);
        assert_eq!(kept.throttle.strikes, 1);
        assert!(kept.throttle.no_token);
        assert!(!kept.throttle.token_rejected);
        kept.throttle.token_rejected = true;
        exec(&conn, &save_state(&kept)).unwrap();
        assert!(state(&conn).throttle.token_rejected);
        assert!(conn
            .execute("UPDATE sync_state SET token_rejected = 2", [])
            .is_err());
        assert!(!needs_token_rejected(stored_version(&conn).as_ref()));
        upgrade(&conn);
    }

    #[test]
    fn a_new_object_gets_token_rejected_from_the_schema_and_the_upgrade_skips_the_column() {
        let conn = open();
        upgrade(&conn);
        assert!(!needs_upgrade(stored_version(&conn).as_ref()));
        assert!(!needs_token_rejected(stored_version(&conn).as_ref()));
        assert_eq!(state(&conn), SyncState::default());
    }

    #[test]
    fn the_upgrade_is_safe_to_repeat() {
        let conn = open();
        conn.execute_batch(UPGRADE_WORK).unwrap();
        conn.execute(
            "INSERT INTO work (kind, key, due_at) VALUES ('send_alerts', 'main', 0)",
            [],
        )
        .unwrap();
        conn.execute_batch(UPGRADE_WORK).unwrap();
        assert_eq!(count_rows(&conn, "work"), 1);
        assert!(!needs_upgrade(stored_version(&conn).as_ref()));
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
                    WorkKind::BackfillPage(BackfillCursor {
                        window: SearchWindow::Backfill90d,
                        since: "2026-08-09".into(),
                        kind: SearchKind::AuthoredIssues,
                        page: 2,
                        after: Some("Y3I=".into()),
                    }),
                    Timestamp(3),
                )
            },
            WorkRow::pending(WorkKind::Rollup(RollupKey::Nightly), Timestamp(4)),
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
                token_rejected: true,
            },
            last_kick_at: Some(Timestamp(100)),
            last_ok: BTreeMap::from([
                (SyncKind::PollNotifications, Timestamp(90)),
                (SyncKind::Rollup, Timestamp(80)),
            ]),
            poll_phase: PollPhase::default(),
            backfill_done_at: None,
            history_seeded_at: None,
            facts_imported_at: None,
            repos_refreshed_week: None,
            paced_until: None,
        };
        exec(&conn, &save_state(&saved)).unwrap();
        assert_eq!(state(&conn), saved);
    }

    fn backfill_done(conn: &Connection) -> Option<Timestamp> {
        let found = query_json(conn, SELECT_BACKFILL_DONE, &[]);
        assert!(found.len() <= 1);
        let raw =
            serde_json::from_value::<RawKv>(Value::Object(found.into_iter().next()?)).unwrap();
        Some(raw.parse_backfill_done().unwrap())
    }

    #[test]
    fn backfill_done_is_absent_then_saved_and_shares_the_kv_table_with_the_phase() {
        let conn = open();
        assert_eq!(backfill_done(&conn), None);
        exec(&conn, &save_backfill_done(Timestamp(1_794_052_800))).unwrap();
        assert_eq!(backfill_done(&conn), Some(Timestamp(1_794_052_800)));
        exec(&conn, &save_phase(&PollPhase::default())).unwrap();
        assert_eq!(backfill_done(&conn), Some(Timestamp(1_794_052_800)));
        assert_eq!(count_rows(&conn, "kv"), 2);
        let bad = RawKv {
            value: "yesterday".into(),
        };
        assert_eq!(bad.parse_backfill_done(), None);
    }

    #[test]
    fn import_markers_are_kept_apart_in_the_kv_table() {
        let conn = open();
        let read = |select: &str| {
            query_json(&conn, select, &[])
                .into_iter()
                .next()
                .map(|m| serde_json::from_value::<RawKv>(Value::Object(m)).unwrap())
                .and_then(|kv| kv.parse_backfill_done())
        };
        assert_eq!(read(SELECT_FACTS_IMPORTED), None);
        exec(&conn, &save_time(FACTS_IMPORTED_KEY, Timestamp(10))).unwrap();
        exec(&conn, &save_time(REPOS_WEEK_KEY, Timestamp(345_600))).unwrap();
        exec(&conn, &save_time(PACED_UNTIL_KEY, Timestamp(30))).unwrap();
        exec(&conn, &save_time(PACED_UNTIL_KEY, Timestamp(40))).unwrap();
        assert_eq!(read(SELECT_FACTS_IMPORTED), Some(Timestamp(10)));
        assert_eq!(read(SELECT_REPOS_WEEK), Some(Timestamp(345_600)));
        assert_eq!(read(SELECT_PACED_UNTIL), Some(Timestamp(40)));
        assert_eq!(count_rows(&conn, "kv"), 3);
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
            &upsert(&WorkRow::pending(
                WorkKind::Rollup(RollupKey::Nightly),
                Timestamp(1),
            )),
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
