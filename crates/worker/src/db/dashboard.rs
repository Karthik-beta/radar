//! The one D1 batch behind `GET /api/dashboard`. Each list select reads a
//! partial index on `items`, and `parse` validates every row before
//! `api::dashboard::assemble` sees it.

use domain::api::{Action, ItemId, LastAction, RECENT_WINDOW_SECS};
use domain::ids::{ItemKind, Login, RepoName, Timestamp};
use domain::item::ItemState;
use domain::local_state::LocalState;
use domain::stage::Stage;
use domain::turn::Reason;
use serde::Deserialize;
use worker::{D1Database, Result};

use super::rows::{self, tagged, untag, untag_opt, RowError};
use super::sql::{run, Param, Statement};

tagged!(Action {
    Action::Comment => "comment",
    Action::Review => "review",
    Action::ReviewRequested => "review_requested",
    Action::Assigned => "assigned",
    Action::Pushed => "pushed",
});

macro_rules! columns {
    () => {
        "items.node_id, items.repo, items.number, items.kind, items.title, items.url, \
         items.latest_url, items.head_ref, items.state, items.reason, items.since, items.stage, \
         items.stage_since, items.closed_at, items.last_human_at, items.last_human_login, \
         items.last_human_kind, \
         CASE WHEN json_valid(items.requested_reviewers) \
           AND json_type(items.requested_reviewers) = 'array' \
         THEN COALESCE((SELECT group_concat(value, ',') \
           FROM json_each(items.requested_reviewers)), '') END AS reviewers"
    };
}

/// Open items waiting on me, oldest first, with my local state for `visible`.
pub const YOUR_TURN: &str = concat!(
    "SELECT ",
    columns!(),
    ", local_state.kind AS local_kind, local_state.value AS local_value \
     FROM items LEFT JOIN local_state ON local_state.item_id = items.node_id \
     WHERE items.turn = 'me' AND items.state IN ('open', 'draft') \
     ORDER BY items.since ASC"
);

/// My open items waiting on someone else, longest wait first.
pub const THEIR_TURN: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM items WHERE turn = 'them' AND authored_by_me = 1 AND state IN ('open', 'draft') \
     ORDER BY since ASC"
);

/// My open pull requests, by stage.
pub const PIPELINE: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM items WHERE kind = 'pr' AND authored_by_me = 1 AND state IN ('open', 'draft') \
     ORDER BY stage ASC, since ASC"
);

/// My pull requests merged or closed since `?1`, newest first.
pub const RECENT: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM items WHERE state IN ('merged', 'closed') AND closed_at >= ?1 \
     AND kind = 'pr' AND authored_by_me = 1 ORDER BY closed_at DESC"
);

/// My pull requests merged since `?1`, the start of the calendar month. The
/// `IN` term repeats the index predicate, so SQLite can use the partial index.
pub const MERGED_SINCE: &str = "SELECT COUNT(*) AS n FROM items \
     WHERE state IN ('merged', 'closed') AND state = 'merged' AND closed_at >= ?1 \
     AND kind = 'pr' AND authored_by_me = 1";

/// The statements in the order `read` expects their results.
pub fn statements(now: Timestamp) -> Vec<Statement> {
    let recent_since = Param::Int(now.0.saturating_sub(RECENT_WINDOW_SECS));
    vec![
        Statement {
            sql: YOUR_TURN,
            params: Vec::new(),
        },
        Statement {
            sql: THEIR_TURN,
            params: Vec::new(),
        },
        Statement {
            sql: PIPELINE,
            params: Vec::new(),
        },
        Statement {
            sql: RECENT,
            params: vec![recent_since],
        },
        Statement {
            sql: MERGED_SINCE,
            params: vec![Param::ts(now.month_start())],
        },
    ]
}

/// One `items` row as the dashboard needs it, validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRecord {
    pub id: ItemId,
    pub kind: ItemKind,
    pub repo: RepoName,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub latest_url: Option<String>,
    pub head_ref: Option<String>,
    pub state: ItemState,
    pub reason: Option<Reason>,
    pub since: Timestamp,
    pub stage: Option<Stage>,
    pub stage_since: Option<Timestamp>,
    pub closed_at: Option<Timestamp>,
    pub last_human: Option<LastAction>,
    pub requested_reviewers: Vec<Login>,
}

/// A Your turn row and my local state for it, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub item: ItemRecord,
    pub local: Option<LocalState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DashboardRows {
    pub your_turn: Vec<Waiting>,
    pub their_turn: Vec<ItemRecord>,
    pub pipeline: Vec<ItemRecord>,
    pub recent: Vec<ItemRecord>,
    pub merged_this_month: u32,
}

#[derive(Debug, Deserialize)]
pub struct RawItem {
    node_id: String,
    repo: String,
    number: i64,
    kind: String,
    title: String,
    url: String,
    latest_url: Option<String>,
    head_ref: Option<String>,
    state: String,
    reason: Option<String>,
    since: i64,
    stage: Option<String>,
    stage_since: Option<i64>,
    closed_at: Option<i64>,
    last_human_at: Option<i64>,
    last_human_login: Option<String>,
    last_human_kind: Option<String>,
    /// `requested_reviewers` joined with commas, which no login contains, so
    /// the Worker needs no JSON parser for it. `None` when the column is not
    /// a JSON array.
    reviewers: Option<String>,
    #[serde(default)]
    local_kind: Option<String>,
    #[serde(default)]
    local_value: Option<i64>,
}

/// A row with one integer column `n`, as a count or a `RETURNING 1 AS n`.
#[derive(Debug, Deserialize)]
pub struct RawCount {
    pub n: i64,
}

impl RawItem {
    pub fn parse(self) -> Result<Waiting, RowError> {
        let bad_id = |column, source| RowError::BadId { column, source };
        let id = ItemId::parse(&self.node_id).map_err(|_| RowError::UnknownTag {
            column: "node_id",
            value: self.node_id.clone(),
        })?;
        let number =
            u32::try_from(self.number)
                .ok()
                .filter(|n| *n > 0)
                .ok_or(RowError::BadNumber {
                    column: "number",
                    value: self.number,
                })?;
        let last_human = match (
            self.last_human_at,
            self.last_human_login,
            self.last_human_kind,
        ) {
            (None, None, None) => None,
            (Some(at), Some(login), Some(kind)) => Some(LastAction {
                actor: Login::parse(&login).map_err(|e| bad_id("last_human_login", e))?,
                what: untag("last_human_kind", &kind)?,
                at: Timestamp(at),
            }),
            _ => return Err(RowError::PartialLastHuman),
        };
        let reviewers = self.reviewers.ok_or(RowError::UnknownTag {
            column: "requested_reviewers",
            value: "not a JSON array".to_owned(),
        })?;
        let requested_reviewers = reviewers
            .split(',')
            .filter(|r| !r.is_empty())
            .map(|r| Login::parse(r).map_err(|e| bad_id("requested_reviewers", e)))
            .collect::<Result<_, _>>()?;
        let local = match (self.local_kind, self.local_value) {
            (Some(kind), Some(value)) => Some(LocalState::from_columns(&kind, value).ok_or(
                RowError::UnknownTag {
                    column: "local_state.kind",
                    value: kind,
                },
            )?),
            _ => None,
        };
        let item = ItemRecord {
            id,
            kind: untag("kind", &self.kind)?,
            repo: RepoName::parse(&self.repo).map_err(|e| bad_id("repo", e))?,
            number,
            title: self.title,
            url: self.url,
            latest_url: self.latest_url.filter(|url| !url.is_empty()),
            head_ref: self.head_ref,
            state: untag("state", &self.state)?,
            reason: untag_opt("reason", self.reason.as_deref())?,
            since: rows::ts(self.since),
            stage: untag_opt("stage", self.stage.as_deref())?,
            stage_since: self.stage_since.filter(|at| *at > 0).map(Timestamp),
            closed_at: self.closed_at.map(Timestamp),
            last_human,
            requested_reviewers,
        };
        Ok(Waiting { item, local })
    }
}

/// The raw results of `statements`, in order.
#[derive(Debug, Default)]
pub struct RawResults {
    pub your_turn: Vec<RawItem>,
    pub their_turn: Vec<RawItem>,
    pub pipeline: Vec<RawItem>,
    pub recent: Vec<RawItem>,
    pub merged: Vec<RawCount>,
}

impl RawResults {
    /// Drops each row that does not parse and returns why, so one bad row
    /// cannot blank the dashboard.
    pub fn parse(self) -> (DashboardRows, Vec<RowError>) {
        let mut errors = Vec::new();
        let mut keep = |raw: Vec<RawItem>| -> Vec<Waiting> {
            raw.into_iter()
                .filter_map(|r| r.parse().map_err(|e| errors.push(e)).ok())
                .collect()
        };
        let items = |rows: Vec<Waiting>| rows.into_iter().map(|w| w.item).collect();
        let your_turn = keep(self.your_turn);
        let their_turn = items(keep(self.their_turn));
        let pipeline = items(keep(self.pipeline));
        let recent = items(keep(self.recent));
        let merged = self.merged.first().map_or(0, |c| c.n);
        let merged_this_month = match rows::count("merged_this_month", merged) {
            Ok(n) => n,
            Err(e) => {
                errors.push(e);
                0
            }
        };
        let rows = DashboardRows {
            your_turn,
            their_turn,
            pipeline,
            recent,
            merged_this_month,
        };
        (rows, errors)
    }
}

/// Runs the dashboard selects as one D1 batch.
pub async fn read(db: &D1Database, now: Timestamp) -> Result<RawResults> {
    let results = run(db, statements(now)).await?;
    let [your_turn, their_turn, pipeline, recent, merged] = results.as_slice() else {
        return Err(worker::Error::RustError(format!(
            "dashboard batch returned {} results, expected 5",
            results.len()
        )));
    };
    Ok(RawResults {
        your_turn: your_turn.results()?,
        their_turn: their_turn.results()?,
        pipeline: pipeline.results()?,
        recent: recent.results()?,
        merged: merged.results()?,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::db::items::{self, ItemRow, LastHuman};
    use crate::db::local_state;
    use crate::db::synthetic;
    use crate::db::testing::{exec, explain, item_row, memory_db, query_json, returned};
    use domain::item::ItemState;
    use domain::turn::Turn;

    /// Runs `statements(now)` natively and decodes the rows the way D1 would.
    pub fn read_native(conn: &Connection, now: Timestamp) -> RawResults {
        let mut decoded = statements(now).into_iter().map(|stmt| {
            query_json(conn, stmt.sql, &stmt.params)
                .into_iter()
                .map(serde_json::Value::Object)
                .collect::<Vec<_>>()
        });
        let mut next = || decoded.next().unwrap_or_default();
        let items = |rows: Vec<serde_json::Value>| {
            rows.into_iter()
                .map(|v| serde_json::from_value::<RawItem>(v).expect("raw item"))
                .collect()
        };
        RawResults {
            your_turn: items(next()),
            their_turn: items(next()),
            pipeline: items(next()),
            recent: items(next()),
            merged: next()
                .into_iter()
                .map(|v| serde_json::from_value(v).expect("count"))
                .collect(),
        }
    }

    fn seeded(n: u32) -> Connection {
        let conn = memory_db();
        for row in synthetic::items(n).expect("items") {
            exec(&conn, &items::upsert(&row)).expect("seed");
        }
        conn
    }

    /// A full scan of the partial index is fine. A scan of the table is not.
    fn assert_indexed(name: &str, plan: &[String], index: &str) {
        assert!(plan.iter().any(|d| d.contains(index)), "{name}: {plan:?}");
        for detail in plan {
            let scans_table = detail.starts_with("SCAN items") && !detail.contains("USING");
            assert!(!scans_table, "{name} scans the items table: {detail}");
            assert!(!detail.contains("TEMP B-TREE"), "{name} sorts: {detail}");
        }
    }

    #[test]
    fn every_dashboard_select_uses_its_index_without_a_sort() {
        let conn = seeded(2000);
        let indexes = [
            ("YOUR_TURN", "idx_items_me_since"),
            ("THEIR_TURN", "idx_items_them_since"),
            ("PIPELINE", "idx_items_pipeline"),
            ("RECENT", "idx_items_closed"),
            ("MERGED_SINCE", "idx_items_closed"),
        ];
        let stmts = statements(synthetic::NOW);
        assert_eq!(stmts.len(), indexes.len());
        for (stmt, (name, index)) in stmts.iter().zip(indexes) {
            assert_indexed(name, &explain(&conn, stmt), index);
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

    #[test]
    fn stored_action_words_match_the_api_names() {
        for action in <Action as rows::Tagged>::VARIANTS {
            let json = serde_json::to_value(action).expect("json");
            assert_eq!(json.as_str(), Some(rows::Tagged::tag(*action)));
        }
    }

    fn waiting_on_me(n: u32, since: i64, human_at: Option<i64>) -> ItemRow {
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

    #[test]
    fn rows_read_back_with_the_r10_columns_and_local_state() {
        let conn = memory_db();
        let item = waiting_on_me(1, 100, Some(500));
        exec(&conn, &items::upsert(&item)).expect("item");
        conn.execute(
            "UPDATE items SET requested_reviewers = '[\"alice\",\"bob\"]', stage_since = 90, \
             latest_url = 'https://github.com/x/y/pull/1#c' WHERE node_id = 'PR_1'",
            [],
        )
        .expect("extras");
        returned(&conn, &local_state::dismiss("PR_1"));
        let (rows, errors) = read_native(&conn, Timestamp(10_000)).parse();
        assert!(errors.is_empty(), "{errors:?}");
        let [waiting] = rows.your_turn.as_slice() else {
            panic!("one row expected: {rows:?}");
        };
        assert_eq!(
            waiting.local,
            Some(LocalState::Dismissed {
                seen_human_at: Timestamp(500)
            })
        );
        let record = &waiting.item;
        assert_eq!(record.id.as_str(), "PR_1");
        assert_eq!(record.reason, Some(Reason::Mentioned));
        assert_eq!(record.stage_since, Some(Timestamp(90)));
        assert_eq!(
            record.latest_url.as_deref(),
            Some("https://github.com/x/y/pull/1#c")
        );
        let reviewers: Vec<&str> = record
            .requested_reviewers
            .iter()
            .map(Login::as_str)
            .collect();
        assert_eq!(reviewers, ["alice", "bob"]);
        let human = record.last_human.as_ref().expect("last human");
        assert_eq!((human.what, human.at), (Action::Comment, Timestamp(500)));
    }

    #[test]
    fn merged_this_month_counts_my_merges_since_the_first_of_the_month() {
        let conn = memory_db();
        // 2026-11-07T00:00:00Z. The month starts 6 days earlier.
        let now = Timestamp(1_794_009_600);
        let day = 86_400;
        let ended = |n: u32, state: ItemState, at: i64, mine: bool| ItemRow {
            state,
            closed_at: Some(Timestamp(at)),
            authored_by_me: mine,
            stage: None,
            ..item_row(n)
        };
        let rows = [
            ended(1, ItemState::Merged, now.0 - day, true),
            ended(2, ItemState::Merged, now.0 - 6 * day, true),
            ended(3, ItemState::Merged, now.0 - 6 * day - 1, true),
            ended(4, ItemState::Closed, now.0 - day, true),
            ended(5, ItemState::Merged, now.0 - day, false),
        ];
        for row in &rows {
            exec(&conn, &items::upsert(row)).expect("seed");
        }
        let (parsed, errors) = read_native(&conn, now).parse();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(parsed.merged_this_month, 2);
        let recent: Vec<&str> = parsed.recent.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(recent, ["PR_4", "PR_1", "PR_2", "PR_3"]);
    }

    #[test]
    fn a_row_that_does_not_parse_is_dropped_and_reported() {
        let conn = memory_db();
        for n in [1, 2] {
            let theirs = ItemRow {
                authored_by_me: false,
                ..waiting_on_me(n, 100, None)
            };
            exec(&conn, &items::upsert(&theirs)).expect("item");
        }
        conn.execute(
            "UPDATE items SET requested_reviewers = '{\"a\":1}' WHERE node_id = 'PR_2'",
            [],
        )
        .expect("break");
        let (rows, errors) = read_native(&conn, Timestamp(10_000)).parse();
        assert_eq!(rows.your_turn.len(), 1);
        assert_eq!(errors.len(), 1, "{errors:?}");
    }
}
