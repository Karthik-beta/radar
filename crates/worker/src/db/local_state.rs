use domain::ids::Timestamp;

use super::items::ItemRow;
use super::rows::RowError;
use super::sql::{Param, Statement};

const SET: &str = "INSERT INTO local_state (item_id, kind, value) VALUES (?1, ?2, ?3) \
     ON CONFLICT (item_id) DO UPDATE SET kind = excluded.kind, value = excluded.value";

/// What I did to an item inside Radar. Radar never writes it back to GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalState {
    /// Hidden until a human acts after `seen_human_at`. It is `Timestamp(0)`
    /// when the item had no human activity.
    Dismissed { seen_human_at: Timestamp },
    /// Hidden until `until`.
    Snoozed { until: Timestamp },
}

impl LocalState {
    pub fn dismiss(item: &ItemRow) -> Self {
        Self::Dismissed {
            seen_human_at: item.last_human.as_ref().map_or(Timestamp(0), |h| h.at),
        }
    }

    pub fn from_columns(kind: &str, value: i64) -> Result<Self, RowError> {
        match kind {
            "dismissed" => Ok(Self::Dismissed {
                seen_human_at: Timestamp(value),
            }),
            "snoozed" => Ok(Self::Snoozed {
                until: Timestamp(value),
            }),
            _ => Err(RowError::UnknownTag {
                column: "kind",
                value: kind.to_owned(),
            }),
        }
    }

    fn columns(self) -> (&'static str, i64) {
        match self {
            Self::Dismissed { seen_human_at } => ("dismissed", seen_human_at.0),
            Self::Snoozed { until } => ("snoozed", until.0),
        }
    }
}

pub fn set(item_id: &str, state: LocalState) -> Statement {
    let (kind, value) = state.columns();
    Statement {
        sql: SET,
        params: vec![Param::text(item_id), Param::text(kind), Param::Int(value)],
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::db::items::{self, LastHuman};
    use crate::db::queries;
    use crate::db::testing::{exec, item_row, memory_db, query_json};
    use domain::ids::Login;
    use domain::turn::{Reason, Turn};

    fn waiting(n: u32, since: i64, human_at: Option<i64>) -> ItemRow {
        ItemRow {
            turn: Turn::Me(Reason::Mentioned),
            since: Timestamp(since),
            last_human: human_at.map(|at| LastHuman {
                at: Timestamp(at),
                login: Login::parse("octocat").expect("login"),
                kind: "comment".to_owned(),
                url: None,
            }),
            ..item_row(n)
        }
    }

    fn your_turn(conn: &Connection, now: i64) -> Vec<String> {
        let stmt = queries::your_turn(Timestamp(now));
        let params = stmt.params;
        query_json(conn, stmt.sql, &params)
            .iter()
            .map(|r| r["node_id"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    #[test]
    fn dismissed_item_hidden_until_newer_human_activity() {
        let conn = memory_db();
        let item = waiting(1, 100, Some(500));
        exec(&conn, &items::upsert(&item)).expect("item");
        assert_eq!(
            your_turn(&conn, 10_000),
            std::slice::from_ref(&item.node_id)
        );

        exec(&conn, &set(&item.node_id, LocalState::dismiss(&item))).expect("dismiss");
        assert!(your_turn(&conn, 10_000).is_empty());

        let bumped = ItemRow {
            updated_at: Timestamp(item.updated_at.0 + 1),
            ..waiting(1, 100, Some(501))
        };
        exec(&conn, &items::upsert(&bumped)).expect("bump");
        assert_eq!(your_turn(&conn, 10_000), [item.node_id]);
    }

    #[test]
    fn dismissing_an_item_with_no_human_activity_hides_it_until_one_arrives() {
        let conn = memory_db();
        let item = waiting(1, 100, None);
        exec(&conn, &items::upsert(&item)).expect("item");
        exec(&conn, &set(&item.node_id, LocalState::dismiss(&item))).expect("dismiss");
        assert!(your_turn(&conn, 10_000).is_empty());
        let reply = ItemRow {
            updated_at: Timestamp(item.updated_at.0 + 1),
            ..waiting(1, 100, Some(900))
        };
        exec(&conn, &items::upsert(&reply)).expect("reply");
        assert_eq!(your_turn(&conn, 10_000).len(), 1);
    }

    #[test]
    fn snoozed_item_returns_at_wake_time() {
        let conn = memory_db();
        let item = waiting(1, 100, None);
        exec(&conn, &items::upsert(&item)).expect("item");
        let until = Timestamp(1000);
        exec(&conn, &set(&item.node_id, LocalState::Snoozed { until })).expect("snooze");
        assert!(your_turn(&conn, 999).is_empty());
        assert_eq!(your_turn(&conn, 1000).len(), 1);
        assert_eq!(your_turn(&conn, 5000).len(), 1);

        exec(
            &conn,
            &set(
                &item.node_id,
                LocalState::Snoozed {
                    until: Timestamp(2000),
                },
            ),
        )
        .expect("resnooze");
        assert!(your_turn(&conn, 1500).is_empty());
    }

    #[test]
    fn your_turn_query_orders_oldest_first() {
        let conn = memory_db();
        for (n, since) in [(1, 300), (2, 100), (3, 200)] {
            exec(&conn, &items::upsert(&waiting(n, since, None))).expect("item");
        }
        let order = your_turn(&conn, 10_000);
        assert_eq!(order, ["PR_2", "PR_3", "PR_1"]);
    }

    #[test]
    fn a_state_round_trips_through_its_columns() {
        for state in [
            LocalState::Dismissed {
                seen_human_at: Timestamp(7),
            },
            LocalState::Snoozed {
                until: Timestamp(9),
            },
        ] {
            let (kind, value) = state.columns();
            assert_eq!(LocalState::from_columns(kind, value), Ok(state));
        }
        assert!(LocalState::from_columns("archived", 1).is_err());
    }
}
