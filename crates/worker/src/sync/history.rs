//! The handler for `WorkKind::SeedHistory`. The 90 day backfill leaves the 52 week
//! chart empty before it, so one read-only GraphQL call per page of a year's pull
//! request contributions fills `rollups_weekly` with `seeded` rows. The years run
//! newest first, each unit names the next, and the last one marks the seed done.

use std::collections::{BTreeMap, BTreeSet};

use domain::ids::Timestamp;
use domain::rollup::WeekStart;
use serde::Deserialize;
use serde_json::json;
use worker::{console_log, Env};

use super::backfill::{valid_cursor, SearchWindow};
use super::scheduler::UnitOutcome;
use super::state::SyncState;
use super::work::WorkKind;
use crate::auth;
use crate::db::rollups::{self, RollupTotals};
use crate::github::enrich_response::RateLimit;
use crate::github::query::CONTRIBUTION_HISTORY;
use crate::github::search::PageInfo;
use crate::github::{Client, GithubError};

const SECONDS_PER_DAY: i64 = 86_400;
/// GitHub answers `contributionsCollection` for at most one year.
const YEAR_DAYS: i64 = 365;
/// Years of history to seed.
const YEARS: u8 = 2;
const PREFIX: &str = "seed";
const START: &str = "start";

/// One page of one year. The key is `seed.<from>.<older years left>.<cursor|start>`,
/// and a restart resumes at the stored cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryKey {
    from: Timestamp,
    older: u8,
    after: Option<String>,
}

fn date_of(at: Timestamp) -> String {
    at.to_string().chars().take(10).collect()
}

impl HistoryKey {
    /// The most recent year, which starts one year before `now`.
    pub fn first(now: Timestamp) -> Self {
        let from = Timestamp(now.0 - YEAR_DAYS * SECONDS_PER_DAY);
        let day = from.0.div_euclid(SECONDS_PER_DAY) * SECONDS_PER_DAY;
        Self {
            from: Timestamp(day),
            older: YEARS - 1,
            after: None,
        }
    }

    pub fn key(&self) -> String {
        format!(
            "{PREFIX}.{}.{}.{}",
            date_of(self.from),
            self.older,
            self.after.as_deref().unwrap_or(START)
        )
    }

    pub fn parse(key: &str) -> Option<Self> {
        let mut parts = key.splitn(4, '.');
        (parts.next()? == PREFIX).then_some(())?;
        let date = parts.next()?;
        let from = Timestamp::parse_rfc3339(&format!("{date}T00:00:00Z")).ok()?;
        (date_of(from) == date).then_some(())?;
        let older = parts.next()?.parse::<u8>().ok().filter(|n| *n < YEARS)?;
        let after = match parts.next()? {
            START => None,
            cursor if valid_cursor(cursor) => Some(cursor.to_owned()),
            _ => return None,
        };
        Some(Self { from, older, after })
    }

    pub fn is_seed_key(key: &str) -> bool {
        key.split('.').next() == Some(PREFIX)
    }

    fn to(&self) -> Timestamp {
        Timestamp(self.from.0 + YEAR_DAYS * SECONDS_PER_DAY)
    }

    fn page(&self, after: String) -> Self {
        Self {
            after: Some(after),
            ..self.clone()
        }
    }

    fn older_year(&self) -> Option<Self> {
        Some(Self {
            from: Timestamp(self.from.0 - YEAR_DAYS * SECONDS_PER_DAY),
            older: self.older.checked_sub(1)?,
            after: None,
        })
    }
}

#[derive(Debug, Deserialize)]
struct HistoryData {
    viewer: Viewer,
    #[serde(rename = "rateLimit")]
    rate_limit: RateLimit,
}

#[derive(Debug, Deserialize)]
struct Viewer {
    #[serde(rename = "contributionsCollection")]
    collection: Collection,
}

#[derive(Debug, Deserialize)]
struct Collection {
    #[serde(rename = "pullRequestContributions")]
    pull_requests: Contributions,
}

#[derive(Debug, Deserialize)]
struct Contributions {
    nodes: Vec<Option<Contribution>>,
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
struct Contribution {
    #[serde(rename = "occurredAt")]
    occurred_at: Timestamp,
    #[serde(rename = "pullRequest")]
    pull_request: PullRequest,
}

#[derive(Debug, Deserialize)]
struct PullRequest {
    #[serde(rename = "mergedAt")]
    merged_at: Option<Timestamp>,
    repository: Repository,
}

#[derive(Debug, Deserialize)]
struct Repository {
    #[serde(rename = "nameWithOwner")]
    name: String,
}

/// What one page adds to one week.
#[derive(Debug, Default, PartialEq, Eq)]
struct SeedWeek {
    opened: u32,
    merged: u32,
    repos: BTreeSet<String>,
}

/// The first week the 90 day backfill covers. Seeded weeks end before it, so a
/// week the backfill reads is never also seeded.
fn seed_cutoff(now: Timestamp) -> WeekStart {
    let days = SearchWindow::Backfill90d.days();
    WeekStart::containing(Timestamp(now.0 - days * SECONDS_PER_DAY))
}

fn plan_page(nodes: &[&Contribution], cutoff: WeekStart) -> BTreeMap<WeekStart, SeedWeek> {
    let mut weeks: BTreeMap<WeekStart, SeedWeek> = BTreeMap::new();
    for node in nodes {
        let repo = &node.pull_request.repository.name;
        let opened = WeekStart::containing(node.occurred_at);
        if opened < cutoff {
            let week = weeks.entry(opened).or_default();
            week.opened += 1;
            week.repos.insert(repo.clone());
        }
        if let Some(merged) = node.pull_request.merged_at.map(WeekStart::containing) {
            if merged < cutoff {
                let week = weeks.entry(merged).or_default();
                week.merged += 1;
                week.repos.insert(repo.clone());
            }
        }
    }
    weeks
}

fn statements(weeks: &BTreeMap<WeekStart, SeedWeek>) -> Vec<crate::db::Statement> {
    weeks
        .iter()
        .map(|(week, add)| {
            rollups::seed(
                *week,
                RollupTotals {
                    events: 0,
                    prs_opened: add.opened,
                    prs_merged: add.merged,
                    repos_touched: u32::try_from(add.repos.len()).unwrap_or(u32::MAX),
                },
            )
        })
        .collect()
}

fn next_cursor(key: &HistoryKey, info: &PageInfo) -> Result<Option<String>, String> {
    if !info.has_next_page {
        return Ok(None);
    }
    let next = info
        .end_cursor
        .as_deref()
        .filter(|c| valid_cursor(c))
        .ok_or("history says more pages follow but sent no usable cursor")?;
    if key.after.as_deref() == Some(next) {
        return Err(format!("history page repeats its cursor {next}"));
    }
    Ok(Some(next.to_owned()))
}

/// What follows a page: the next page of the year, the next older year, or the
/// end of the seed.
fn follow_up(key: &HistoryKey, next_after: Option<String>) -> Option<HistoryKey> {
    match next_after {
        Some(after) => Some(key.page(after)),
        None => key.older_year(),
    }
}

/// One page. The caller has awaited nothing else, so no other GitHub request for
/// the token is in flight (P5).
pub async fn run(
    key: &HistoryKey,
    env: &Env,
    state: &mut SyncState,
    now: Timestamp,
) -> UnitOutcome {
    let retry = |error: String| UnitOutcome::Retry { error };
    let Some(token) = auth::load_token(env).await else {
        return UnitOutcome::NoToken;
    };
    let db = match env.d1("DB") {
        Ok(db) => db,
        Err(err) => return retry(format!("d1 binding: {err}")),
    };
    let base = env
        .var("GITHUB_API_BASE")
        .map(|v| v.to_string())
        .unwrap_or_default();
    let client = match Client::new(&base, token) {
        Ok(client) => client,
        Err(err) => return retry(err.to_string()),
    };
    let variables = json!({
        "from": key.from.to_string(),
        "to": key.to().to_string(),
        "after": key.after,
    });
    let queried = match client
        .query::<HistoryData>(&CONTRIBUTION_HISTORY, variables)
        .await
    {
        Ok(queried) => queried,
        Err(GithubError::RateLimited { retry_after_secs }) => {
            return UnitOutcome::RateLimited { retry_after_secs }
        }
        Err(err) => return retry(err.to_string()),
    };
    state.record_rate(&queried.meta);
    state.rate.remaining = Some(queried.data.rate_limit.remaining);
    state.rate.reset = Some(queried.data.rate_limit.reset_at);

    let page = &queried.data.viewer.collection.pull_requests;
    let next_after = match next_cursor(key, &page.page_info) {
        Ok(next) => next,
        Err(error) => return retry(error),
    };
    let nodes: Vec<&Contribution> = page.nodes.iter().flatten().collect();
    let weeks = plan_page(&nodes, seed_cutoff(now));
    if !weeks.is_empty() {
        if let Err(err) = crate::db::run(&db, statements(&weeks)).await {
            return retry(format!("d1 write: {err}"));
        }
    }
    console_log!(
        "sync.history from={} older={} nodes={} weeks={} next={}",
        date_of(key.from),
        key.older,
        nodes.len(),
        weeks.len(),
        next_after.as_deref().unwrap_or("done")
    );
    let follow_ups = match follow_up(key, next_after) {
        Some(next) => vec![WorkKind::SeedHistory(next)],
        None => {
            state.history_seeded_at = Some(now);
            vec![]
        }
    };
    UnitOutcome::Done { follow_ups }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};

    const NOW: Timestamp = Timestamp(1_794_052_800);

    fn at(s: &str) -> Timestamp {
        Timestamp::parse_rfc3339(s).expect("timestamp")
    }

    fn node(opened: &str, merged: Option<&str>, repo: &str) -> Contribution {
        Contribution {
            occurred_at: at(opened),
            pull_request: PullRequest {
                merged_at: merged.map(at),
                repository: Repository { name: repo.into() },
            },
        }
    }

    fn plan(nodes: &[Contribution]) -> BTreeMap<WeekStart, SeedWeek> {
        let refs: Vec<&Contribution> = nodes.iter().collect();
        plan_page(&refs, seed_cutoff(NOW))
    }

    #[test]
    fn seed_skips_weeks_inside_the_backfill_window() {
        let cutoff = seed_cutoff(NOW);
        assert_eq!(cutoff.date(), "2026-08-03");
        let weeks = plan(&[
            node("2026-07-29T10:00:00Z", Some("2026-08-04T10:00:00Z"), "o/a"),
            node("2026-08-05T10:00:00Z", None, "o/a"),
            node("2026-09-01T10:00:00Z", Some("2026-09-02T10:00:00Z"), "o/a"),
        ]);
        let got: Vec<_> = weeks
            .iter()
            .map(|(w, s)| (w.date(), s.opened, s.merged))
            .collect();
        assert_eq!(got, [("2026-07-27".to_owned(), 1, 0)]);
    }

    #[test]
    fn a_week_counts_opened_merged_and_distinct_repos() {
        let weeks = plan(&[
            node("2025-03-03T10:00:00Z", Some("2025-03-04T10:00:00Z"), "o/a"),
            node("2025-03-05T10:00:00Z", None, "o/a"),
            node("2025-03-06T10:00:00Z", None, "o/b"),
        ]);
        let week = weeks.values().next().expect("one week");
        assert_eq!((week.opened, week.merged, week.repos.len()), (3, 1, 2));
    }

    fn seed_weeks(conn: &rusqlite::Connection, nodes: &[Contribution]) {
        for stmt in statements(&plan(nodes)) {
            exec(conn, &stmt).expect("seed");
        }
    }

    #[test]
    fn seed_never_overwrites_a_rolled_week() {
        let conn = memory_db();
        let week = WeekStart::parse_date("2025-03-03").expect("monday");
        conn.execute(
            "INSERT INTO rollups_weekly (week_start, prs_opened, prs_merged, repos_touched, \
             events, source) VALUES (?1, 8, 7, 3, 40, 'rolled')",
            [week.timestamp().0],
        )
        .expect("rolled");
        seed_weeks(&conn, &[node("2025-03-03T10:00:00Z", None, "o/a")]);
        let rows = query_json(
            &conn,
            "SELECT prs_opened, repos_touched, events, source FROM rollups_weekly",
            &[],
        );
        assert_eq!(count_rows(&conn, "rollups_weekly"), 1);
        assert_eq!(rows[0]["prs_opened"], 8);
        assert_eq!(rows[0]["repos_touched"], 3);
        assert_eq!(rows[0]["source"], "rolled");
    }

    #[test]
    fn pages_of_one_week_add_up_while_it_stays_seeded() {
        let conn = memory_db();
        seed_weeks(&conn, &[node("2025-03-03T10:00:00Z", None, "o/a")]);
        seed_weeks(
            &conn,
            &[
                node("2025-03-04T10:00:00Z", Some("2025-03-05T10:00:00Z"), "o/a"),
                node("2025-03-04T11:00:00Z", None, "o/b"),
            ],
        );
        let rows = query_json(
            &conn,
            "SELECT prs_opened, prs_merged, repos_touched, source FROM rollups_weekly",
            &[],
        );
        assert_eq!(rows[0]["prs_opened"], 3);
        assert_eq!(rows[0]["prs_merged"], 1);
        assert_eq!(rows[0]["repos_touched"], 2);
        assert_eq!(rows[0]["source"], "seeded");
    }

    #[test]
    fn two_years_make_two_units() {
        let first = HistoryKey::first(NOW);
        let older = follow_up(&first, None).expect("older year");
        assert_eq!(follow_up(&older, None), None);
        assert_eq!(older.from.0, first.from.0 - YEAR_DAYS * SECONDS_PER_DAY);
        assert_ne!(first.key(), older.key());
        let paged = follow_up(&first, Some("Y3Vyc29y".into())).expect("next page");
        assert_eq!(paged.from, first.from);
        assert_eq!(paged.after.as_deref(), Some("Y3Vyc29y"));
    }

    #[test]
    fn keys_round_trip_and_reject_everything_else() {
        let first = HistoryKey::first(NOW);
        for key in [first.clone(), first.page("Y3Vyc29yOjI=".into())] {
            assert_eq!(HistoryKey::parse(&key.key()), Some(key));
        }
        assert_eq!(first.key(), "seed.2025-11-07.1.start");
        for bad in [
            "",
            "seed",
            "seed.2025-11-07",
            "seed.2025-11-07.2.start",
            "seed.2025-13-07.1.start",
            "seed.2025-11-07.1.",
            "seed.2025-11-07.1.a.b",
            "b90.2025-11-07.1.start",
        ] {
            assert_eq!(HistoryKey::parse(bad), None, "{bad}");
        }
        assert!(HistoryKey::is_seed_key("seed.x"));
        assert!(!HistoryKey::is_seed_key("b90.2026-08-09.authored.1.start"));
    }

    #[test]
    fn a_year_reads_one_year_of_contributions() {
        let key = HistoryKey::first(NOW);
        assert_eq!(key.to().0 - key.from.0, 365 * SECONDS_PER_DAY);
    }
}
