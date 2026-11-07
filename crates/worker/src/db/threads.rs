use domain::ids::{RepoName, Timestamp};

use super::rows::{tagged, Tagged};
use super::sql::{Param, Statement};

const UPSERT: &str = "INSERT INTO notification_threads (id, repo, subject_kind, subject_title, \
     subject_number, item_node_id, reason, updated_at, synced_at) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
     ON CONFLICT (id) DO UPDATE SET \
     repo = excluded.repo, subject_kind = excluded.subject_kind, \
     subject_title = excluded.subject_title, subject_number = excluded.subject_number, \
     item_node_id = COALESCE(excluded.item_node_id, notification_threads.item_node_id), \
     reason = excluded.reason, \
     updated_at = excluded.updated_at, synced_at = excluded.synced_at \
     WHERE excluded.updated_at >= notification_threads.updated_at";

const KNOWN_UPDATED_AT: &str = "SELECT id, updated_at FROM notification_threads \
     WHERE id IN (SELECT value FROM json_each(?1))";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKind {
    Pr,
    Issue,
    Other,
}

tagged!(SubjectKind {
    SubjectKind::Pr => "pr",
    SubjectKind::Issue => "issue",
    SubjectKind::Other => "other",
});

/// One GitHub notification thread. It has no unread flag, because reading
/// never changes anything in Radar (ADR 0002).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadRow {
    pub id: String,
    pub repo: RepoName,
    pub subject_kind: SubjectKind,
    pub subject_title: String,
    pub subject_number: Option<u32>,
    pub item_node_id: Option<String>,
    pub reason: String,
    pub updated_at: Timestamp,
    pub synced_at: Timestamp,
}

pub fn upsert(row: &ThreadRow) -> Statement {
    Statement {
        sql: UPSERT,
        params: vec![
            Param::text(&row.id),
            Param::text(row.repo.to_string()),
            Param::text(row.subject_kind.tag()),
            Param::text(&row.subject_title),
            row.subject_number
                .map_or(Param::Null, |n| Param::Int(i64::from(n))),
            Param::opt_text(row.item_node_id.as_deref()),
            Param::text(&row.reason),
            Param::ts(row.updated_at),
            Param::ts(row.synced_at),
        ],
    }
}

/// A stored thread's id and `updated_at`, which a poll compares with the page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct KnownThread {
    pub id: String,
    pub updated_at: i64,
}

/// The stored `updated_at` of each thread in `ids` that Radar already has. The ids
/// travel as one JSON array so the statement stays a constant. An id with a character
/// outside a thread id's alphabet is left out, which also keeps the array valid JSON.
pub fn known_updated_at(ids: &[&str]) -> Statement {
    let plain = |id: &&str| {
        id.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'='))
    };
    let quoted: Vec<String> = ids
        .iter()
        .filter(|id| plain(id))
        .map(|id| format!("\"{id}\""))
        .collect();
    Statement {
        sql: KNOWN_UPDATED_AT,
        params: vec![Param::text(format!("[{}]", quoted.join(",")))],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};

    fn thread(updated_at: i64, title: &str) -> ThreadRow {
        ThreadRow {
            id: "1234".to_owned(),
            repo: RepoName::parse("karthik-beta/radar").expect("repo"),
            subject_kind: SubjectKind::Pr,
            subject_title: title.to_owned(),
            subject_number: Some(42),
            item_node_id: None,
            reason: "review_requested".to_owned(),
            updated_at: Timestamp(updated_at),
            synced_at: Timestamp(5000),
        }
    }

    fn title(conn: &rusqlite::Connection) -> String {
        let rows = query_json(conn, "SELECT subject_title FROM notification_threads", &[]);
        rows[0]["subject_title"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn older_thread_update_does_not_overwrite_newer() {
        let conn = memory_db();
        exec(&conn, &upsert(&thread(3000, "newer"))).expect("newer");
        exec(&conn, &upsert(&thread(2000, "older"))).expect("older");
        assert_eq!(count_rows(&conn, "notification_threads"), 1);
        assert_eq!(title(&conn), "newer");
    }

    #[test]
    fn a_poll_keeps_the_node_id_an_enrich_stored() {
        let conn = memory_db();
        let enriched = ThreadRow {
            item_node_id: Some("PR_kwDOAbc123".to_owned()),
            ..thread(2000, "enriched")
        };
        exec(&conn, &upsert(&enriched)).expect("enriched");
        exec(&conn, &upsert(&thread(3000, "polled"))).expect("polled");
        let rows = query_json(
            &conn,
            "SELECT item_node_id, subject_title FROM notification_threads",
            &[],
        );
        assert_eq!(rows[0]["item_node_id"], "PR_kwDOAbc123");
        assert_eq!(rows[0]["subject_title"], "polled");
    }

    #[test]
    fn known_updated_at_returns_only_the_stored_ids() {
        let conn = memory_db();
        exec(&conn, &upsert(&thread(3000, "one"))).expect("one");
        let found = |ids: &[&str]| {
            let stmt = known_updated_at(ids);
            query_json(&conn, stmt.sql, &stmt.params)
        };
        let hit = found(&["1234", "9999"]);
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0]["id"], "1234");
        assert_eq!(hit[0]["updated_at"], 3000);
        assert!(found(&["9999"]).is_empty());
        assert!(found(&[]).is_empty());
        assert_eq!(
            found(&["1234", "a\"b", "c,d"]).len(),
            1,
            "odd ids are ignored"
        );
    }

    #[test]
    fn thread_has_no_unread_column() {
        let conn = memory_db();
        let rows = query_json(
            &conn,
            "SELECT name FROM pragma_table_info('notification_threads')",
            &[],
        );
        for row in rows {
            let name = row["name"].as_str().unwrap_or_default();
            assert!(!name.contains("unread") && !name.contains("read"), "{name}");
        }
    }

    #[test]
    fn non_item_thread_needs_no_number() {
        let conn = memory_db();
        let release = ThreadRow {
            subject_kind: SubjectKind::Other,
            subject_number: None,
            ..thread(1000, "v1.0")
        };
        exec(&conn, &upsert(&release)).expect("other without number");
        let pr_without_number = ThreadRow {
            id: "9".to_owned(),
            subject_number: None,
            ..thread(1000, "pr")
        };
        assert!(exec(&conn, &upsert(&pr_without_number)).is_err());
    }
}
