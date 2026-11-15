//! The contribution fact tables (ADR 0010). Each table takes one `json_each`
//! statement per batch, as `events::insert_many` does, so an Enrich batch adds at
//! most four D1 statements whatever its size. `WHERE true` keeps SQLite from
//! reading `ON` as a join constraint.

use domain::activity::ReviewState;
pub use domain::facts::{FactSource, IssueLink, PrFact, ReviewFact, Side, Transition};
use domain::ids::Login;
use domain::stage::Stage;
use serde::Serialize;

use super::rows::Tagged;
use super::sql::{Param, Statement};

/// `ci_failures` is the larger of what the batch saw and what `events` and
/// `own_events` hold, and the stored count never drops, so a rollup that moves
/// the events cannot lower it. The first response keeps the earliest one known,
/// because the timeline window can end after it. A live row replaces an imported one.
const UPSERT_PRS: &str = "INSERT INTO pr_facts (node_id, repo, external, number, created_at, \
     merged_at, closed_at, first_response_at, first_response_login, changes_requested_rounds, \
     ci_failures, additions, deletions, changed_files, agent, source) \
     SELECT f.node_id, f.repo, f.external, f.number, f.created_at, f.merged_at, f.closed_at, \
     f.first_response_at, f.first_response_login, f.changes_requested_rounds, \
     MAX(f.ci_failures, \
       (SELECT COUNT(*) FROM events e WHERE e.item_node_id = f.node_id AND e.kind = 'ci_failed') \
       + (SELECT COUNT(*) FROM own_events o WHERE o.item_node_id = f.node_id \
          AND o.kind = 'ci_failed')), \
     f.additions, f.deletions, f.changed_files, f.agent, f.source \
     FROM (SELECT json_extract(value, '$.node_id') AS node_id, \
       json_extract(value, '$.repo') AS repo, json_extract(value, '$.external') AS external, \
       json_extract(value, '$.number') AS number, \
       json_extract(value, '$.created_at') AS created_at, \
       json_extract(value, '$.merged_at') AS merged_at, \
       json_extract(value, '$.closed_at') AS closed_at, \
       json_extract(value, '$.first_response_at') AS first_response_at, \
       json_extract(value, '$.first_response_login') AS first_response_login, \
       json_extract(value, '$.changes_requested_rounds') AS changes_requested_rounds, \
       json_extract(value, '$.ci_failures') AS ci_failures, \
       json_extract(value, '$.additions') AS additions, \
       json_extract(value, '$.deletions') AS deletions, \
       json_extract(value, '$.changed_files') AS changed_files, \
       json_extract(value, '$.agent') AS agent, json_extract(value, '$.source') AS source \
       FROM json_each(?1)) AS f WHERE true \
     ON CONFLICT (node_id) DO UPDATE SET \
     repo = excluded.repo, external = excluded.external, number = excluded.number, \
     created_at = excluded.created_at, merged_at = excluded.merged_at, \
     closed_at = excluded.closed_at, \
     first_response_at = CASE WHEN pr_facts.first_response_at IS NULL \
       OR excluded.first_response_at < pr_facts.first_response_at \
       THEN excluded.first_response_at ELSE pr_facts.first_response_at END, \
     first_response_login = CASE WHEN pr_facts.first_response_at IS NULL \
       OR excluded.first_response_at < pr_facts.first_response_at \
       THEN excluded.first_response_login ELSE pr_facts.first_response_login END, \
     changes_requested_rounds = excluded.changes_requested_rounds, \
     ci_failures = MAX(pr_facts.ci_failures, excluded.ci_failures), \
     additions = excluded.additions, deletions = excluded.deletions, \
     changed_files = excluded.changed_files, agent = excluded.agent, source = excluded.source \
     WHERE excluded.source = 'live' OR pr_facts.source = 'imported'";

/// The flags also look at what D1 already holds, because the issue and my
/// comment on it usually arrived in an earlier batch. Each enrich refreshes them.
const UPSERT_LINKS: &str = "INSERT INTO pr_issue_links (pr_node_id, issue_repo, issue_number, \
     issue_authored_by_me, issue_commented_by_me) \
     SELECT l.pr, l.repo, l.number, \
     MAX(l.authored, EXISTS (SELECT 1 FROM items i WHERE i.repo = l.repo AND i.number = l.number \
       AND i.kind = 'issue' AND i.authored_by_me = 1)), \
     MAX(l.commented, \
       EXISTS (SELECT 1 FROM items i JOIN events e ON e.item_node_id = i.node_id \
         WHERE i.repo = l.repo AND i.number = l.number AND i.kind = 'issue' \
         AND e.kind = 'commented' AND e.actor_is_me = 1), \
       EXISTS (SELECT 1 FROM items i JOIN own_events o ON o.item_node_id = i.node_id \
         WHERE i.repo = l.repo AND i.number = l.number AND i.kind = 'issue' \
         AND o.kind = 'commented' AND o.actor_is_me = 1)) \
     FROM (SELECT json_extract(value, '$.pr') AS pr, json_extract(value, '$.repo') AS repo, \
       json_extract(value, '$.number') AS number, \
       json_extract(value, '$.authored') AS authored, \
       json_extract(value, '$.commented') AS commented FROM json_each(?1)) AS l WHERE true \
     ON CONFLICT (pr_node_id, issue_repo, issue_number) DO UPDATE SET \
     issue_authored_by_me = excluded.issue_authored_by_me, \
     issue_commented_by_me = excluded.issue_commented_by_me";

const INSERT_TRANSITIONS: &str = "INSERT INTO pr_transitions (item_node_id, stage, turn, at) \
     SELECT json_extract(value, '$.item'), json_extract(value, '$.stage'), \
     json_extract(value, '$.turn'), json_extract(value, '$.at') FROM json_each(?1) WHERE true \
     ON CONFLICT (item_node_id, at, stage, turn) DO NOTHING";

const UPSERT_REVIEWS: &str = "INSERT INTO review_facts (review_id, repo, external, pr_number, \
     pr_author_login, state, submitted_at, source) \
     SELECT json_extract(value, '$.review_id'), json_extract(value, '$.repo'), \
     json_extract(value, '$.external'), json_extract(value, '$.pr_number'), \
     json_extract(value, '$.pr_author_login'), json_extract(value, '$.state'), \
     json_extract(value, '$.submitted_at'), json_extract(value, '$.source') \
     FROM json_each(?1) WHERE true \
     ON CONFLICT (review_id) DO UPDATE SET \
     repo = excluded.repo, external = excluded.external, pr_number = excluded.pr_number, \
     pr_author_login = excluded.pr_author_login, state = excluded.state, \
     submitted_at = excluded.submitted_at, source = excluded.source \
     WHERE excluded.source = 'live' OR review_facts.source = 'imported'";

/// Everything one batch learned for the fact tables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    pub prs: Vec<PrFact>,
    pub reviews: Vec<ReviewFact>,
    pub transitions: Vec<Transition>,
    pub links: Vec<IssueLink>,
}

impl Facts {
    pub fn extend(&mut self, other: Facts) {
        self.prs.extend(other.prs);
        self.reviews.extend(other.reviews);
        self.transitions.extend(other.transitions);
        self.links.extend(other.links);
    }

    /// One statement per table that has rows, in table order.
    pub fn statements(&self) -> Vec<Statement> {
        let mut statements = Vec::with_capacity(4);
        if !self.prs.is_empty() {
            statements.push(upsert_prs(&self.prs));
        }
        if !self.links.is_empty() {
            statements.push(upsert_links(&self.links));
        }
        if !self.transitions.is_empty() {
            statements.push(insert_transitions(&self.transitions));
        }
        if !self.reviews.is_empty() {
            statements.push(upsert_reviews(&self.reviews));
        }
        statements
    }
}

fn json_param<T: Serialize>(rows: &[T]) -> Param {
    Param::text(serde_json::to_string(rows).unwrap_or_else(|_| "[]".to_owned()))
}

#[derive(Serialize)]
struct FlatPr<'a> {
    node_id: &'a str,
    repo: String,
    external: u8,
    number: u32,
    created_at: i64,
    merged_at: Option<i64>,
    closed_at: Option<i64>,
    first_response_at: Option<i64>,
    first_response_login: Option<&'a str>,
    changes_requested_rounds: u32,
    ci_failures: u32,
    additions: u32,
    deletions: u32,
    changed_files: u32,
    agent: u8,
    source: &'static str,
}

pub fn upsert_prs(rows: &[PrFact]) -> Statement {
    let flat: Vec<FlatPr<'_>> = rows
        .iter()
        .map(|f| FlatPr {
            node_id: &f.node_id,
            repo: f.repo.to_string(),
            external: u8::from(f.external),
            number: f.number,
            created_at: f.created_at.0,
            merged_at: f.merged_at.map(|t| t.0),
            closed_at: f.closed_at.map(|t| t.0),
            first_response_at: f.first_response_at.map(|t| t.0),
            first_response_login: f.first_response_login.as_ref().map(Login::as_str),
            changes_requested_rounds: f.changes_requested_rounds,
            ci_failures: f.ci_failures,
            additions: f.additions,
            deletions: f.deletions,
            changed_files: f.changed_files,
            agent: u8::from(f.agent),
            source: f.source.tag(),
        })
        .collect();
    Statement {
        sql: UPSERT_PRS,
        params: vec![json_param(&flat)],
    }
}

#[derive(Serialize)]
struct FlatLink<'a> {
    pr: &'a str,
    repo: String,
    number: u32,
    authored: u8,
    commented: u8,
}

pub fn upsert_links(rows: &[IssueLink]) -> Statement {
    let flat: Vec<FlatLink<'_>> = rows
        .iter()
        .map(|l| FlatLink {
            pr: &l.pr_node_id,
            repo: l.issue_repo.to_string(),
            number: l.issue_number,
            authored: u8::from(l.issue_authored_by_me),
            commented: u8::from(l.issue_commented_by_me),
        })
        .collect();
    Statement {
        sql: UPSERT_LINKS,
        params: vec![json_param(&flat)],
    }
}

#[derive(Serialize)]
struct FlatTransition<'a> {
    item: &'a str,
    stage: &'static str,
    turn: &'static str,
    at: i64,
}

pub fn insert_transitions(rows: &[Transition]) -> Statement {
    let flat: Vec<FlatTransition<'_>> = rows
        .iter()
        .map(|t| FlatTransition {
            item: &t.item_node_id,
            stage: Stage::tag(t.stage),
            turn: t.turn.tag(),
            at: t.at.0,
        })
        .collect();
    Statement {
        sql: INSERT_TRANSITIONS,
        params: vec![json_param(&flat)],
    }
}

#[derive(Serialize)]
struct FlatReview<'a> {
    review_id: &'a str,
    repo: String,
    external: u8,
    pr_number: u32,
    pr_author_login: &'a str,
    state: &'static str,
    submitted_at: i64,
    source: &'static str,
}

pub fn upsert_reviews(rows: &[ReviewFact]) -> Statement {
    let flat: Vec<FlatReview<'_>> = rows
        .iter()
        .map(|r| FlatReview {
            review_id: &r.review_id,
            repo: r.repo.to_string(),
            external: u8::from(r.external),
            pr_number: r.pr_number,
            pr_author_login: r.pr_author_login.as_str(),
            state: ReviewState::tag(r.state),
            submitted_at: r.submitted_at.0,
            source: r.source.tag(),
        })
        .collect();
    Statement {
        sql: UPSERT_REVIEWS,
        params: vec![json_param(&flat)],
    }
}

#[cfg(test)]
mod tests {
    use domain::ids::{RepoName, Timestamp};

    use super::*;
    use crate::db::events::{insert_many, CiOutcome, EventId, EventKind, EventRow, EventSource};
    use crate::db::rollups::WeekStart;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};

    fn repo(name: &str) -> RepoName {
        RepoName::parse(name).expect("repo")
    }

    fn login(name: &str) -> Login {
        Login::parse(name).expect("login")
    }

    fn pr(node_id: &str, ci_failures: u32) -> PrFact {
        PrFact {
            node_id: node_id.to_owned(),
            repo: repo("octo/widgets"),
            external: true,
            number: 7,
            created_at: Timestamp(1_000),
            merged_at: None,
            closed_at: None,
            first_response_at: None,
            first_response_login: None,
            changes_requested_rounds: 1,
            ci_failures,
            additions: 10,
            deletions: 2,
            changed_files: 1,
            agent: false,
            source: FactSource::Live,
        }
    }

    fn review(id: &str, state: ReviewState, source: FactSource) -> ReviewFact {
        ReviewFact {
            review_id: id.to_owned(),
            repo: repo("rust-lang/rust"),
            external: true,
            pr_number: 42,
            pr_author_login: login("alice"),
            state,
            submitted_at: Timestamp(2_000),
            source,
        }
    }

    fn moved(at: i64, stage: Stage, turn: Side) -> Transition {
        Transition {
            item_node_id: "PR_7".to_owned(),
            stage,
            turn,
            at: Timestamp(at),
        }
    }

    fn run(conn: &rusqlite::Connection, statements: &[Statement]) {
        for stmt in statements {
            exec(conn, stmt).expect("statement");
        }
    }

    #[test]
    fn enrich_batch_writes_one_statement_per_table() {
        let link = IssueLink {
            pr_node_id: "PR_7".to_owned(),
            issue_repo: repo("octo/widgets"),
            issue_number: 3,
            issue_authored_by_me: false,
            issue_commented_by_me: false,
        };
        let facts = Facts {
            prs: (0..20).map(|i| pr(&format!("PR_{i}"), 0)).collect(),
            reviews: (0..20)
                .map(|i| review(&format!("RV_{i}"), ReviewState::Approved, FactSource::Live))
                .collect(),
            transitions: (0..20)
                .map(|i| moved(i, Stage::AwaitingReview, Side::Them))
                .collect(),
            links: vec![
                link.clone(),
                IssueLink {
                    issue_number: 4,
                    ..link
                },
            ],
        };
        let statements = facts.statements();
        let sql: Vec<&str> = statements.iter().map(|s| s.sql).collect();
        assert_eq!(
            sql,
            [UPSERT_PRS, UPSERT_LINKS, INSERT_TRANSITIONS, UPSERT_REVIEWS]
        );
        let conn = memory_db();
        run(&conn, &statements);
        assert_eq!(count_rows(&conn, "pr_facts"), 20);
        assert_eq!(count_rows(&conn, "review_facts"), 20);
        assert_eq!(count_rows(&conn, "pr_transitions"), 20);
        assert_eq!(count_rows(&conn, "pr_issue_links"), 2);
        assert!(Facts::default().statements().is_empty());
    }

    #[test]
    fn a_live_review_replaces_an_imported_one() {
        let conn = memory_db();
        let imported = review("RV_1", ReviewState::Commented, FactSource::Imported);
        exec(&conn, &upsert_reviews(&[imported])).expect("imported");
        let live = review("RV_1", ReviewState::Approved, FactSource::Live);
        exec(&conn, &upsert_reviews(std::slice::from_ref(&live))).expect("live");
        exec(&conn, &upsert_reviews(&[live])).expect("live again");
        let rows = query_json(&conn, "SELECT state, source FROM review_facts", &[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["state"], "approved");
        assert_eq!(rows[0]["source"], "live");
        let late = review("RV_1", ReviewState::Dismissed, FactSource::Imported);
        exec(&conn, &upsert_reviews(&[late])).expect("imported later");
        let rows = query_json(&conn, "SELECT state, source FROM review_facts", &[]);
        assert_eq!(rows[0]["source"], "live");
    }

    #[test]
    fn transitions_only_insert() {
        let conn = memory_db();
        let first = moved(10, Stage::AwaitingReview, Side::Them);
        exec(&conn, &insert_transitions(std::slice::from_ref(&first))).expect("first");
        exec(&conn, &insert_transitions(&[first])).expect("same again");
        let turn = moved(20, Stage::AwaitingReview, Side::Me);
        exec(&conn, &insert_transitions(&[turn])).expect("turn");
        let rows = query_json(
            &conn,
            "SELECT stage, turn, at FROM pr_transitions ORDER BY at",
            &[],
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["turn"], "them");
        assert_eq!(rows[1]["turn"], "me");
        assert_eq!(rows[1]["stage"], "awaiting_review");
    }

    fn ci_failed(oid: &str, at: i64) -> EventRow {
        EventRow {
            id: EventId::for_ci("PR_7", oid, CiOutcome::Failed),
            item_node_id: "PR_7".to_owned(),
            repo: repo("octo/widgets"),
            kind: EventKind::CiFailed,
            actor_login: None,
            actor_is_me: false,
            actor_is_bot: false,
            occurred_at: Timestamp(at),
            source: EventSource::Poll,
            ingested_at: Timestamp(at),
        }
    }

    fn ci_failures(conn: &rusqlite::Connection) -> i64 {
        query_json(conn, "SELECT ci_failures FROM pr_facts", &[])[0]["ci_failures"]
            .as_i64()
            .expect("a count")
    }

    #[test]
    fn ci_failures_never_drop_after_a_rollup() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO items (node_id, repo, number, kind, title, url, author_login, \
             author_kind, authored_by_me, state, created_at, updated_at, turn, since, synced_at) \
             VALUES ('PR_7', 'octo/widgets', 7, 'pr', 't', 'u', 'karthik', 'user', 1, 'open', \
             0, 0, 'them', 0, 0)",
            [],
        )
        .expect("my pull request");
        let week = WeekStart::parse_date("2026-01-05").expect("monday");
        let start = week.timestamp().0;
        exec(
            &conn,
            &insert_many(&[ci_failed("a", start + 10), ci_failed("b", start + 20)]),
        )
        .expect("events");
        exec(&conn, &upsert_prs(&[pr("PR_7", 0)])).expect("first enrich");
        assert_eq!(ci_failures(&conn), 2);

        for stmt in crate::rollup::fold(
            week,
            crate::db::rollups::RollupTotals {
                events: 0,
                prs_opened: 0,
                prs_merged: 0,
                repos_touched: 0,
            },
        ) {
            exec(&conn, &stmt).expect("fold");
        }
        assert_eq!(count_rows(&conn, "events"), 0);
        assert_eq!(count_rows(&conn, "own_events"), 2);

        exec(&conn, &upsert_prs(&[pr("PR_7", 0)])).expect("enrich after the rollup");
        assert_eq!(ci_failures(&conn), 2);
        exec(&conn, &insert_many(&[ci_failed("c", start + 9_000_000)])).expect("new failure");
        exec(&conn, &upsert_prs(&[pr("PR_7", 1)])).expect("enrich");
        assert_eq!(ci_failures(&conn), 3);
    }

    #[test]
    fn the_earliest_first_response_is_kept() {
        let conn = memory_db();
        let answered = |at: i64, who: &str| PrFact {
            first_response_at: Some(Timestamp(at)),
            first_response_login: Some(login(who)),
            ..pr("PR_7", 0)
        };
        exec(&conn, &upsert_prs(&[answered(500, "maint")])).expect("first");
        exec(&conn, &upsert_prs(&[pr("PR_7", 0)])).expect("window moved past it");
        exec(&conn, &upsert_prs(&[answered(900, "other")])).expect("later reply");
        let rows = query_json(
            &conn,
            "SELECT first_response_at, first_response_login FROM pr_facts",
            &[],
        );
        assert_eq!(rows[0]["first_response_at"], 500);
        assert_eq!(rows[0]["first_response_login"], "maint");
    }

    #[test]
    fn a_link_learns_from_the_issue_and_my_comment_in_d1() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO items (node_id, repo, number, kind, title, url, author_login, \
             author_kind, authored_by_me, state, created_at, updated_at, turn, since, synced_at) \
             VALUES ('I_3', 'karthik/radar', 3, 'issue', 't', 'u', 'karthik', 'user', 1, 'open', \
             0, 0, 'them', 0, 0)",
            [],
        )
        .expect("my issue");
        conn.execute(
            "INSERT INTO own_events (id, item_node_id, repo, kind, actor_login, actor_is_me, \
             actor_is_bot, occurred_at, source, ingested_at) \
             VALUES ('IC_1', 'I_3', 'karthik/radar', 'commented', 'karthik', 1, 0, 5, 'poll', 5)",
            [],
        )
        .expect("my folded comment");
        let link = |number| IssueLink {
            pr_node_id: "PR_7".to_owned(),
            issue_repo: repo("karthik/radar"),
            issue_number: number,
            issue_authored_by_me: false,
            issue_commented_by_me: false,
        };
        exec(&conn, &upsert_links(&[link(3), link(4)])).expect("links");
        let rows = query_json(
            &conn,
            "SELECT issue_number, issue_authored_by_me AS a, issue_commented_by_me AS c \
             FROM pr_issue_links ORDER BY issue_number",
            &[],
        );
        assert_eq!(
            (rows[0]["a"].as_i64(), rows[0]["c"].as_i64()),
            (Some(1), Some(1))
        );
        assert_eq!(
            (rows[1]["a"].as_i64(), rows[1]["c"].as_i64()),
            (Some(0), Some(0))
        );
    }

    #[test]
    fn no_fact_table_has_a_text_column_for_content() {
        let conn = memory_db();
        for table in [
            "pr_facts",
            "review_facts",
            "pr_transitions",
            "pr_issue_links",
        ] {
            let columns = query_json(
                &conn,
                &format!("SELECT name FROM pragma_table_info('{table}')"),
                &[],
            );
            for column in columns {
                let name = column["name"].as_str().unwrap_or_default();
                assert!(
                    !["title", "body", "text"]
                        .iter()
                        .any(|word| name.split('_').any(|part| part == *word)),
                    "{table}.{name}"
                );
            }
        }
    }
}
