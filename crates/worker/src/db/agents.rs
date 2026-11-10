//! The `agent_work` table behind `/hooks/*` and `GET /api/agents`.

use domain::agents::{
    AgentKey, AgentName, AgentRow, BranchName, IssueNumber, Reported, VISIBLE_FOR_SECS,
};
use domain::ids::{RepoName, Timestamp};
use serde::Deserialize;
use worker::{D1Database, Result};

use super::rows::{self, RowError};
use super::sql::{run, Param, Statement};

const UPSERT: &str = "INSERT INTO agent_work \
     (key, repo, branch, issue, agent, reported, title, first_seen_at, last_signal_at) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) \
     ON CONFLICT (key) DO UPDATE SET \
     issue = COALESCE(excluded.issue, agent_work.issue), \
     agent = COALESCE(excluded.agent, agent_work.agent), \
     reported = excluded.reported, \
     title = COALESCE(excluded.title, agent_work.title), \
     last_signal_at = excluded.last_signal_at";

const VISIBLE: &str = "SELECT key, repo, branch, issue, agent, reported, title, \
     first_seen_at, last_signal_at FROM agent_work \
     WHERE reported != 'finished' AND last_signal_at >= ?1 \
     ORDER BY last_signal_at DESC, key LIMIT 200";

/// One signal for one row. A field left `None` keeps the stored value.
#[derive(Debug, Clone)]
pub struct Signal {
    pub key: AgentKey,
    pub issue: Option<IssueNumber>,
    pub agent: Option<AgentName>,
    pub reported: Reported,
    pub title: Option<String>,
    pub at: Timestamp,
}

pub fn upsert(signal: &Signal) -> Statement {
    let (repo, branch, key_issue, key_agent) = match &signal.key {
        AgentKey::Branch { repo, branch } => (repo, Some(branch), None, None),
        AgentKey::Issue {
            repo,
            number,
            agent,
        } => (repo, None, Some(*number), Some(agent)),
    };
    let issue = signal.issue.or(key_issue);
    let agent = signal.agent.as_ref().or(key_agent);
    Statement {
        sql: UPSERT,
        params: vec![
            Param::text(signal.key.to_string()),
            Param::text(repo.to_string()),
            Param::opt_text(branch.map(ToString::to_string)),
            issue.map_or(Param::Null, |n| Param::Int(i64::from(n.get()))),
            Param::opt_text(agent.map(ToString::to_string)),
            Param::text(signal.reported.tag()),
            Param::opt_text(signal.title.clone()),
            Param::ts(signal.at),
        ],
    }
}

pub fn visible(now: Timestamp) -> Statement {
    Statement {
        sql: VISIBLE,
        params: vec![Param::ts(Timestamp(now.0 - VISIBLE_FOR_SECS))],
    }
}

pub async fn write(db: &D1Database, signal: &Signal) -> Result<()> {
    run(db, vec![upsert(signal)]).await.map(drop)
}

pub async fn read(db: &D1Database, now: Timestamp) -> Result<Vec<RawAgentRow>> {
    let results = run(db, vec![visible(now)]).await?;
    match results.first() {
        Some(result) => result.results(),
        None => Ok(Vec::new()),
    }
}

#[derive(Debug, Deserialize)]
pub struct RawAgentRow {
    key: String,
    repo: String,
    branch: Option<String>,
    issue: Option<i64>,
    agent: Option<String>,
    reported: String,
    title: Option<String>,
    first_seen_at: i64,
    last_signal_at: i64,
}

impl RawAgentRow {
    pub fn parse(self) -> std::result::Result<AgentRow, RowError> {
        let bad = |column: &'static str, value: &str| RowError::UnknownTag {
            column,
            value: value.to_owned(),
        };
        let issue = match self.issue {
            Some(n) => Some(
                u64::try_from(n)
                    .ok()
                    .and_then(|n| IssueNumber::parse(n).ok())
                    .ok_or(RowError::BadNumber {
                        column: "issue",
                        value: n,
                    })?,
            ),
            None => None,
        };
        Ok(AgentRow {
            repo: RepoName::parse(&self.repo).map_err(|source| RowError::BadId {
                column: "repo",
                source,
            })?,
            branch: match &self.branch {
                Some(b) => Some(BranchName::parse(b).map_err(|_| bad("branch", b))?),
                None => None,
            },
            issue,
            agent: match &self.agent {
                Some(a) => Some(AgentName::parse(a).map_err(|_| bad("agent", a))?),
                None => None,
            },
            reported: Reported::parse(&self.reported)
                .ok_or_else(|| bad("reported", &self.reported))?,
            title: self.title,
            first_seen_at: rows::ts(self.first_seen_at),
            last_signal_at: rows::ts(self.last_signal_at),
            key: self.key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};

    fn branch_signal(reported: Reported, title: Option<&str>, at: i64) -> Signal {
        Signal {
            key: AgentKey::Branch {
                repo: RepoName::parse("karthik/radar").expect("repo"),
                branch: BranchName::parse("agent/fix-123").expect("branch"),
            },
            issue: None,
            agent: None,
            reported,
            title: title.map(str::to_owned),
            at: Timestamp(at),
        }
    }

    fn rows(conn: &rusqlite::Connection, now: i64) -> Vec<AgentRow> {
        let stmt = visible(Timestamp(now));
        query_json(conn, stmt.sql, &stmt.params)
            .into_iter()
            .map(|row| {
                let raw: RawAgentRow =
                    serde_json::from_value(serde_json::Value::Object(row)).expect("raw row");
                raw.parse().expect("row")
            })
            .collect()
    }

    #[test]
    fn a_replayed_push_keeps_one_row_with_the_first_values() {
        let conn = memory_db();
        for at in [100, 200, 300] {
            exec(
                &conn,
                &upsert(&branch_signal(Reported::Working, Some("Fix"), at)),
            )
            .expect("upsert");
        }
        assert_eq!(count_rows(&conn, "agent_work"), 1);
        let row = rows(&conn, 300).remove(0);
        assert_eq!(
            (
                row.title.as_deref(),
                row.reported,
                row.first_seen_at,
                row.last_signal_at
            ),
            (
                Some("Fix"),
                Reported::Working,
                Timestamp(100),
                Timestamp(300)
            )
        );
    }

    #[test]
    fn a_deleted_branch_finishes_the_row_and_keeps_its_title() {
        let conn = memory_db();
        exec(
            &conn,
            &upsert(&branch_signal(Reported::Working, Some("Fix"), 100)),
        )
        .expect("push");
        exec(
            &conn,
            &upsert(&branch_signal(Reported::Finished, None, 200)),
        )
        .expect("delete");
        let stored = query_json(&conn, "SELECT reported, title FROM agent_work", &[]);
        assert_eq!(stored[0]["reported"], "finished");
        assert_eq!(stored[0]["title"], "Fix");
        assert!(rows(&conn, 200).is_empty());
    }

    #[test]
    fn an_issue_checkin_stores_its_agent_and_issue() {
        let conn = memory_db();
        let signal = Signal {
            key: AgentKey::Issue {
                repo: RepoName::parse("o/r").expect("repo"),
                number: IssueNumber::parse(7).expect("issue"),
                agent: AgentName::parse("claude-1").expect("agent"),
            },
            issue: None,
            agent: None,
            reported: Reported::Blocked,
            title: None,
            at: Timestamp(100),
        };
        exec(&conn, &upsert(&signal)).expect("upsert");
        let row = rows(&conn, 100).remove(0);
        assert_eq!(row.key, "issue:o/r#7:claude-1");
        assert_eq!(row.issue.map(IssueNumber::get), Some(7));
        assert_eq!(
            row.agent.map(|a| a.to_string()).as_deref(),
            Some("claude-1")
        );
    }

    #[test]
    fn rows_older_than_seven_days_are_not_read() {
        let conn = memory_db();
        exec(&conn, &upsert(&branch_signal(Reported::Working, None, 0))).expect("upsert");
        assert_eq!(rows(&conn, VISIBLE_FOR_SECS).len(), 1);
        assert!(rows(&conn, VISIBLE_FOR_SECS + 1).is_empty());
    }
}
