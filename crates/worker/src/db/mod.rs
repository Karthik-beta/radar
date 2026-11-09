//! D1 row access. Every write is a pure `Statement`, so tests run the same SQL
//! against SQLite natively and `run` is the only function that touches D1.

pub mod auth;
pub mod batch;
pub mod events;
pub mod items;
pub mod local_state;
pub mod queries;
pub mod repos;
pub mod rollups;
pub mod rows;
pub mod sql;
pub mod threads;

#[cfg(any(test, feature = "sql-dump"))]
pub mod synthetic;

pub use rows::RowError;
pub use sql::{run, Param, Statement};

#[cfg(test)]
pub(crate) mod testing {
    use domain::activity::{Actor, ActorKind};
    use domain::ids::{ItemKind, ItemRef, Login, RepoName, Timestamp};
    use domain::item::{CiState, ItemState, ReviewDecision};
    use domain::stage::Stage;
    use domain::turn::Turn;
    use rusqlite::types::{Value, ValueRef};
    use rusqlite::{params_from_iter, Connection};
    use serde_json::{Map, Value as Json};

    use super::items::ItemRow;
    use super::sql::{Param, Statement};

    const MIGRATIONS: [&str; 2] = [
        include_str!("../../../../migrations/0001_init.sql"),
        include_str!("../../../../migrations/0003_item_extras.sql"),
    ];

    pub fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open");
        for migration in MIGRATIONS {
            conn.execute_batch(migration).expect("migration applies");
        }
        conn
    }

    fn bind(params: &[Param]) -> Vec<Value> {
        params
            .iter()
            .map(|p| match p {
                Param::Text(s) => Value::Text(s.clone()),
                Param::Int(i) => Value::Integer(*i),
                Param::Real(f) => Value::Real(*f),
                Param::Null => Value::Null,
                Param::Blob(b) => Value::Blob(b.clone()),
            })
            .collect()
    }

    pub fn exec(conn: &Connection, stmt: &Statement) -> rusqlite::Result<usize> {
        conn.execute(stmt.sql, params_from_iter(bind(&stmt.params)))
    }

    pub fn query_json(conn: &Connection, sql: &str, params: &[Param]) -> Vec<Map<String, Json>> {
        let mut prepared = conn.prepare(sql).expect("prepare");
        let names: Vec<String> = prepared.column_names().map(str::to_owned).collect();
        let mut rows = prepared
            .query(params_from_iter(bind(params)))
            .expect("query");
        let mut out = Vec::new();
        while let Some(row) = rows.next().expect("row") {
            let mut map = Map::new();
            for (i, name) in names.iter().enumerate() {
                let value = match row.get_ref(i).expect("column") {
                    ValueRef::Null => Json::Null,
                    ValueRef::Integer(n) => Json::from(n),
                    ValueRef::Real(f) => Json::from(f),
                    ValueRef::Text(t) => Json::from(String::from_utf8_lossy(t).into_owned()),
                    ValueRef::Blob(b) => Json::from(b.to_vec()),
                };
                map.insert(name.clone(), value);
            }
            out.push(map);
        }
        out
    }

    /// `table` may carry a WHERE clause, as in `items WHERE turn = 'me'`.
    pub fn count_rows(conn: &Connection, table: &str) -> usize {
        let sql = format!("SELECT COUNT(*) AS n FROM {table}");
        let rows = query_json(conn, &sql, &[]);
        rows[0]["n"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(usize::MAX)
    }

    pub fn explain(conn: &Connection, stmt: &Statement) -> Vec<String> {
        let sql = format!("EXPLAIN QUERY PLAN {}", stmt.sql);
        query_json(conn, &sql, &stmt.params)
            .iter()
            .map(|r| r["detail"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    /// The `(opcode, p4)` pairs of the VDBE program SQLite compiles for `stmt`.
    pub fn opcodes(conn: &Connection, stmt: &Statement) -> Vec<(String, String)> {
        let sql = format!("EXPLAIN {}", stmt.sql);
        query_json(conn, &sql, &stmt.params)
            .iter()
            .map(|r| {
                let text = |key: &str| r[key].as_str().unwrap_or_default().to_owned();
                (text("opcode"), text("p4"))
            })
            .collect()
    }

    /// An open pull request of mine that waits on someone else.
    pub fn item_row(n: u32) -> ItemRow {
        ItemRow {
            node_id: format!("PR_{n}"),
            item_ref: ItemRef {
                repo: RepoName::parse("karthik-beta/radar").expect("repo"),
                number: n,
                kind: ItemKind::Pr,
            },
            title: format!("Title {n}"),
            url: format!("https://github.com/karthik-beta/radar/pull/{n}"),
            head_ref: Some("feature/x".to_owned()),
            author: Actor {
                login: Login::parse("karthik-beta").expect("login"),
                kind: ActorKind::User,
            },
            authored_by_me: true,
            state: ItemState::Open,
            created_at: Timestamp(1000),
            updated_at: Timestamp(2000),
            closed_at: None,
            turn: Turn::Them,
            since: Timestamp(2000),
            stage: Some(Stage::AwaitingReview),
            ci_state: Some(CiState::Passing),
            review_decision: Some(ReviewDecision::ReviewRequired),
            last_human: None,
            requested_reviewers: Vec::new(),
            stage_since: Timestamp(2000),
            latest_url: format!("https://github.com/karthik-beta/radar/pull/{n}"),
            synced_at: Timestamp(5000),
        }
    }
}
