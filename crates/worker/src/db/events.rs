use domain::activity::ReviewState;
pub use domain::event::{CiOutcome, Event, EventId, EventKind};
use domain::ids::{Login, RepoName, Timestamp};
use serde::{Deserialize, Serialize};

use super::rows::{tagged, Tagged};
use super::sql::{Param, Statement};

const INSERT: &str = "INSERT INTO events (id, item_node_id, repo, kind, review_state, \
     actor_login, actor_is_me, actor_is_bot, occurred_at, source, ingested_at) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) \
     ON CONFLICT (id) DO NOTHING";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Poll,
    Backfill,
    Webhook,
    Agent,
}

tagged!(EventSource {
    EventSource::Poll => "poll",
    EventSource::Backfill => "backfill",
    EventSource::Webhook => "webhook",
    EventSource::Agent => "agent",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRow {
    pub id: EventId,
    pub item_node_id: String,
    pub repo: RepoName,
    pub kind: EventKind,
    pub actor_login: Option<Login>,
    pub actor_is_me: bool,
    pub actor_is_bot: bool,
    pub occurred_at: Timestamp,
    pub source: EventSource,
    pub ingested_at: Timestamp,
}

impl EventRow {
    pub fn from_event(event: &Event, source: EventSource, ingested_at: Timestamp) -> Self {
        let actor = event.actor.as_ref();
        Self {
            id: event.id.clone(),
            item_node_id: event.item.clone(),
            repo: event.repo.clone(),
            kind: event.kind,
            actor_login: actor.map(|a| a.login.clone()),
            actor_is_me: actor.is_some_and(|a| a.is_me),
            actor_is_bot: actor.is_some_and(|a| a.is_bot),
            occurred_at: event.occurred_at,
            source,
            ingested_at,
        }
    }
}

pub fn insert(row: &EventRow) -> Statement {
    Statement {
        sql: INSERT,
        params: vec![
            Param::text(row.id.as_str()),
            Param::text(&row.item_node_id),
            Param::text(row.repo.to_string()),
            Param::text(row.kind.tag()),
            Param::opt_text(row.kind.review_state().map(ReviewState::tag)),
            Param::opt_text(row.actor_login.as_ref().map(Login::as_str)),
            Param::flag(row.actor_is_me),
            Param::flag(row.actor_is_bot),
            Param::ts(row.occurred_at),
            Param::text(row.source.tag()),
            Param::ts(row.ingested_at),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db};

    fn event(id: EventId, kind: EventKind) -> EventRow {
        EventRow {
            id,
            item_node_id: "PR_1".to_owned(),
            repo: RepoName::parse("karthik-beta/radar").expect("repo"),
            kind,
            actor_login: Some(Login::parse("octocat").expect("login")),
            actor_is_me: false,
            actor_is_bot: false,
            occurred_at: Timestamp(1000),
            source: EventSource::Poll,
            ingested_at: Timestamp(2000),
        }
    }

    #[test]
    fn same_timeline_node_is_one_event() {
        let conn = memory_db();
        let row = event(EventId::for_node("IC_kwDOA"), EventKind::Commented);
        exec(&conn, &insert(&row)).expect("first");
        let again = EventRow {
            source: EventSource::Backfill,
            ingested_at: Timestamp(9000),
            ..row
        };
        assert_eq!(exec(&conn, &insert(&again)).expect("second"), 0);
        assert_eq!(count_rows(&conn, "events"), 1);
    }

    #[test]
    fn new_timeline_node_is_a_new_event() {
        let conn = memory_db();
        for node in ["IC_kwDOA", "IC_kwDOB"] {
            let row = event(EventId::for_node(node), EventKind::Commented);
            exec(&conn, &insert(&row)).expect("insert");
        }
        assert_eq!(count_rows(&conn, "events"), 2);
    }

    #[test]
    fn same_ci_transition_is_one_event() {
        let conn = memory_db();
        let failed = |oid| EventId::for_ci("PR_1", oid, CiOutcome::Failed);
        for id in [failed("abc"), failed("abc")] {
            exec(&conn, &insert(&event(id, EventKind::CiFailed))).expect("insert");
        }
        assert_eq!(count_rows(&conn, "events"), 1);
        exec(&conn, &insert(&event(failed("def"), EventKind::CiFailed))).expect("new commit");
        let passed = EventId::for_ci("PR_1", "abc", CiOutcome::Passed);
        exec(&conn, &insert(&event(passed, EventKind::CiPassed))).expect("new outcome");
        assert_eq!(count_rows(&conn, "events"), 3);
    }

    #[test]
    fn unknown_kind_is_rejected() {
        let conn = memory_db();
        let mut stmt = insert(&event(EventId::for_node("IC_1"), EventKind::Commented));
        stmt.params[3] = Param::text("pushed");
        let err = exec(&conn, &stmt).expect_err("must fail");
        assert!(err.to_string().contains("CHECK"), "{err}");
        assert_eq!(count_rows(&conn, "events"), 0);
    }

    #[test]
    fn review_state_only_comes_with_reviewed() {
        let conn = memory_db();
        let mut stmt = insert(&event(EventId::for_node("IC_1"), EventKind::Commented));
        stmt.params[4] = Param::text("approved");
        assert!(exec(&conn, &stmt).is_err());
        let reviewed = EventKind::Reviewed(ReviewState::ChangesRequested);
        exec(&conn, &insert(&event(EventId::for_node("RV_1"), reviewed))).expect("reviewed");
        assert_eq!(
            count_rows(&conn, "events WHERE review_state = 'changes_requested'"),
            1
        );
    }

    #[test]
    fn every_event_kind_is_accepted_by_the_check() {
        let conn = memory_db();
        let kinds = [
            EventKind::Opened,
            EventKind::Commented,
            EventKind::Reviewed(ReviewState::Approved),
            EventKind::ReviewRequested,
            EventKind::Assigned,
            EventKind::Mentioned,
            EventKind::Committed,
            EventKind::Merged,
            EventKind::Closed,
            EventKind::Reopened,
            EventKind::CiFailed,
            EventKind::CiPassed,
        ];
        for (i, kind) in kinds.into_iter().enumerate() {
            let id = EventId::for_node(&format!("E{i}"));
            exec(&conn, &insert(&event(id, kind))).expect("accepted");
        }
        for state in ReviewState::VARIANTS {
            let id = EventId::for_node(&format!("R{}", state.tag()));
            let reviewed = EventKind::Reviewed(*state);
            exec(&conn, &insert(&event(id, reviewed))).expect("accepted");
        }
        assert_eq!(
            count_rows(&conn, "events"),
            kinds.len() + ReviewState::VARIANTS.len()
        );
    }

    #[test]
    fn event_row_reads_from_a_fixture_shape() {
        let json = r#"{"id":"IC_1","item_node_id":"PR_1","repo":"karthik-beta/radar",
            "kind":{"reviewed":"approved"},"actor_login":"octocat","actor_is_me":false,
            "actor_is_bot":false,"occurred_at":"2026-11-02T10:15:00Z","source":"poll",
            "ingested_at":"2026-11-02T10:16:00Z"}"#;
        let row: EventRow = serde_json::from_str(json).expect("parse");
        assert_eq!(row.kind, EventKind::Reviewed(ReviewState::Approved));
        assert_eq!(
            Ok(row.occurred_at),
            Timestamp::parse_rfc3339("2026-11-02T10:15:00Z")
        );
    }
}
