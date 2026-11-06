use domain::ids::Timestamp;

use super::items::item_columns;
use super::sql::{Param, Statement};

pub const RECENT_WINDOW_SECS: i64 = 14 * 86_400;

/// Open items waiting on me, oldest first. `?1` is now, in epoch seconds. A
/// dismissed item stays hidden until a human acts after the activity I saw. A
/// snoozed item stays hidden until its wake time.
pub const YOUR_TURN: &str = concat!(
    "SELECT ",
    item_columns!(),
    " FROM items WHERE turn = 'me' AND state IN ('open', 'draft') \
     AND NOT EXISTS ( \
       SELECT 1 FROM local_state WHERE local_state.item_id = items.node_id AND ( \
         (local_state.kind = 'dismissed' AND COALESCE(items.last_human_at, 0) <= local_state.value) \
         OR (local_state.kind = 'snoozed' AND local_state.value > ?1))) \
     ORDER BY since ASC"
);

/// My open items waiting on someone else, longest wait first.
pub const THEIR_TURN: &str = concat!(
    "SELECT ",
    item_columns!(),
    " FROM items WHERE turn = 'them' AND authored_by_me = 1 AND state IN ('open', 'draft') \
     ORDER BY since ASC"
);

/// My open pull requests grouped by stage. The oldest wait comes first in each stage.
pub const PIPELINE: &str = concat!(
    "SELECT ",
    item_columns!(),
    " FROM items WHERE kind = 'pr' AND authored_by_me = 1 AND state IN ('open', 'draft') \
     ORDER BY stage ASC, since ASC"
);

/// Items closed or merged since `?1`, newest first.
pub const MERGED_CLOSED: &str = concat!(
    "SELECT ",
    item_columns!(),
    " FROM items WHERE state IN ('merged', 'closed') AND closed_at >= ?1 \
     ORDER BY closed_at DESC"
);

pub fn your_turn(now: Timestamp) -> Statement {
    Statement {
        sql: YOUR_TURN,
        params: vec![Param::ts(now)],
    }
}

pub fn their_turn() -> Statement {
    Statement {
        sql: THEIR_TURN,
        params: Vec::new(),
    }
}

pub fn pipeline() -> Statement {
    Statement {
        sql: PIPELINE,
        params: Vec::new(),
    }
}

pub fn merged_closed(now: Timestamp) -> Statement {
    Statement {
        sql: MERGED_CLOSED,
        params: vec![Param::Int(now.0 - RECENT_WINDOW_SECS)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::items;
    use crate::db::synthetic;
    use crate::db::testing::{exec, explain, memory_db};

    fn seeded(n: u32) -> rusqlite::Connection {
        let conn = memory_db();
        for row in synthetic::items(n).expect("items") {
            exec(&conn, &items::upsert(&row)).expect("seed");
        }
        conn
    }

    /// A full scan of the partial index is fine. A scan of the table is not.
    fn assert_indexed(name: &str, plan: &[String]) {
        assert!(
            plan.iter().any(|d| d.contains("INDEX idx_items_")),
            "{name} uses no idx_items_ index: {plan:?}"
        );
        for detail in plan {
            let scans_table = detail.starts_with("SCAN items") && !detail.contains("USING");
            assert!(!scans_table, "{name} scans the items table: {detail}");
        }
    }

    #[test]
    fn dashboard_queries_use_an_items_index() {
        let conn = seeded(2000);
        let now = synthetic::NOW;
        let cases = [
            ("YOUR_TURN", your_turn(now), "idx_items_me_since"),
            ("THEIR_TURN", their_turn(), "idx_items_them_since"),
            ("PIPELINE", pipeline(), "idx_items_pipeline"),
            ("MERGED_CLOSED", merged_closed(now), "idx_items_closed"),
        ];
        for (name, stmt, index) in cases {
            let plan = explain(&conn, &stmt);
            assert_indexed(name, &plan);
            assert!(plan.iter().any(|d| d.contains(index)), "{name}: {plan:?}");
        }
    }

    #[test]
    fn dashboard_queries_avoid_a_sort() {
        let conn = seeded(500);
        for stmt in [
            your_turn(synthetic::NOW),
            their_turn(),
            pipeline(),
            merged_closed(synthetic::NOW),
        ] {
            let plan = explain(&conn, &stmt);
            assert!(
                !plan.iter().any(|d| d.contains("TEMP B-TREE")),
                "{}: {plan:?}",
                stmt.sql
            );
        }
    }

    #[test]
    fn explain_reports_a_table_scan_when_there_is_one() {
        let conn = seeded(10);
        let plan = explain(
            &conn,
            &Statement {
                sql: "SELECT node_id FROM items WHERE title = ?1",
                params: vec![Param::text("x")],
            },
        );
        assert!(plan
            .iter()
            .any(|d| d.starts_with("SCAN items") && !d.contains("USING")));
    }
}
