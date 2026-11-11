//! `alerts`, one row per event that was sent or failed. The row holds the event
//! id, a state word, and a count, never the message.

use domain::alert::{AlertItem, MAX_ATTEMPTS};
use domain::event::{Event, EventActor, EventId, EventKind};
use domain::ids::{Login, RepoName, Timestamp};
use domain::local_state::{self, LocalState};
use domain::turn::Turn;
use serde::Deserialize;

use super::rows::{flag, untag, RowError, TurnColumns};
use super::sql::{Param, Statement};

/// How many candidates one dispatch reads. The 6 hour window keeps the set small.
pub const CANDIDATE_LIMIT: i64 = 100;

/// An eligible mention or review request in the age window with no row, or a
/// failed row that has attempts left. Every skip condition `decide` checks is
/// in the WHERE clause, so events that would be skipped never fill the limit
/// and starve the eligible ones behind them.
const CANDIDATES: &str = "SELECT e.id, e.kind, e.repo, e.item_node_id, e.actor_login, \
     e.actor_is_me, e.actor_is_bot, e.occurred_at, \
     i.number, i.title, i.url, i.turn, i.reason, i.state, i.last_human_at, \
     l.kind AS local_kind, l.value AS local_value, COALESCE(a.attempts, 0) AS attempts \
     FROM events e \
     JOIN items i ON i.node_id = e.item_node_id \
     LEFT JOIN local_state l ON l.item_id = i.node_id \
     LEFT JOIN alerts a ON a.event_id = e.id \
     WHERE e.kind IN ('mentioned', 'review_requested') AND e.occurred_at >= ?1 \
       AND (a.event_id IS NULL OR (a.state = 'failed' AND a.attempts < ?2)) \
       AND e.actor_login IS NOT NULL AND e.actor_is_me = 0 AND e.actor_is_bot = 0 \
       AND i.state IN ('open', 'draft') AND i.turn = 'me' \
       AND i.reason = CASE e.kind WHEN 'mentioned' THEN 'mentioned' ELSE 'review_requested' END \
       AND (l.kind IS NULL \
         OR (l.kind = 'dismissed' AND COALESCE(i.last_human_at, 0) > l.value) \
         OR (l.kind = 'snoozed' AND l.value <= ?4)) \
     ORDER BY e.occurred_at, e.id LIMIT ?3";

const RECORD_SENT: &str = "INSERT INTO alerts (event_id, state, attempts, updated_at) \
     VALUES (?1, 'sent', 1, ?2) \
     ON CONFLICT (event_id) DO UPDATE SET state = 'sent', \
     attempts = alerts.attempts + 1, updated_at = excluded.updated_at";

/// A decision that cannot change later. The row keeps the event out of `CANDIDATES`.
const RECORD_SKIPPED: &str = "INSERT INTO alerts (event_id, state, attempts, updated_at) \
     VALUES (?1, 'skipped', 1, ?2) ON CONFLICT (event_id) DO NOTHING";

const RECORD_FAILED: &str = "INSERT INTO alerts (event_id, state, attempts, updated_at) \
     VALUES (?1, 'failed', 1, ?2) \
     ON CONFLICT (event_id) DO UPDATE SET state = 'failed', \
     attempts = alerts.attempts + 1, updated_at = excluded.updated_at";

pub fn candidates(since: Timestamp, now: Timestamp) -> Statement {
    Statement {
        sql: CANDIDATES,
        params: vec![
            Param::ts(since),
            Param::Int(i64::from(MAX_ATTEMPTS)),
            Param::Int(CANDIDATE_LIMIT),
            Param::ts(now),
        ],
    }
}

pub fn record_sent(event_id: &EventId, now: Timestamp) -> Statement {
    Statement {
        sql: RECORD_SENT,
        params: vec![Param::text(event_id.as_str()), Param::ts(now)],
    }
}

pub fn record_skipped(event_id: &EventId, now: Timestamp) -> Statement {
    Statement {
        sql: RECORD_SKIPPED,
        params: vec![Param::text(event_id.as_str()), Param::ts(now)],
    }
}

pub fn record_failed(event_id: &EventId, now: Timestamp) -> Statement {
    Statement {
        sql: RECORD_FAILED,
        params: vec![Param::text(event_id.as_str()), Param::ts(now)],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub event: Event,
    pub item: AlertItem,
}

#[derive(Debug, Deserialize)]
pub struct CandidateRow {
    id: String,
    kind: String,
    repo: String,
    item_node_id: String,
    actor_login: Option<String>,
    actor_is_me: i64,
    actor_is_bot: i64,
    occurred_at: i64,
    number: i64,
    title: String,
    url: String,
    turn: String,
    reason: Option<String>,
    state: String,
    last_human_at: Option<i64>,
    local_kind: Option<String>,
    local_value: Option<i64>,
}

fn event_kind(tag: &str) -> Result<EventKind, RowError> {
    match tag {
        "mentioned" => Ok(EventKind::Mentioned),
        "review_requested" => Ok(EventKind::ReviewRequested),
        other => Err(RowError::UnknownTag {
            column: "events.kind",
            value: other.to_owned(),
        }),
    }
}

impl CandidateRow {
    pub fn into_candidate(self, now: Timestamp) -> Result<Candidate, RowError> {
        let bad_id = |column, source| RowError::BadId { column, source };
        let actor = self
            .actor_login
            .as_deref()
            .map(|login| {
                Ok::<_, RowError>(EventActor {
                    login: Login::parse(login).map_err(|e| bad_id("actor_login", e))?,
                    is_me: flag("actor_is_me", self.actor_is_me)?,
                    is_bot: flag("actor_is_bot", self.actor_is_bot)?,
                })
            })
            .transpose()?;
        let local = match (self.local_kind, self.local_value) {
            (Some(kind), Some(value)) => Some(LocalState::from_columns(&kind, value).ok_or(
                RowError::UnknownTag {
                    column: "local_state.kind",
                    value: kind,
                },
            )?),
            _ => None,
        };
        let turn = Turn::try_from(TurnColumns {
            turn: self.turn,
            reason: self.reason,
        })?;
        let number = u32::try_from(self.number).map_err(|_| RowError::BadNumber {
            column: "number",
            value: self.number,
        })?;
        let visible = local_state::visible(local, self.last_human_at.map(Timestamp), now);
        let event = Event {
            id: EventId::for_node(&self.id),
            item: self.item_node_id,
            repo: RepoName::parse(&self.repo).map_err(|e| bad_id("repo", e))?,
            kind: event_kind(&self.kind)?,
            actor,
            occurred_at: Timestamp(self.occurred_at),
        };
        let item = AlertItem {
            number,
            title: self.title,
            url: self.url,
            turn,
            state: untag("state", &self.state)?,
            visible,
        };
        Ok(Candidate { event, item })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::events::{insert, EventRow, EventSource};
    use crate::db::items;
    use crate::db::testing::{exec, item_row, memory_db, query_json};
    use domain::turn::Reason;

    const NOW: Timestamp = Timestamp(1_800_000_000);

    fn mention(id: &str, item: &str, age: i64) -> EventRow {
        EventRow {
            id: EventId::for_node(id),
            item_node_id: item.to_owned(),
            repo: RepoName::parse("karthik-beta/radar").expect("repo"),
            kind: EventKind::Mentioned,
            actor_login: Some(Login::parse("alice").expect("login")),
            actor_is_me: false,
            actor_is_bot: false,
            occurred_at: Timestamp(NOW.0 - age),
            source: EventSource::Poll,
            ingested_at: NOW,
        }
    }

    fn setup() -> rusqlite::Connection {
        let conn = memory_db();
        let mut row = item_row(1);
        row.turn = Turn::Me(Reason::Mentioned);
        exec(&conn, &items::upsert(&row)).expect("item");
        conn
    }

    fn ids(conn: &rusqlite::Connection) -> Vec<String> {
        let stmt = candidates(Timestamp(NOW.0 - 6 * 3600), NOW);
        query_json(conn, stmt.sql, &stmt.params)
            .iter()
            .map(|r| r["id"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    #[test]
    fn a_fresh_event_is_a_candidate_until_it_is_sent() {
        let conn = setup();
        exec(&conn, &insert(&mention("E1", "PR_1", 60))).expect("event");
        assert_eq!(ids(&conn), ["E1"]);
        exec(&conn, &record_sent(&EventId::for_node("E1"), NOW)).expect("sent");
        assert!(ids(&conn).is_empty());
    }

    #[test]
    fn a_failed_event_returns_until_it_has_three_attempts() {
        let conn = setup();
        exec(&conn, &insert(&mention("E1", "PR_1", 60))).expect("event");
        let id = EventId::for_node("E1");
        for attempt in 1..=3 {
            assert_eq!(ids(&conn), ["E1"], "before attempt {attempt}");
            exec(&conn, &record_failed(&id, NOW)).expect("failed");
        }
        assert!(ids(&conn).is_empty());
    }

    #[test]
    fn skipped_events_cannot_starve_an_eligible_one() {
        let conn = setup();
        let mut other = item_row(2);
        other.turn = Turn::Them;
        exec(&conn, &items::upsert(&other)).expect("item");
        let node = other.node_id.clone();
        for n in 0..(CANDIDATE_LIMIT + 5) {
            let mut row = mention(&format!("S{n}"), &node, 3000 + n);
            row.actor_is_bot = n % 2 == 0;
            exec(&conn, &insert(&row)).expect("skipped event");
        }
        exec(&conn, &insert(&mention("E1", "PR_1", 60))).expect("event");
        assert_eq!(ids(&conn), ["E1"]);
    }

    #[test]
    fn a_skipped_row_keeps_an_event_out() {
        let conn = setup();
        exec(&conn, &insert(&mention("E1", "PR_1", 60))).expect("event");
        exec(&conn, &record_skipped(&EventId::for_node("E1"), NOW)).expect("skipped");
        assert!(ids(&conn).is_empty());
    }

    #[test]
    fn an_event_outside_the_window_is_not_a_candidate() {
        let conn = setup();
        exec(&conn, &insert(&mention("E1", "PR_1", 7 * 3600))).expect("event");
        assert!(ids(&conn).is_empty());
    }

    #[test]
    fn rows_convert_to_candidates_with_visibility() {
        let conn = setup();
        exec(&conn, &insert(&mention("E1", "PR_1", 60))).expect("event");
        let stmt = candidates(Timestamp(0), NOW);
        let rows = query_json(&conn, stmt.sql, &stmt.params);
        let row: CandidateRow =
            serde_json::from_value(serde_json::Value::Object(rows[0].clone())).expect("row");
        let candidate = row.into_candidate(NOW).expect("candidate");
        assert_eq!(candidate.item.turn, Turn::Me(Reason::Mentioned));
        assert!(candidate.item.visible);
        assert_eq!(candidate.event.kind, EventKind::Mentioned);
    }
}
