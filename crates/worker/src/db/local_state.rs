//! Writes to `local_state`, what I did to an item inside Radar. Radar never
//! writes it back to GitHub. `domain::local_state::visible` reads it.

use domain::ids::Timestamp;

use super::sql::{Param, Statement};

/// Writes one row for an item that exists and none for an unknown id, so the
/// change count tells the caller which. `WHERE true` lets SQLite parse the
/// upsert after a `SELECT`.
const DISMISS: &str = "INSERT INTO local_state (item_id, kind, value) \
     SELECT node_id, 'dismissed', COALESCE(last_human_at, 0) FROM items \
     WHERE node_id = ?1 AND true \
     ON CONFLICT (item_id) DO UPDATE SET kind = excluded.kind, value = excluded.value";

const SNOOZE: &str = "INSERT INTO local_state (item_id, kind, value) \
     SELECT node_id, 'snoozed', ?2 FROM items WHERE node_id = ?1 AND true \
     ON CONFLICT (item_id) DO UPDATE SET kind = excluded.kind, value = excluded.value";

/// Hides the item until a human acts after its current `last_human_at`.
pub fn dismiss(item_id: &str) -> Statement {
    Statement {
        sql: DISMISS,
        params: vec![Param::text(item_id)],
    }
}

/// Hides the item until `until`.
pub fn snooze(item_id: &str, until: Timestamp) -> Statement {
    Statement {
        sql: SNOOZE,
        params: vec![Param::text(item_id), Param::ts(until)],
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::db::items::{self, ItemRow, LastHuman};
    use crate::db::testing::{exec, item_row, memory_db, query_json};
    use domain::ids::Login;
    use domain::local_state::LocalState;
    use domain::turn::{Reason, Turn};

    fn waiting(n: u32, human_at: Option<i64>) -> ItemRow {
        ItemRow {
            turn: Turn::Me(Reason::Mentioned),
            last_human: human_at.map(|at| LastHuman {
                at: Timestamp(at),
                login: Login::parse("octocat").expect("login"),
                kind: "comment".to_owned(),
                url: None,
            }),
            ..item_row(n)
        }
    }

    fn stored(conn: &Connection) -> Vec<Option<LocalState>> {
        query_json(conn, "SELECT kind, value FROM local_state", &[])
            .iter()
            .map(|r| {
                let kind = r["kind"].as_str().unwrap_or_default();
                LocalState::from_columns(kind, r["value"].as_i64().unwrap_or(-1))
            })
            .collect()
    }

    #[test]
    fn dismiss_stores_the_current_last_human_at_once() {
        let conn = memory_db();
        exec(&conn, &items::upsert(&waiting(1, Some(500)))).expect("item");
        assert_eq!(exec(&conn, &dismiss("PR_1")).expect("dismiss"), 1);
        assert_eq!(exec(&conn, &dismiss("PR_1")).expect("again"), 1);
        assert_eq!(
            stored(&conn),
            [Some(LocalState::Dismissed {
                seen_human_at: Timestamp(500)
            })]
        );
    }

    #[test]
    fn dismissing_an_item_with_no_human_activity_stores_zero() {
        let conn = memory_db();
        exec(&conn, &items::upsert(&waiting(1, None))).expect("item");
        exec(&conn, &dismiss("PR_1")).expect("dismiss");
        assert_eq!(
            stored(&conn),
            [Some(LocalState::Dismissed {
                seen_human_at: Timestamp(0)
            })]
        );
    }

    #[test]
    fn snooze_replaces_a_dismissal_and_a_resnooze_moves_the_wake_time() {
        let conn = memory_db();
        exec(&conn, &items::upsert(&waiting(1, Some(500)))).expect("item");
        exec(&conn, &dismiss("PR_1")).expect("dismiss");
        exec(&conn, &snooze("PR_1", Timestamp(1000))).expect("snooze");
        exec(&conn, &snooze("PR_1", Timestamp(2000))).expect("resnooze");
        assert_eq!(
            stored(&conn),
            [Some(LocalState::Snoozed {
                until: Timestamp(2000)
            })]
        );
    }

    #[test]
    fn an_unknown_id_writes_nothing() {
        let conn = memory_db();
        exec(&conn, &items::upsert(&waiting(1, None))).expect("item");
        assert_eq!(exec(&conn, &dismiss("PR_404")).expect("dismiss"), 0);
        assert_eq!(
            exec(&conn, &snooze("PR_404", Timestamp(1))).expect("snooze"),
            0
        );
        assert!(stored(&conn).is_empty());
    }
}
