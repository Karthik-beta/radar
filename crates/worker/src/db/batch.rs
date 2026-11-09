//! The reads one `Enrich` unit makes before it calls GitHub. D1 runs them as one
//! batch, so a unit costs one round trip to read and one to write.

use domain::ids::{ItemKind, ItemRef, Login, RepoName, Timestamp};
use domain::item::CiState;
use domain::stage::Stage;
use serde::Deserialize;

use super::rows::{count, untag, untag_opt, RowError};
use super::sql::{Param, Statement};

/// `?1` is a JSON array of ids. A thread is named by its notification id, or by
/// its item's node id when the poll already learned it.
macro_rules! wanted {
    () => {
        "t.subject_kind IN ('pr', 'issue') AND (t.id IN (SELECT value FROM json_each(?1)) \
         OR t.item_node_id IN (SELECT value FROM json_each(?1)))"
    };
}

const THREADS: &str = concat!(
    "SELECT t.id, t.repo, t.subject_kind, t.subject_number, t.reason \
     FROM notification_threads t WHERE ",
    wanted!(),
    " ORDER BY t.id"
);

const STORED: &str = concat!(
    "SELECT i.repo, i.number, i.node_id, i.ci_state, i.stage, i.stage_since, i.updated_at \
     FROM notification_threads t JOIN items i ON i.repo = t.repo AND i.number = t.subject_number \
     WHERE ",
    wanted!()
);

const ME: &str = "SELECT login FROM auth WHERE id = 1";

fn ids_json(ids: &[&str]) -> Param {
    Param::text(serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_owned()))
}

pub fn threads(ids: &[&str]) -> Statement {
    Statement {
        sql: THREADS,
        params: vec![ids_json(ids)],
    }
}

pub fn stored_items(ids: &[&str]) -> Statement {
    Statement {
        sql: STORED,
        params: vec![ids_json(ids)],
    }
}

pub fn me() -> Statement {
    Statement {
        sql: ME,
        params: Vec::new(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ThreadRecord {
    pub id: String,
    pub repo: String,
    pub subject_kind: String,
    pub subject_number: i64,
    pub reason: String,
}

/// A notification thread that points at a pull request or an issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    pub item: ItemRef,
    /// The notification reason, such as `mention`. It is the only place Radar
    /// learns that a comment mentioned me.
    pub reason: String,
}

impl TryFrom<ThreadRecord> for Thread {
    type Error = RowError;

    fn try_from(r: ThreadRecord) -> Result<Self, RowError> {
        let repo = RepoName::parse(&r.repo).map_err(|source| RowError::BadId {
            column: "repo",
            source,
        })?;
        Ok(Self {
            id: r.id,
            item: ItemRef {
                repo,
                number: count("subject_number", r.subject_number)?,
                kind: untag::<ItemKind>("subject_kind", &r.subject_kind)?,
            },
            reason: r.reason,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct StoredRecord {
    pub repo: String,
    pub number: i64,
    pub node_id: String,
    pub ci_state: Option<String>,
    pub stage: Option<String>,
    pub stage_since: i64,
    pub updated_at: i64,
}

/// What the last enrich of an item left in `items`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub repo: RepoName,
    pub number: u32,
    pub node_id: String,
    pub ci_state: Option<CiState>,
    pub stage: Option<Stage>,
    pub stage_since: Timestamp,
    pub updated_at: Timestamp,
}

impl TryFrom<StoredRecord> for Stored {
    type Error = RowError;

    fn try_from(r: StoredRecord) -> Result<Self, RowError> {
        Ok(Self {
            repo: RepoName::parse(&r.repo).map_err(|source| RowError::BadId {
                column: "repo",
                source,
            })?,
            number: count("number", r.number)?,
            node_id: r.node_id,
            ci_state: untag_opt::<CiState>("ci_state", r.ci_state.as_deref())?,
            stage: untag_opt::<Stage>("stage", r.stage.as_deref())?,
            stage_since: Timestamp(r.stage_since),
            updated_at: Timestamp(r.updated_at),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct MeRecord {
    pub login: String,
}

impl MeRecord {
    pub fn login(&self) -> Result<Login, RowError> {
        Login::parse(&self.login).map_err(|source| RowError::BadId {
            column: "login",
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::items;
    use crate::db::testing::{exec, item_row, memory_db, query_json};
    use crate::db::threads::{self, SubjectKind, ThreadRow};

    fn thread_row(id: &str, number: u32, node: Option<&str>) -> ThreadRow {
        ThreadRow {
            id: id.to_owned(),
            repo: RepoName::parse("karthik-beta/radar").expect("repo"),
            subject_kind: SubjectKind::Pr,
            subject_title: "t".to_owned(),
            subject_number: Some(number),
            item_node_id: node.map(str::to_owned),
            reason: "mention".to_owned(),
            updated_at: Timestamp(10),
            synced_at: Timestamp(10),
        }
    }

    fn typed<T, R: TryFrom<T, Error = RowError>>(
        rows: Vec<serde_json::Map<String, serde_json::Value>>,
    ) -> Vec<R>
    where
        T: serde::de::DeserializeOwned,
    {
        rows.into_iter()
            .map(|row| {
                let record: T =
                    serde_json::from_value(serde_json::Value::Object(row)).expect("record");
                R::try_from(record).expect("typed")
            })
            .collect()
    }

    #[test]
    fn threads_are_found_by_thread_id_or_node_id_and_come_back_in_id_order() {
        let conn = memory_db();
        exec(&conn, &threads::upsert(&thread_row("9", 3, None))).expect("a");
        exec(&conn, &threads::upsert(&thread_row("2", 1, Some("PR_one")))).expect("b");
        exec(&conn, &threads::upsert(&thread_row("5", 2, None))).expect("c");
        let stmt = threads(&["9", "PR_one"]);
        let rows = query_json(&conn, stmt.sql, &stmt.params);
        let found: Vec<Thread> = typed::<ThreadRecord, Thread>(rows);
        let ids: Vec<&str> = found.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["2", "9"]);
        assert_eq!(found[0].item.number, 1);
        assert_eq!(found[0].reason, "mention");
    }

    #[test]
    fn a_thread_that_is_not_an_item_is_left_out() {
        let conn = memory_db();
        let release = ThreadRow {
            subject_kind: SubjectKind::Other,
            subject_number: None,
            ..thread_row("7", 1, None)
        };
        exec(&conn, &threads::upsert(&release)).expect("release");
        let stmt = threads(&["7"]);
        assert!(query_json(&conn, stmt.sql, &stmt.params).is_empty());
    }

    #[test]
    fn stored_items_join_through_the_thread_and_use_the_repo_number_index() {
        let conn = memory_db();
        let row = item_row(42);
        exec(&conn, &items::upsert(&row)).expect("item");
        exec(&conn, &items::upsert(&item_row(43))).expect("other item");
        exec(&conn, &threads::upsert(&thread_row("1", 42, None))).expect("thread");
        let stmt = stored_items(&["1"]);
        let stored: Vec<Stored> =
            typed::<StoredRecord, Stored>(query_json(&conn, stmt.sql, &stmt.params));
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].node_id, "PR_42");
        assert_eq!(stored[0].ci_state, row.ci_state);
        assert_eq!(stored[0].stage_since, row.stage_since);

        let plan = crate::db::testing::explain(&conn, &stmt);
        assert!(
            plan.iter()
                .any(|d| d.contains("sqlite_autoindex_items") || d.contains("idx_items")),
            "{plan:?}"
        );
    }

    #[test]
    fn me_reads_the_login() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO auth (id, github_user_id, login, scopes, created_at) VALUES (1, 1001, 'karthik', '', 0)",
            [],
        )
        .expect("auth");
        let stmt = me();
        let rows = query_json(&conn, stmt.sql, &stmt.params);
        let record: MeRecord =
            serde_json::from_value(serde_json::Value::Object(rows[0].clone())).expect("me");
        assert_eq!(record.login().expect("login").as_str(), "karthik");
    }
}
