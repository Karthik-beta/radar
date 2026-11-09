//! The handler for `WorkKind::BackfillPage`. One unit fetches one page of search
//! results and stores each node the way an Enrich unit stores a notification. The
//! sign-in backfill covers 90 days and the nightly search covers 7. Each is three
//! searches in a row, and each page names the next one, so a restart resumes at the
//! stored cursor.

use domain::event::Event;
use domain::ids::{Login, RepoName, Timestamp};
use serde_json::json;
use worker::{console_log, Env};

use super::config::ENRICH_BATCH;
use super::enrich::{classify_node, writes};
use super::scheduler::UnitOutcome;
use super::state::SyncState;
use super::work::WorkKind;
use crate::auth;
use crate::db::batch::{self, MeRecord, Stored, StoredRecord, ThreadReason};
use crate::db::events::EventSource;
use crate::db::items::ItemRow;
use crate::github::enrich_response::ItemNode;
use crate::github::search::{PageInfo, SearchData, SEARCH_QUERY};
use crate::github::{Client, GithubError};

const SECONDS_PER_DAY: i64 = 86_400;
const START: &str = "start";
const MAX_CURSOR_LEN: usize = 128;

/// How far back a search reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchWindow {
    Backfill90d,
    Nightly7d,
}

impl SearchWindow {
    pub fn name(self) -> &'static str {
        match self {
            Self::Backfill90d => "Backfill90d",
            Self::Nightly7d => "Nightly7d",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Backfill90d => "b90",
            Self::Nightly7d => "n7",
        }
    }

    fn days(self) -> i64 {
        match self {
            Self::Backfill90d => 90,
            Self::Nightly7d => 7,
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        [Self::Backfill90d, Self::Nightly7d]
            .into_iter()
            .find(|w| w.tag() == tag)
    }

    /// The `updated:>=` date, as `YYYY-MM-DD`.
    pub fn since(self, now: Timestamp) -> String {
        let from = Timestamp(now.0.saturating_sub(self.days() * SECONDS_PER_DAY));
        from.to_string().chars().take(10).collect()
    }
}

/// Which of the three searches a page belongs to, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Authored,
    AuthoredIssues,
    Involves,
}

impl SearchKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Authored => "Authored",
            Self::AuthoredIssues => "AuthoredIssues",
            Self::Involves => "Involves",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Authored => "authored",
            Self::AuthoredIssues => "issues",
            Self::Involves => "involves",
        }
    }

    fn query(self) -> &'static str {
        match self {
            Self::Authored => "is:pr author:@me",
            Self::AuthoredIssues => "is:issue author:@me",
            Self::Involves => "involves:@me",
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        [Self::Authored, Self::AuthoredIssues, Self::Involves]
            .into_iter()
            .find(|k| k.tag() == tag)
    }

    fn next(self) -> Option<Self> {
        match self {
            Self::Authored => Some(Self::AuthoredIssues),
            Self::AuthoredIssues => Some(Self::Involves),
            Self::Involves => None,
        }
    }
}

/// Where a search page starts. It is the whole of a `BackfillPage` row's key, so a
/// restart resumes from it. `page` counts from 1 within a kind and only labels the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackfillCursor {
    pub window: SearchWindow,
    pub kind: SearchKind,
    pub page: u32,
    pub after: Option<String>,
}

impl BackfillCursor {
    pub fn first(window: SearchWindow) -> Self {
        Self {
            window,
            kind: SearchKind::Authored,
            page: 1,
            after: None,
        }
    }

    /// The search string: the kind's terms and the window's date, and nothing else.
    pub fn query(&self, now: Timestamp) -> String {
        format!("{} updated:>={}", self.kind.query(), self.window.since(now))
    }

    pub fn key(&self) -> String {
        format!(
            "{}.{}.{}.{}",
            self.window.tag(),
            self.kind.tag(),
            self.page,
            self.after.as_deref().unwrap_or(START)
        )
    }

    pub fn parse(key: &str) -> Option<Self> {
        let mut parts = key.splitn(4, '.');
        let window = SearchWindow::from_tag(parts.next()?)?;
        let kind = SearchKind::from_tag(parts.next()?)?;
        let page = parts.next()?.parse::<u32>().ok().filter(|&p| p >= 1)?;
        let after = match parts.next()? {
            START => None,
            cursor if valid_cursor(cursor) => Some(cursor.to_owned()),
            _ => return None,
        };
        Some(Self {
            window,
            kind,
            page,
            after,
        })
    }
}

/// A cursor ends up in a row key and the log, so it keeps to the alphabet of a base64 string.
fn valid_cursor(raw: &str) -> bool {
    !raw.is_empty()
        && raw != START
        && raw.len() <= MAX_CURSOR_LEN
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'='))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Page(BackfillCursor),
    Done,
}

/// What follows a page. `next_after` is the cursor of the next page of the same
/// search, or `None` when it was the last one.
pub fn next_step(cursor: &BackfillCursor, next_after: Option<String>) -> Step {
    if let Some(after) = next_after {
        return Step::Page(BackfillCursor {
            page: cursor.page.saturating_add(1),
            after: Some(after),
            ..cursor.clone()
        });
    }
    match cursor.kind.next() {
        Some(kind) => Step::Page(BackfillCursor {
            window: cursor.window,
            kind,
            page: 1,
            after: None,
        }),
        None => Step::Done,
    }
}

fn next_cursor(cursor: &BackfillCursor, info: &PageInfo) -> Result<Option<String>, String> {
    if !info.has_next_page {
        return Ok(None);
    }
    let next = info
        .end_cursor
        .as_deref()
        .filter(|c| valid_cursor(c))
        .ok_or("search says more pages follow but sent no usable cursor")?;
    if cursor.after.as_deref() == Some(next) {
        return Err(format!("search page repeats its cursor {next}"));
    }
    Ok(Some(next.to_owned()))
}

/// `https://github.com/<owner>/<name>/pull/<n>` or `.../issues/<n>`, which is how a
/// search node names its repository.
fn repo_of(url: &str) -> Option<RepoName> {
    let mut parts = url.strip_prefix("https://github.com/")?.split('/');
    let (owner, name, kind) = (parts.next()?, parts.next()?, parts.next()?);
    if !matches!(kind, "pull" | "issues") {
        return None;
    }
    RepoName::parse(&format!("{owner}/{name}")).ok()
}

#[derive(Debug, Default, PartialEq)]
pub struct Page {
    pub items: Vec<ItemRow>,
    pub events: Vec<Event>,
    /// Nodes whose url did not name a repository.
    pub skipped: Vec<String>,
}

/// Classifies every node. `stored` is what `items` already holds for them.
pub fn plan_page(
    nodes: &[&ItemNode],
    stored: &[Stored],
    reasons: &[ThreadReason],
    me: &Login,
    now: Timestamp,
) -> Page {
    let mut page = Page::default();
    for node in nodes {
        let Some(repo) = repo_of(node.url()) else {
            page.skipped.push(node.id().to_owned());
            continue;
        };
        let known = stored.iter().find(|s| s.node_id == node.id());
        let reason = reasons
            .iter()
            .find(|r| r.item_node_id == node.id())
            .map_or("", |r| r.reason.as_str());
        let enriched = classify_node(node, &repo, reason, known, me, now);
        page.items.push(enriched.row);
        page.events.extend(enriched.events);
    }
    page
}

type Loaded = (Vec<Stored>, Login, Vec<ThreadReason>);

fn load(results: &[worker::D1Result]) -> Result<Loaded, String> {
    let read = |index: usize| {
        results
            .get(index)
            .ok_or_else(|| "d1 returned too few results".to_owned())
    };
    let stored = read(0)?
        .results::<StoredRecord>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(Stored::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let me = read(1)?
        .results::<MeRecord>()
        .map_err(|e| e.to_string())?
        .first()
        .ok_or_else(|| "no signed-in account".to_owned())?
        .login()
        .map_err(|e| e.to_string())?;
    let reasons = read(2)?
        .results::<ThreadReason>()
        .map_err(|e| e.to_string())?;
    Ok((stored, me, reasons))
}

/// One search page. The caller has awaited nothing else, so no other GitHub
/// request for the token is in flight (P5).
pub async fn run(
    cursor: &BackfillCursor,
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
        "q": cursor.query(now),
        "first": ENRICH_BATCH,
        "after": cursor.after,
    });
    let queried = match client.query::<SearchData>(&SEARCH_QUERY, variables).await {
        Ok(queried) => queried,
        Err(GithubError::RateLimited { retry_after_secs }) => {
            return UnitOutcome::RateLimited { retry_after_secs }
        }
        Err(err) => return retry(err.to_string()),
    };
    state.record_rate(&queried.meta);
    let limit = &queried.data.rate_limit;
    state.rate.remaining = Some(limit.remaining);
    state.rate.reset = Some(limit.reset_at);

    let page = &queried.data.search;
    let next_after = match next_cursor(cursor, &page.page_info) {
        Ok(next) => next,
        Err(error) => return retry(error),
    };
    let nodes: Vec<&ItemNode> = page.nodes.iter().flatten().collect();
    if !nodes.is_empty() {
        let ids: Vec<&str> = nodes.iter().map(|n| n.id()).collect();
        let loaded = match crate::db::run(
            &db,
            vec![
                batch::stored_by_node(&ids),
                batch::me(),
                batch::thread_reasons(&ids),
            ],
        )
        .await
        {
            Ok(results) => load(&results),
            Err(err) => Err(format!("d1 read: {err}")),
        };
        let (stored, me, reasons) = match loaded {
            Ok(loaded) => loaded,
            Err(error) => return retry(error),
        };
        let planned = plan_page(&nodes, &stored, &reasons, &me, now);
        for id in &planned.skipped {
            console_log!("sync.backfill skipped node={id}");
        }
        let statements = writes(&planned.items, &planned.events, EventSource::Backfill, now);
        if let Err(err) = crate::db::run(&db, statements).await {
            return retry(format!("d1 write: {err}"));
        }
    }
    console_log!(
        "sync.backfill window={} kind={} page={} items={} next={}",
        cursor.window.name(),
        cursor.kind.name(),
        cursor.page,
        nodes.len(),
        next_after.as_deref().unwrap_or("done")
    );
    let follow_ups = match next_step(cursor, next_after) {
        Step::Page(next) => vec![WorkKind::BackfillPage(next)],
        Step::Done => {
            if cursor.window == SearchWindow::Backfill90d {
                state.backfill_done_at = Some(now);
            }
            vec![]
        }
    };
    UnitOutcome::Done { follow_ups }
}

#[cfg(test)]
#[path = "backfill_tests.rs"]
mod tests;
