//! Agent work before a pull request exists (R23): rows written by signed
//! GitHub pushes to `agent/*` branches and by agent check-ins.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{RepoName, Timestamp};

pub const QUIET_AFTER_SECS: i64 = 2 * 60 * 60;
pub const VISIBLE_FOR_SECS: i64 = 7 * 24 * 60 * 60;
pub const MAX_AGENT_NAME: usize = 40;
pub const MAX_NOTE: usize = 140;
pub const MAX_BRANCH: usize = 200;
pub const MAX_TITLE: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentError {
    BadAgentName(String),
    BadBranch(String),
    BadIssue(u64),
    BadNote,
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadAgentName(s) => write!(f, "invalid agent name `{s}`"),
            Self::BadBranch(s) => write!(f, "invalid branch `{s}`"),
            Self::BadIssue(n) => write!(f, "invalid issue number {n}"),
            Self::BadNote => write!(f, "note is empty, too long, or has control characters"),
        }
    }
}

impl std::error::Error for AgentError {}

macro_rules! text_newtype {
    ($name:ident) => {
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = AgentError;
            fn try_from(s: String) -> Result<Self, AgentError> {
                Self::parse(&s)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

/// A name an agent gives itself, 1 to 40 of `A-Z a-z 0-9 . _ -`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AgentName(String);

impl AgentName {
    pub fn parse(s: &str) -> Result<Self, AgentError> {
        let ok = !s.is_empty()
            && s.chars().count() <= MAX_AGENT_NAME
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        ok.then(|| Self(s.to_owned()))
            .ok_or_else(|| AgentError::BadAgentName(s.to_owned()))
    }
}

text_newtype!(AgentName);

/// A branch name, held to characters that are safe in a GitHub URL path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BranchName(String);

impl BranchName {
    pub fn parse(s: &str) -> Result<Self, AgentError> {
        let ok = !s.is_empty()
            && s.len() <= MAX_BRANCH
            && !s.starts_with(['/', '-', '.'])
            && !s.ends_with(['/', '.'])
            && !s.contains("..")
            && !s.contains("//")
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
        ok.then(|| Self(s.to_owned()))
            .ok_or_else(|| AgentError::BadBranch(s.to_owned()))
    }

    /// The branch a push names, when its ref is under `refs/heads/agent/`.
    pub fn from_agent_ref(git_ref: &str) -> Option<Self> {
        let branch = git_ref.strip_prefix("refs/heads/")?;
        branch.starts_with("agent/").then_some(())?;
        Self::parse(branch).ok()
    }
}

text_newtype!(BranchName);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u32")]
pub struct IssueNumber(u32);

impl IssueNumber {
    pub fn parse(n: u64) -> Result<Self, AgentError> {
        u32::try_from(n)
            .ok()
            .filter(|n| *n > 0)
            .map(Self)
            .ok_or(AgentError::BadIssue(n))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u64> for IssueNumber {
    type Error = AgentError;
    fn try_from(n: u64) -> Result<Self, AgentError> {
        Self::parse(n)
    }
}

impl From<IssueNumber> for u32 {
    fn from(n: IssueNumber) -> Self {
        n.0
    }
}

/// A check-in's free text, 1 to 140 characters with no control characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ShortNote(String);

impl ShortNote {
    pub fn parse(s: &str) -> Result<Self, AgentError> {
        let s = s.trim();
        let ok = !s.is_empty() && s.chars().count() <= MAX_NOTE && !s.chars().any(char::is_control);
        ok.then(|| Self(s.to_owned())).ok_or(AgentError::BadNote)
    }
}

text_newtype!(ShortNote);

/// What the agent or GitHub last said about the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reported {
    Working,
    Blocked,
    Finished,
}

impl Reported {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Finished => "finished",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Working, Self::Blocked, Self::Finished]
            .into_iter()
            .find(|r| r.tag() == s)
    }
}

/// The state the lane shows, derived from `Reported` and the signal's age.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    Quiet,
    Blocked,
    Finished,
}

impl AgentState {
    pub fn word(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Quiet => "quiet",
            Self::Blocked => "blocked",
            Self::Finished => "finished",
        }
    }
}

/// One row's identity. A push and a check-in that name the same branch land
/// on the same row.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentKey {
    Branch {
        repo: RepoName,
        branch: BranchName,
    },
    Issue {
        repo: RepoName,
        number: IssueNumber,
        agent: AgentName,
    },
}

impl fmt::Display for AgentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Branch { repo, branch } => write!(f, "branch:{repo}:{branch}"),
            Self::Issue {
                repo,
                number,
                agent,
            } => write!(f, "issue:{repo}#{}:{agent}", number.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRow {
    pub key: String,
    pub repo: RepoName,
    pub branch: Option<BranchName>,
    pub issue: Option<IssueNumber>,
    pub agent: Option<AgentName>,
    pub reported: Reported,
    pub title: Option<String>,
    pub first_seen_at: Timestamp,
    pub last_signal_at: Timestamp,
}

pub fn agent_state(reported: Reported, last_signal_at: Timestamp, now: Timestamp) -> AgentState {
    match reported {
        Reported::Finished => AgentState::Finished,
        Reported::Blocked => AgentState::Blocked,
        Reported::Working if now.0 - last_signal_at.0 > QUIET_AFTER_SECS => AgentState::Quiet,
        Reported::Working => AgentState::Working,
    }
}

/// A row stays in the lane until it finishes or goes 7 days without a signal.
pub fn is_visible(row: &AgentRow, now: Timestamp) -> bool {
    row.reported != Reported::Finished && now.0 - row.last_signal_at.0 <= VISIBLE_FOR_SECS
}

/// Two unfinished rows that hold the same issue in the same repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlap {
    pub first: String,
    pub second: String,
}

pub fn flag_overlaps(rows: &[AgentRow]) -> Vec<Overlap> {
    let open: Vec<&AgentRow> = rows
        .iter()
        .filter(|row| row.reported != Reported::Finished && row.issue.is_some())
        .collect();
    let mut overlaps = Vec::new();
    for (i, a) in open.iter().enumerate() {
        for b in &open[i + 1..] {
            if a.repo == b.repo && a.issue == b.issue {
                overlaps.push(Overlap {
                    first: a.key.clone(),
                    second: b.key.clone(),
                });
            }
        }
    }
    overlaps
}

/// The body of `POST /hooks/checkin`, parsed once at the boundary.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCheckin {
    pub agent: AgentName,
    pub repo: RepoName,
    #[serde(default)]
    pub branch: Option<BranchName>,
    #[serde(default)]
    pub issue: Option<IssueNumber>,
    pub state: Reported,
    #[serde(default)]
    pub note: Option<ShortNote>,
}

impl AgentCheckin {
    /// `None` when the check-in names neither a branch nor an issue.
    pub fn key(&self) -> Option<AgentKey> {
        match (&self.branch, self.issue) {
            (Some(branch), _) => Some(AgentKey::Branch {
                repo: self.repo.clone(),
                branch: branch.clone(),
            }),
            (None, Some(number)) => Some(AgentKey::Issue {
                repo: self.repo.clone(),
                number,
                agent: self.agent.clone(),
            }),
            (None, None) => None,
        }
    }
}

/// The first line of a commit message, cut to 120 characters.
pub fn commit_title(message: &str) -> Option<String> {
    let line = message.lines().next()?.trim();
    (!line.is_empty()).then(|| line.chars().take(MAX_TITLE).collect())
}

/// One row as `GET /api/agents` returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentView {
    pub key: String,
    pub repo: RepoName,
    pub branch: Option<BranchName>,
    pub issue: Option<IssueNumber>,
    pub agent: Option<AgentName>,
    pub state: AgentState,
    pub title: Option<String>,
    pub last_signal_at: Timestamp,
    pub also_taken_by: Vec<String>,
}

impl AgentView {
    /// The agent's name, else its branch, else the issue it holds.
    pub fn who(&self) -> String {
        match (&self.agent, &self.branch, self.issue) {
            (Some(agent), ..) => agent.to_string(),
            (None, Some(branch), _) => branch.to_string(),
            (None, None, Some(n)) => format!("#{}", n.0),
            (None, None, None) => self.key.clone(),
        }
    }

    pub fn issue_url(&self) -> Option<String> {
        self.issue
            .map(|n| format!("https://github.com/{}/issues/{}", self.repo, n.0))
    }

    /// The branch on GitHub when there is one, else the issue.
    pub fn url(&self) -> Option<String> {
        match &self.branch {
            Some(branch) => Some(format!("https://github.com/{}/tree/{branch}", self.repo)),
            None => self.issue_url(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentsPage {
    pub agents: Vec<AgentView>,
}

/// The visible rows, newest signal first, each with the rows it overlaps.
pub fn agents_page(mut rows: Vec<AgentRow>, now: Timestamp) -> AgentsPage {
    rows.retain(|row| is_visible(row, now));
    rows.sort_by(|a, b| {
        b.last_signal_at
            .cmp(&a.last_signal_at)
            .then_with(|| a.key.cmp(&b.key))
    });
    let overlaps = flag_overlaps(&rows);
    let who = |key: &str| {
        rows.iter()
            .find(|row| row.key == key)
            .map(|row| match (&row.agent, &row.branch) {
                (Some(agent), _) => agent.to_string(),
                (None, Some(branch)) => branch.to_string(),
                (None, None) => row.key.clone(),
            })
            .unwrap_or_default()
    };
    let agents = rows
        .iter()
        .map(|row| {
            let also_taken_by = overlaps
                .iter()
                .filter_map(|o| {
                    if o.first == row.key {
                        Some(who(&o.second))
                    } else if o.second == row.key {
                        Some(who(&o.first))
                    } else {
                        None
                    }
                })
                .collect();
            AgentView {
                key: row.key.clone(),
                repo: row.repo.clone(),
                branch: row.branch.clone(),
                issue: row.issue,
                agent: row.agent.clone(),
                state: agent_state(row.reported, row.last_signal_at, now),
                title: row.title.clone(),
                last_signal_at: row.last_signal_at,
                also_taken_by,
            }
        })
        .collect();
    AgentsPage { agents }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Timestamp = Timestamp(1_800_000_000);
    const MINUTE: i64 = 60;
    const DAY: i64 = 86_400;

    fn ago(secs: i64) -> Timestamp {
        Timestamp(NOW.0 - secs)
    }

    fn repo(s: &str) -> RepoName {
        RepoName::parse(s).expect("repo")
    }

    fn row(key: &str, repo_name: &str, issue: Option<u32>, reported: Reported) -> AgentRow {
        AgentRow {
            key: key.to_owned(),
            repo: repo(repo_name),
            branch: None,
            issue: issue.map(|n| IssueNumber::parse(u64::from(n)).expect("issue")),
            agent: Some(AgentName::parse(key).expect("agent")),
            reported,
            title: None,
            first_seen_at: ago(DAY),
            last_signal_at: ago(MINUTE),
        }
    }

    #[test]
    fn agents_working_turns_quiet_after_two_hours() {
        assert_eq!(
            agent_state(Reported::Working, ago(119 * MINUTE), NOW),
            AgentState::Working
        );
        assert_eq!(
            agent_state(Reported::Working, ago(121 * MINUTE), NOW),
            AgentState::Quiet
        );
        assert_eq!(
            agent_state(Reported::Blocked, ago(3 * DAY), NOW),
            AgentState::Blocked
        );
        assert_eq!(
            agent_state(Reported::Finished, ago(3 * DAY), NOW),
            AgentState::Finished
        );
    }

    #[test]
    fn agents_visibility_drops_finished_rows_and_rows_older_than_seven_days() {
        let mut fresh = row("a", "o/r", None, Reported::Blocked);
        fresh.last_signal_at = ago(6 * DAY);
        assert!(is_visible(&fresh, NOW));
        let mut stale = fresh.clone();
        stale.last_signal_at = ago(8 * DAY);
        assert!(!is_visible(&stale, NOW));
        assert!(!is_visible(&row("b", "o/r", None, Reported::Finished), NOW));
    }

    #[test]
    fn agents_overlap_marks_two_unfinished_rows_on_one_issue() {
        let rows = [
            row("a", "o/r", Some(7), Reported::Working),
            row("b", "o/r", Some(7), Reported::Blocked),
            row("c", "o/r", Some(7), Reported::Finished),
            row("d", "o/other", Some(7), Reported::Working),
            row("e", "o/r", None, Reported::Working),
        ];
        assert_eq!(
            flag_overlaps(&rows),
            vec![Overlap {
                first: "a".into(),
                second: "b".into()
            }]
        );
    }

    #[test]
    fn agents_page_names_the_other_holder() {
        let page = agents_page(
            vec![
                row("alpha", "o/r", Some(7), Reported::Working),
                row("beta", "o/r", Some(7), Reported::Working),
                row("gone", "o/r", Some(7), Reported::Finished),
            ],
            NOW,
        );
        let taken: Vec<_> = page
            .agents
            .iter()
            .map(|a| (a.key.as_str(), a.also_taken_by.clone()))
            .collect();
        assert_eq!(
            taken,
            [
                ("alpha", vec!["beta".to_owned()]),
                ("beta", vec!["alpha".to_owned()])
            ]
        );
    }

    fn checkin(json: &str) -> Result<AgentCheckin, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn agents_checkin_parses_and_keys_by_branch_then_issue() {
        let by_issue = checkin(r#"{"agent":"claude-1","repo":"o/r","issue":7,"state":"working"}"#)
            .expect("checkin");
        assert_eq!(
            by_issue.key().map(|k| k.to_string()),
            Some("issue:o/r#7:claude-1".to_owned())
        );
        let by_branch = checkin(
            r#"{"agent":"a","repo":"o/r","branch":"agent/fix-1","issue":7,"state":"blocked","note":"waiting on CI"}"#,
        )
        .expect("checkin");
        assert_eq!(
            by_branch.key().map(|k| k.to_string()),
            Some("branch:o/r:agent/fix-1".to_owned())
        );
        let neither = checkin(r#"{"agent":"a","repo":"o/r","state":"working"}"#).expect("checkin");
        assert_eq!(neither.key(), None);
    }

    #[test]
    fn agents_checkin_rejects_bad_bodies() {
        let name41 = "a".repeat(41);
        let name40 = "a".repeat(40);
        let note141 = "n".repeat(141);
        let note140 = "n".repeat(140);
        let base = |agent: &str, repo_name: &str, extra: &str| {
            format!(
                r#"{{"agent":"{agent}","repo":"{repo_name}","issue":1,"state":"working"{extra}}}"#
            )
        };
        for bad in [
            base("a", "o/r", r#","extra":1"#),
            base(&name41, "o/r", ""),
            base("a", "o/r", &format!(r#","note":"{note141}""#)),
            base("a", "a/b/c", ""),
            base("a", "o/r", r#","state":"done""#),
            base("a", "o/r", r#","branch":"../x""#),
            r#"{"agent":"a","repo":"o/r","issue":0,"state":"working"}"#.to_owned(),
        ] {
            assert!(checkin(&bad).is_err(), "{bad}");
        }
        assert!(checkin(&base(&name40, "o/r", &format!(r#","note":"{note140}""#))).is_ok());
    }

    #[test]
    fn agents_branch_comes_only_from_an_agent_ref() {
        assert_eq!(
            BranchName::from_agent_ref("refs/heads/agent/fix-123").map(|b| b.to_string()),
            Some("agent/fix-123".to_owned())
        );
        for other in [
            "refs/heads/main",
            "refs/tags/agent/x",
            "refs/heads/agentx/y",
            "refs/heads/agent/a b",
        ] {
            assert_eq!(BranchName::from_agent_ref(other), None, "{other}");
        }
    }

    #[test]
    fn agents_commit_title_is_the_first_line_cut_to_120() {
        assert_eq!(commit_title("Fix it\n\nbody"), Some("Fix it".to_owned()));
        assert_eq!(commit_title(&"x".repeat(200)).map(|t| t.len()), Some(120));
        assert_eq!(commit_title(""), None);
    }

    #[test]
    fn agents_view_links_the_branch_before_the_issue() {
        let mut view = agents_page(vec![row("a", "o/r", Some(7), Reported::Working)], NOW)
            .agents
            .remove(0);
        assert_eq!(
            view.url().as_deref(),
            Some("https://github.com/o/r/issues/7")
        );
        view.branch = Some(BranchName::parse("agent/x").expect("branch"));
        assert_eq!(
            view.url().as_deref(),
            Some("https://github.com/o/r/tree/agent/x")
        );
    }
}
