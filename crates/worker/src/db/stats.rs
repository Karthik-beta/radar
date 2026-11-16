//! The batch behind `GET /api/stats`: indexed selects over the fact tables, one
//! D1 batch per request. Each statement only selects rows. `domain::stats`
//! turns them into numbers.

use domain::activity::ReviewState;
use domain::facts::Side;
use domain::ids::{RepoName, Timestamp};
use domain::stage::Stage;
use domain::stats::{ImportState, MergeRow, Period, PrRow, ReviewRow, StageRow};
use serde::de::DeserializeOwned;
use serde_json::Value;
use worker::{D1Database, D1Result, Result};

use super::rows::{self, untag, RowError};
use super::sql::{run, Param, Statement};
use super::trends::{self, RawTrends};

macro_rules! pr_select {
    () => {
        "SELECT f.repo, f.external, f.created_at, f.merged_at, f.closed_at, \
         f.first_response_at, f.changes_requested_rounds, f.ci_failures, \
         f.additions + f.deletions AS lines, f.agent, f.source = 'live' AS live, \
         r.stars, r.primary_language AS language \
         FROM pr_facts f LEFT JOIN repos r ON r.full_name = f.repo "
    };
}

/// Pull requests opened or merged since `?1`.
const PRS: &str = concat!(
    pr_select!(),
    "WHERE f.created_at >= ?1 OR f.merged_at >= ?1"
);

/// Pull requests still open that opened before `?1`, for the count of stale ones.
const PRS_OPEN: &str = concat!(
    pr_select!(),
    "WHERE f.merged_at IS NULL AND f.closed_at IS NULL AND f.created_at < ?1"
);

/// Every merge ever, for first merges per repo and the 50th and 100th.
const MERGES: &str = "SELECT f.repo, f.external, f.merged_at, r.stars \
     FROM pr_facts f LEFT JOIN repos r ON r.full_name = f.repo \
     WHERE f.merged_at IS NOT NULL ORDER BY f.merged_at";

const REVIEWS: &str =
    "SELECT repo, external, state, submitted_at FROM review_facts WHERE submitted_at >= ?1";

/// Every transition since `?1`, and for each pull request still active at `?1` its
/// last transition before it, so a segment that spans the start is counted.
/// The rows come grouped by pull request, oldest first.
const TRANSITIONS: &str = "SELECT t.item_node_id, f.external, t.stage, t.turn, t.at, t.id, \
     COALESCE(f.merged_at, f.closed_at) AS ended_at \
     FROM pr_transitions t JOIN pr_facts f ON f.node_id = t.item_node_id WHERE t.at >= ?1 \
     UNION ALL \
     SELECT t.item_node_id, f.external, t.stage, t.turn, t.at, t.id, \
     COALESCE(f.merged_at, f.closed_at) AS ended_at \
     FROM pr_transitions t JOIN pr_facts f ON f.node_id = t.item_node_id \
     WHERE t.at < ?1 AND COALESCE(f.merged_at, f.closed_at, ?1) >= ?1 \
     AND NOT EXISTS (SELECT 1 FROM pr_transitions n WHERE n.item_node_id = t.item_node_id \
       AND n.at < ?1 AND (n.at > t.at OR (n.at = t.at AND n.id > t.id))) \
     ORDER BY item_node_id, at, id";

const ISSUES: &str = "SELECT COUNT(*) AS n FROM (SELECT DISTINCT l.issue_repo, l.issue_number \
     FROM pr_issue_links l JOIN pr_facts f ON f.node_id = l.pr_node_id \
     WHERE f.external = 1 AND f.merged_at >= ?1 \
     AND (l.issue_authored_by_me = 1 OR l.issue_commented_by_me = 1))";

const EVENTS: &str = "SELECT occurred_at FROM events WHERE actor_is_me = 1 AND occurred_at >= ?1 \
     UNION ALL \
     SELECT occurred_at FROM own_events WHERE actor_is_me = 1 AND occurred_at >= ?1";

const SETTINGS: &str = "SELECT s.utc_offset_minutes, s.goal_merged_per_month, \
     i.facts_imported_at, i.account_created_at \
     FROM settings s CROSS JOIN import_state i WHERE s.id = 1 AND i.id = 1";

/// The three Trends selects first, then the eight Stats selects.
pub fn statements(period: Period, now: Timestamp) -> Vec<Statement> {
    let since = Param::ts(period.fetch_from(now));
    let from = |sql| Statement {
        sql,
        params: vec![since.clone()],
    };
    let mut all = trends::statements(now);
    all.extend([PRS, PRS_OPEN, REVIEWS, TRANSITIONS, EVENTS].map(from));
    all.push(Statement {
        sql: MERGES,
        params: Vec::new(),
    });
    all.push(Statement {
        sql: ISSUES,
        params: vec![Param::ts(period.span(now).from)],
    });
    all.push(Statement {
        sql: SETTINGS,
        params: Vec::new(),
    });
    all
}

/// One selected row. The columns are read by name and checked here, once, so
/// the batch needs no struct per select, and each `Deserialize` impl that
/// would add to the Worker's size is left out.
struct Row<'a>(&'a Value);

type Parsed<T> = std::result::Result<T, RowError>;

impl Row<'_> {
    fn opt_int(&self, column: &'static str) -> Parsed<Option<i64>> {
        match self.0.get(column) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value.as_i64().map(Some).ok_or(RowError::Missing(column)),
        }
    }

    fn int(&self, column: &'static str) -> Parsed<i64> {
        self.opt_int(column)?.ok_or(RowError::Missing(column))
    }

    fn flag(&self, column: &'static str) -> Parsed<bool> {
        rows::flag(column, self.int(column)?)
    }

    fn count(&self, column: &'static str) -> Parsed<u32> {
        rows::count(column, self.int(column)?)
    }

    fn ts(&self, column: &'static str) -> Parsed<Timestamp> {
        self.int(column).map(rows::ts)
    }

    fn opt_ts(&self, column: &'static str) -> Parsed<Option<Timestamp>> {
        Ok(self.opt_int(column)?.map(rows::ts))
    }

    fn opt_text(&self, column: &'static str) -> Parsed<Option<&str>> {
        match self.0.get(column) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value.as_str().map(Some).ok_or(RowError::Missing(column)),
        }
    }

    fn text(&self, column: &'static str) -> Parsed<&str> {
        self.opt_text(column)?.ok_or(RowError::Missing(column))
    }

    fn repo(&self) -> Parsed<RepoName> {
        RepoName::parse(self.text("repo")?).map_err(|source| RowError::BadId {
            column: "repo",
            source,
        })
    }

    fn stars(&self) -> Parsed<Option<u32>> {
        self.opt_int("stars")?
            .map(|n| rows::count("stars", n))
            .transpose()
    }
}

pub fn parse_pr(row: &Value) -> Parsed<PrRow> {
    let row = Row(row);
    Ok(PrRow {
        repo: row.repo()?,
        external: row.flag("external")?,
        created_at: row.ts("created_at")?,
        merged_at: row.opt_ts("merged_at")?,
        closed_at: row.opt_ts("closed_at")?,
        first_response_at: row.opt_ts("first_response_at")?,
        changes_requested_rounds: row.count("changes_requested_rounds")?,
        ci_failures: row.count("ci_failures")?,
        lines: row.count("lines")?,
        agent: row.flag("agent")?,
        live: row.flag("live")?,
        stars: row.stars()?,
        language: row.opt_text("language")?.map(str::to_owned),
    })
}

pub fn parse_merge(row: &Value) -> Parsed<MergeRow> {
    let row = Row(row);
    Ok(MergeRow {
        repo: row.repo()?,
        external: row.flag("external")?,
        at: row.ts("merged_at")?,
        stars: row.stars()?,
    })
}

pub fn parse_review(row: &Value) -> Parsed<ReviewRow> {
    let row = Row(row);
    Ok(ReviewRow {
        repo: row.repo()?,
        external: row.flag("external")?,
        state: untag::<ReviewState>("state", row.text("state")?)?,
        at: row.ts("submitted_at")?,
    })
}

pub fn parse_transition(row: &Value) -> Parsed<StageRow> {
    let row = Row(row);
    Ok(StageRow {
        item: row.text("item_node_id")?.to_owned(),
        external: row.flag("external")?,
        stage: untag::<Stage>("stage", row.text("stage")?)?,
        side: untag::<Side>("turn", row.text("turn")?)?,
        at: row.ts("at")?,
        ended_at: row.opt_ts("ended_at")?,
    })
}

/// The selected rows, as D1 returned them.
#[derive(Debug, Default)]
pub struct RawStats {
    pub trends: RawTrends,
    pub prs: Vec<Value>,
    pub reviews: Vec<Value>,
    pub transitions: Vec<Value>,
    pub merges: Vec<Value>,
    pub events: Vec<Value>,
    pub counts: Vec<Value>,
    pub settings: Vec<Value>,
}

impl RawStats {
    fn settings(&self) -> Row<'_> {
        Row(self.settings.first().unwrap_or(&Value::Null))
    }

    pub fn utc_offset_minutes(&self) -> i64 {
        self.settings().int("utc_offset_minutes").unwrap_or(0)
    }

    pub fn goal_merged_per_month(&self) -> Option<i64> {
        self.settings()
            .opt_int("goal_merged_per_month")
            .ok()
            .flatten()
    }

    pub fn closed_issues(&self) -> i64 {
        Row(self.counts.first().unwrap_or(&Value::Null))
            .int("n")
            .unwrap_or(0)
    }

    pub fn import(&self) -> ImportState {
        let settings = self.settings();
        ImportState {
            facts_imported_at: settings.opt_ts("facts_imported_at").ok().flatten(),
            account_created_at: settings.opt_ts("account_created_at").ok().flatten(),
        }
    }

    /// When I acted. A row without a time is skipped.
    pub fn event_times(&self) -> Vec<Timestamp> {
        self.events
            .iter()
            .filter_map(|row| Row(row).ts("occurred_at").ok())
            .collect()
    }
}

fn select<T: DeserializeOwned>(results: &[D1Result], i: usize) -> Result<Vec<T>> {
    results
        .get(i)
        .ok_or_else(|| worker::Error::RustError("stats batch returned too few results".into()))?
        .results()
}

pub async fn read(db: &D1Database, period: Period, now: Timestamp) -> Result<RawStats> {
    let results = run(db, statements(period, now)).await?;
    let mut prs = select(&results, 3)?;
    prs.extend(select(&results, 4)?);
    Ok(RawStats {
        trends: RawTrends {
            prs: select(&results, 0)?,
            events: select(&results, 1)?,
            rollups: select(&results, 2)?,
        },
        prs,
        reviews: select(&results, 5)?,
        transitions: select(&results, 6)?,
        events: select(&results, 7)?,
        merges: select(&results, 8)?,
        counts: select(&results, 9)?,
        settings: select(&results, 10)?,
    })
}

#[cfg(test)]
mod tests {
    use domain::stats::Grain;
    use serde_json::{Map, Value};

    use super::*;
    use crate::db::testing::{explain, memory_db, query_json};

    const NOW: Timestamp = Timestamp(1_794_830_400);
    const PERIOD: Period = Period::Last12Months(Grain::Month);

    fn batch(conn: &rusqlite::Connection, period: Period) -> Vec<Vec<Map<String, Value>>> {
        statements(period, NOW)
            .iter()
            .map(|s| query_json(conn, s.sql, &s.params))
            .collect()
    }

    fn seed(conn: &rusqlite::Connection) {
        let since = PERIOD.fetch_from(NOW).0;
        conn.execute_batch(&format!(
            "INSERT INTO repos (full_name, open_prs, merged_prs, closed_prs, updated_at, stars, \
               primary_language) VALUES ('o/big', 0, 1, 0, 0, 2000, 'Rust');
             INSERT INTO pr_facts (node_id, repo, external, number, created_at, merged_at, \
               closed_at, changes_requested_rounds, ci_failures, additions, deletions, \
               changed_files, agent, source) VALUES \
               ('PR_a', 'o/big', 1, 1, {c}, {m}, {m}, 1, 0, 30, 20, 2, 0, 'live'), \
               ('PR_b', 'o/big', 1, 2, {old}, NULL, NULL, 0, 0, 1, 1, 1, 1, 'imported'), \
               ('PR_c', 'o/big', 1, 3, {older}, {old}, {old}, 0, 0, 1, 1, 1, 0, 'imported');
             INSERT INTO review_facts VALUES ('RV_1', 'o/big', 1, 5, 'x', 'approved', {c}, 'live'), \
               ('RV_0', 'o/big', 1, 5, 'x', 'approved', {before}, 'live');
             INSERT INTO pr_transitions (item_node_id, stage, turn, at) VALUES \
               ('PR_a', 'awaiting_review', 'them', {c}), \
               ('PR_c', 'awaiting_review', 'them', {before}), \
               ('PR_b', 'ci_running', 'them', {long_before}), \
               ('PR_b', 'awaiting_review', 'them', {before_b});
             INSERT INTO pr_issue_links VALUES ('PR_a', 'o/big', 9, 1, 0), \
               ('PR_a', 'o/big', 10, 0, 0), ('PR_c', 'o/big', 11, 1, 1);
             INSERT INTO events (id, item_node_id, repo, kind, actor_login, actor_is_me, \
               actor_is_bot, occurred_at, source, ingested_at) VALUES \
               ('e1', 'PR_a', 'o/big', 'commented', 'me', 1, 0, {c}, 'poll', 0), \
               ('e2', 'PR_a', 'o/big', 'commented', 'x', 0, 0, {c}, 'poll', 0);
             INSERT INTO own_events SELECT * FROM events WHERE id = 'e1';
             UPDATE own_events SET id = 'e3';
             UPDATE settings SET utc_offset_minutes = 330, goal_merged_per_month = 4;
             UPDATE import_state SET facts_imported_at = 5, account_created_at = 1;",
            c = since + 100,
            m = since + 5_000,
            old = since - 100,
            older = since - 9_999,
            before = since - 1,
            long_before = since - 900,
            before_b = since - 500,
        ))
        .expect("seed");
    }

    #[test]
    fn one_batch_holds_the_three_trends_selects_and_the_eight_stats_selects() {
        assert_eq!(statements(PERIOD, NOW).len(), 11);
        assert_eq!(statements(Period::YearOverYear, NOW).len(), 11);
    }

    #[test]
    fn each_select_reads_the_rows_it_names() {
        let conn = memory_db();
        seed(&conn);
        let got = batch(&conn, PERIOD);
        assert_eq!(got[3].len(), 1, "created or merged in the window");
        assert_eq!(got[4].len(), 1, "open before the window");
        assert_eq!(got[3][0]["stars"], 2000);
        assert_eq!(got[3][0]["language"], "Rust");
        assert_eq!(got[3][0]["lines"], 50);
        assert_eq!(got[5].len(), 1, "reviews since the window");
        let since = PERIOD.fetch_from(NOW).0;
        let transitions: Vec<(&str, i64)> = got[6]
            .iter()
            .map(|r| {
                (
                    r["item_node_id"].as_str().unwrap_or(""),
                    r["at"].as_i64().unwrap_or(0),
                )
            })
            .collect();
        assert_eq!(
            transitions,
            [("PR_a", since + 100), ("PR_b", since - 500)],
            "since the window, plus the last one before it for an active pull request, by pull request"
        );
        assert_eq!(got[6][0]["ended_at"], since + 5_000);
        assert!(got[6][1]["ended_at"].is_null());
        assert_eq!(
            got[7].len(),
            2,
            "my events from both tables, not other people's"
        );
        assert_eq!(got[8].len(), 2, "every merge ever, oldest first");
        assert!(got[8][0]["merged_at"].as_i64() < got[8][1]["merged_at"].as_i64());
        assert_eq!(
            got[9][0]["n"], 1,
            "issues on merged pull requests since the window"
        );
        assert_eq!(got[10][0]["utc_offset_minutes"], 330);
        assert_eq!(got[10][0]["goal_merged_per_month"], 4);
    }

    #[test]
    fn transitions_at_the_same_second_keep_their_insert_order() {
        let conn = memory_db();
        seed(&conn);
        let at = PERIOD.fetch_from(NOW).0 + 200;
        conn.execute_batch(&format!(
            "INSERT INTO pr_transitions (item_node_id, stage, turn, at) VALUES \
               ('PR_a', 'changes_requested', 'me', {at}), \
               ('PR_a', 'awaiting_review', 'them', {at});"
        ))
        .expect("seed");
        let got = batch(&conn, PERIOD);
        let same: Vec<&str> = got[6]
            .iter()
            .filter(|r| r["at"].as_i64() == Some(at))
            .map(|r| r["stage"].as_str().unwrap_or(""))
            .collect();
        assert_eq!(same, ["changes_requested", "awaiting_review"]);
    }

    #[test]
    fn year_over_year_reads_from_last_january() {
        let since = Period::YearOverYear.fetch_from(NOW);
        assert_eq!(
            statements(Period::YearOverYear, NOW)[3].params,
            vec![Param::ts(since)]
        );
        assert!(since < PERIOD.fetch_from(NOW));
    }

    #[test]
    fn the_big_tables_are_read_through_an_index() {
        let conn = memory_db();
        let all = statements(PERIOD, NOW);
        for i in [3, 4, 5, 6, 7, 8] {
            let plan = explain(&conn, &all[i]);
            for line in plan.iter().filter(|l| l.starts_with("SCAN")) {
                assert!(
                    line.contains("USING"),
                    "statement {i} scans a table: {line}"
                );
            }
        }
    }

    #[test]
    fn rows_parse_and_a_bad_repo_is_a_row_error() {
        let conn = memory_db();
        seed(&conn);
        let got = batch(&conn, PERIOD);
        let value = |row: &Map<String, Value>| Value::Object(row.clone());
        let pr = parse_pr(&value(&got[3][0])).expect("pr");
        assert_eq!((pr.lines, pr.stars, pr.live), (50, Some(2000), true));
        assert_eq!(pr.language.as_deref(), Some("Rust"));
        let merge = parse_merge(&value(&got[8][1])).expect("merge");
        assert!(merge.external);
        parse_review(&value(&got[5][0])).expect("review");
        let transition = parse_transition(&value(&got[6][0])).expect("transition");
        assert!(transition.ended_at.is_some());
        let bad = serde_json::json!({"repo": "nope", "external": 1, "merged_at": 1});
        assert!(parse_merge(&bad).is_err());
        let missing = serde_json::json!({"repo": "a/b", "external": 1});
        assert!(matches!(
            parse_merge(&missing),
            Err(RowError::Missing("merged_at"))
        ));
    }

    #[test]
    fn the_single_row_selects_read_through_the_accessors() {
        let conn = memory_db();
        seed(&conn);
        let got = batch(&conn, PERIOD);
        let raw = RawStats {
            counts: got[9].iter().cloned().map(Value::Object).collect(),
            settings: got[10].iter().cloned().map(Value::Object).collect(),
            events: got[7].iter().cloned().map(Value::Object).collect(),
            ..RawStats::default()
        };
        assert_eq!(raw.closed_issues(), 1);
        assert_eq!(raw.utc_offset_minutes(), 330);
        assert_eq!(raw.goal_merged_per_month(), Some(4));
        assert_eq!(raw.import().facts_imported_at, Some(Timestamp(5)));
        assert_eq!(raw.event_times().len(), 2);
        let empty = RawStats::default();
        assert_eq!((empty.utc_offset_minutes(), empty.closed_issues()), (0, 0));
        assert_eq!(empty.goal_merged_per_month(), None);
        assert_eq!(empty.import().facts_imported_at, None);
    }
}
