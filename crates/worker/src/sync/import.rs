//! The handlers for `WorkKind::ImportFacts` and `WorkKind::RefreshRepos` (ADR 0010).
//! The import reads my pull requests and reviews back to my account's creation,
//! one GraphQL request per unit, and writes them with `source = 'imported'`, so a
//! live row always wins. The repo refresh rereads stars and language once a week.
//!
//! Every import response parses into one `Node`, which with `Page` holds the
//! union of the fields the five queries select, as `RawItem` does for Enrich.
//! Two serde visitors and one `Client::query` type keep the Worker bundle
//! small. The `plan_*` functions check the fields each query needs.

use domain::event::{Event, EventActor, EventId, EventKind};
use domain::facts::{is_agent_branch, is_external, FactSource, IssueLink, PrFact, ReviewFact};
use domain::ids::{Login, RepoName, Timestamp};
use domain::repo_stats::first_responder;
use serde::Deserialize;
use serde_json::{json, Value};
use worker::{console_log, D1Database, Env};

use super::backfill::valid_cursor;
use super::config::IMPORT_PAGE;
use super::enrich::review_state;
use super::scheduler::UnitOutcome;
use super::state::SyncState;
use super::work::{Day, ImportKey, NodeId, PrSet, RepoPage, WorkKind, WorkRow};
use crate::auth;
use crate::db::batch::{self, MeRecord};
use crate::db::facts::{self, Facts};
use crate::db::repos::{self, RepoIdRecord, RepoMeta};
use crate::db::Statement;
use crate::github::enrich_response::{
    ActorNode, ClosingIssueNode, Nodes, RateLimit, ReviewStateNode,
};
use crate::github::query::{
    IMPORT_PRS, IMPORT_REVIEWED_PRS, IMPORT_REVIEWS, IMPORT_TIMELINE, REPO_META, VIEWER,
};
use crate::github::search::PageInfo;
use crate::github::{Client, GithubError, QueryDoc};

/// GitHub's search returns at most this many results for one query.
const SEARCH_LIMIT: u32 = 1_000;
const LANGUAGE_MAX_LEN: usize = 64;
const LAST_SECOND_OF_DAY: i64 = 86_399;

/// The response data, a pull request, a repository, its language, a timeline
/// item, a review, or a review contribution, with the fields its query selects.
#[derive(Debug, Default, Deserialize)]
struct Node {
    search: Option<Page>,
    node: Option<Box<Node>>,
    #[serde(default)]
    nodes: Vec<Option<Node>>,
    viewer: Option<Box<Node>>,
    #[serde(rename = "rateLimit")]
    rate_limit: Option<RateLimit>,
    #[serde(rename = "contributionsCollection")]
    collection: Option<Box<Node>>,
    #[serde(rename = "pullRequestReviewContributions")]
    review_contributions: Option<Page>,
    id: Option<String>,
    number: Option<u32>,
    #[serde(rename = "createdAt")]
    created_at: Option<Timestamp>,
    #[serde(rename = "mergedAt")]
    merged_at: Option<Timestamp>,
    #[serde(rename = "closedAt")]
    closed_at: Option<Timestamp>,
    #[serde(rename = "submittedAt")]
    submitted_at: Option<Timestamp>,
    additions: Option<u32>,
    deletions: Option<u32>,
    #[serde(rename = "changedFiles")]
    changed_files: Option<u32>,
    #[serde(rename = "headRefName")]
    head_ref: Option<String>,
    state: Option<ReviewStateNode>,
    #[serde(rename = "nameWithOwner")]
    name: Option<String>,
    #[serde(rename = "stargazerCount")]
    stars: Option<u32>,
    #[serde(rename = "primaryLanguage")]
    language: Option<Box<Node>>,
    /// The `name` of a language.
    #[serde(rename = "name")]
    language_name: Option<String>,
    author: Option<ActorNode>,
    repository: Option<Box<Node>>,
    #[serde(rename = "pullRequest")]
    pull_request: Option<Box<Node>>,
    #[serde(rename = "closingIssuesReferences")]
    closing: Option<Nodes<ClosingIssueNode>>,
    reviews: Option<Page>,
    #[serde(rename = "timelineItems")]
    timeline: Option<Page>,
}

/// A connection: search results, timeline items, reviews, or contributions.
#[derive(Debug, Default, Deserialize)]
struct Page {
    #[serde(rename = "issueCount")]
    issue_count: Option<u32>,
    #[serde(rename = "totalCount")]
    total_count: Option<u32>,
    #[serde(default)]
    nodes: Vec<Option<Node>>,
    #[serde(rename = "pageInfo")]
    page_info: Option<PageInfo>,
}

fn need<T>(value: Option<T>, field: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("import response has no {field}"))
}

/// The cursor of the page after this one, or `None` after the last page.
fn next_cursor(after: Option<&str>, info: Option<&PageInfo>) -> Result<Option<String>, String> {
    let info = need(info, "pageInfo")?;
    if !info.has_next_page {
        return Ok(None);
    }
    let next = info
        .end_cursor
        .as_deref()
        .filter(|c| valid_cursor(c))
        .ok_or("a page says more follow but sent no usable cursor")?;
    if after == Some(next) {
        return Err(format!("a page repeats its cursor {next}"));
    }
    Ok(Some(next.to_owned()))
}

fn search_range(from: Day, to: Day, after: Option<String>) -> WorkKind {
    WorkKind::ImportFacts(ImportKey::SearchRange { from, to, after })
}

/// One search range and one reviewed list per year, from the year my account
/// was created to this year.
pub fn year_units(created: Timestamp, now: Timestamp) -> Vec<WorkKind> {
    let last = Day::of(now).year();
    let first = Day::of(created).year().min(last);
    (first..=last)
        .filter_map(|year| Some((year, Day::year_start(year)?, Day::year_end(year)?)))
        .flat_map(|(year, from, to)| {
            [
                search_range(from, to, None),
                WorkKind::ImportFacts(ImportKey::ReviewedPrs { year, after: None }),
            ]
        })
        .collect()
}

/// The two halves of a range by date, or `None` for a single day.
fn halves(from: Day, to: Day) -> Option<[(Day, Day); 2]> {
    let days = from.days_through(to);
    (days >= 2).then(|| {
        let end = from.plus_days(days / 2 - 1);
        [(from, end), (end.plus_days(1), to)]
    })
}

/// Whether any `import.` row other than `running` is queued, dead ones included.
pub fn rows_left(rows: &[WorkRow], running: &WorkKind) -> bool {
    rows.iter()
        .any(|r| r.kind.is_import() && r.kind != *running)
}

/// The import is done when this unit queues no import unit and none is left.
pub fn finishes(follow_ups: &[WorkKind], rows_left: bool) -> bool {
    !rows_left && !follow_ups.iter().any(WorkKind::is_import)
}

/// The first reply by a human who is not me, as R19's `first_responder` picks it.
fn first_human(
    pr: &str,
    repo: &RepoName,
    created: Timestamp,
    entries: &[Option<Node>],
    me: &Login,
) -> Option<(Timestamp, Login)> {
    let events: Vec<Event> = entries
        .iter()
        .flatten()
        .filter_map(|entry| {
            let author = entry.author.as_ref();
            let login = author
                .and_then(|a| Login::parse(&a.login).ok())
                .unwrap_or_else(Login::ghost);
            Some(Event {
                id: EventId::for_node(pr),
                item: pr.to_owned(),
                repo: repo.clone(),
                kind: EventKind::Commented,
                actor: Some(EventActor {
                    is_me: login == *me,
                    is_bot: author.is_some_and(|a| a.typename == "Bot"),
                    login,
                }),
                occurred_at: entry.submitted_at.or(entry.created_at)?,
            })
        })
        .collect();
    let first = first_responder(created, &events)?;
    Some((first.occurred_at, first.actor.as_ref()?.login.clone()))
}

#[derive(Debug, PartialEq, Eq)]
enum TimelineStep {
    Answered(Timestamp, Login),
    Next(String),
    End,
}

/// What one page of a timeline settles: the first human reply, the next page to
/// read, or the end of the timeline with no reply.
fn timeline_step(
    pr: &str,
    repo: &RepoName,
    created: Timestamp,
    page: &Page,
    after: Option<&str>,
    me: &Login,
) -> Result<TimelineStep, String> {
    if let Some((at, login)) = first_human(pr, repo, created, &page.nodes, me) {
        return Ok(TimelineStep::Answered(at, login));
    }
    Ok(match next_cursor(after, page.page_info.as_ref())? {
        Some(next) => TimelineStep::Next(next),
        None => TimelineStep::End,
    })
}

/// What the import or the refresh stores for a repo. `full_name` is the name
/// Radar keys it by, and ownership follows the name GitHub answers with.
fn repo_meta(
    full_name: RepoName,
    node: &Node,
    me: &Login,
    now: Timestamp,
) -> Result<RepoMeta, String> {
    let current = node
        .name
        .as_deref()
        .and_then(|n| RepoName::parse(n).ok())
        .unwrap_or_else(|| full_name.clone());
    Ok(RepoMeta {
        owned_by_me: !is_external(&current, me),
        full_name,
        node_id: need(node.id.clone(), "repository.id")?,
        stars: need(node.stars, "repository.stargazerCount")?,
        primary_language: node
            .language
            .as_ref()
            .and_then(|l| l.language_name.clone())
            .filter(|name| !name.is_empty() && name.len() <= LANGUAGE_MAX_LEN),
        refreshed_at: now,
    })
}

/// What a unit writes and queues.
#[derive(Debug, Default)]
struct Done {
    statements: Vec<Statement>,
    follow_ups: Vec<WorkKind>,
}

#[derive(Debug, Default)]
struct RangePlan {
    facts: Facts,
    repos: Vec<RepoMeta>,
    follow_ups: Vec<WorkKind>,
}

/// One search node, with the timeline unit to queue when its first timeline
/// page holds no human reply.
struct ImportedPr {
    fact: PrFact,
    links: Vec<IssueLink>,
    repo: RepoMeta,
    timeline: Option<WorkKind>,
}

/// A node whose ids do not parse is `None`.
fn imported_pr(node: &Node, me: &Login, now: Timestamp) -> Result<Option<ImportedPr>, String> {
    let repository = need(node.repository.as_deref(), "repository")?;
    let id = need(node.id.as_deref(), "id")?;
    let name = need(repository.name.as_deref(), "repository.nameWithOwner")?;
    let (Ok(repo), Ok(pr)) = (RepoName::parse(name), NodeId::parse(id)) else {
        return Ok(None);
    };
    let created = need(node.created_at, "createdAt")?;
    let timeline = need(node.timeline.as_ref(), "timelineItems")?;
    let (first, follow_up) = match timeline_step(id, &repo, created, timeline, None, me) {
        Ok(TimelineStep::Answered(at, login)) => (Some((at, login)), None),
        Ok(TimelineStep::Next(after)) => (
            None,
            Some(WorkKind::ImportFacts(ImportKey::Timeline { pr, after })),
        ),
        Ok(TimelineStep::End) | Err(_) => (None, None),
    };
    let links = node
        .closing
        .iter()
        .flat_map(Nodes::iter)
        .filter_map(|issue| {
            Some(IssueLink {
                pr_node_id: id.to_owned(),
                issue_repo: RepoName::parse(&issue.repository.name).ok()?,
                issue_number: issue.number,
                issue_authored_by_me: false,
                issue_commented_by_me: false,
            })
        })
        .collect();
    let meta = repo_meta(repo.clone(), repository, me, now)?;
    let fact = PrFact {
        node_id: id.to_owned(),
        external: is_external(&repo, me),
        repo,
        number: need(node.number, "number")?,
        created_at: created,
        merged_at: node.merged_at,
        closed_at: node.closed_at,
        first_response_at: first.as_ref().map(|(at, _)| *at),
        first_response_login: first.map(|(_, login)| login),
        changes_requested_rounds: need(
            node.reviews.as_ref().and_then(|r| r.total_count),
            "reviews.totalCount",
        )?,
        ci_failures: 0,
        additions: need(node.additions, "additions")?,
        deletions: need(node.deletions, "deletions")?,
        changed_files: need(node.changed_files, "changedFiles")?,
        agent: is_agent_branch(need(node.head_ref.as_deref(), "headRefName")?),
        source: FactSource::Imported,
    };
    Ok(Some(ImportedPr {
        fact,
        links,
        repo: meta,
        timeline: follow_up,
    }))
}

/// A first page over the search limit only queues its two halves. Any other page
/// becomes facts, and a pull request with no human reply in its first timeline
/// page queues the rest of its timeline.
fn plan_range(
    from: Day,
    to: Day,
    after: Option<&str>,
    page: &Page,
    me: &Login,
    now: Timestamp,
) -> Result<RangePlan, String> {
    let count = need(page.issue_count, "issueCount")?;
    if after.is_none() && count > SEARCH_LIMIT {
        if let Some(halves) = halves(from, to) {
            return Ok(RangePlan {
                follow_ups: halves
                    .into_iter()
                    .map(|(from, to)| search_range(from, to, None))
                    .collect(),
                ..RangePlan::default()
            });
        }
    }
    let next = next_cursor(after, page.page_info.as_ref())?;
    let mut plan = RangePlan::default();
    for node in page.nodes.iter().flatten() {
        let Some(pr) = imported_pr(node, me, now)? else {
            continue;
        };
        plan.facts.links.extend(pr.links);
        if !plan.repos.iter().any(|r| r.full_name == pr.repo.full_name) {
            plan.repos.push(pr.repo);
        }
        plan.facts.prs.push(pr.fact);
        plan.follow_ups.extend(pr.timeline);
    }
    if let Some(after) = next {
        plan.follow_ups.push(search_range(from, to, Some(after)));
    }
    Ok(plan)
}

/// One `Reviews` unit for every 50 pull requests on the page, then the next page.
fn plan_reviewed(year: i32, after: Option<&str>, page: &Page) -> Result<Vec<WorkKind>, String> {
    let next = next_cursor(after, page.page_info.as_ref())?;
    let ids: Vec<NodeId> = page
        .nodes
        .iter()
        .flatten()
        .filter_map(|n| NodeId::parse(n.pull_request.as_ref()?.id.as_deref()?).ok())
        .collect();
    let mut follow_ups: Vec<WorkKind> = ids
        .chunks(IMPORT_PAGE)
        .filter_map(|chunk| PrSet::new(chunk.to_vec()))
        .map(|prs| WorkKind::ImportFacts(ImportKey::Reviews { prs, after: None }))
        .collect();
    if let Some(after) = next {
        follow_ups.push(WorkKind::ImportFacts(ImportKey::ReviewedPrs {
            year,
            after: Some(after),
        }));
    }
    Ok(follow_ups)
}

/// One `review_facts` row per review I submitted, and a follow-up for each pull
/// request with more reviews.
fn plan_reviews(
    nodes: &[Option<Node>],
    after: Option<&str>,
    me: &Login,
) -> Result<(Vec<ReviewFact>, Vec<WorkKind>), String> {
    let mut rows = Vec::new();
    let mut follow_ups = Vec::new();
    for pr in nodes.iter().flatten() {
        let repository = need(pr.repository.as_deref(), "repository")?;
        let Ok(repo) = RepoName::parse(need(repository.name.as_deref(), "nameWithOwner")?) else {
            continue;
        };
        let number = need(pr.number, "number")?;
        let author = pr
            .author
            .as_ref()
            .and_then(|a| Login::parse(&a.login).ok())
            .unwrap_or_else(Login::ghost);
        let reviews = need(pr.reviews.as_ref(), "reviews")?;
        for review in reviews.nodes.iter().flatten() {
            let state = need(review.state, "review.state")?;
            if state == ReviewStateNode::Pending {
                continue;
            }
            rows.push(ReviewFact {
                review_id: need(review.id.clone(), "review.id")?,
                repo: repo.clone(),
                external: is_external(&repo, me),
                pr_number: number,
                pr_author_login: author.clone(),
                state: review_state(state),
                submitted_at: need(review.submitted_at.or(review.created_at), "createdAt")?,
                source: FactSource::Imported,
            });
        }
        let next = next_cursor(after, reviews.page_info.as_ref())
            .ok()
            .flatten();
        let prs = pr
            .id
            .as_deref()
            .and_then(|id| NodeId::parse(id).ok())
            .and_then(|id| PrSet::new(vec![id]));
        if let (Some(after), Some(prs)) = (next, prs) {
            follow_ups.push(WorkKind::ImportFacts(ImportKey::Reviews {
                prs,
                after: Some(after),
            }));
        }
    }
    Ok((rows, follow_ups))
}

/// Matches each answered repo to the name Radar stores it under, by node ID.
fn plan_refresh(
    stored: &[(RepoName, String)],
    nodes: &[Option<Node>],
    me: &Login,
    now: Timestamp,
) -> Vec<RepoMeta> {
    nodes
        .iter()
        .flatten()
        .filter_map(|node| {
            let id = node.id.as_deref()?;
            let (name, _) = stored.iter().find(|(_, stored)| stored == id)?;
            repo_meta(name.clone(), node, me, now).ok()
        })
        .collect()
}

/// The one GraphQL request a unit makes.
fn request(key: &ImportKey, me: &Login) -> Option<(QueryDoc, Value)> {
    Some(match key {
        ImportKey::Start => (VIEWER, json!({})),
        ImportKey::SearchRange { from, to, after } => {
            let q = format!(
                "is:pr author:{} created:{}..{}",
                me.as_str(),
                from.date(),
                to.date()
            );
            (
                IMPORT_PRS,
                json!({ "q": q, "first": IMPORT_PAGE, "after": after }),
            )
        }
        ImportKey::Timeline { pr, after } => (
            IMPORT_TIMELINE,
            json!({ "id": pr.as_str(), "after": after }),
        ),
        ImportKey::ReviewedPrs { year, after } => {
            let from = Day::year_start(*year)?;
            let to = Timestamp(Day::year_end(*year)?.start().0 + LAST_SECOND_OF_DAY);
            (
                IMPORT_REVIEWED_PRS,
                json!({ "from": from.start().to_string(), "to": to.to_string(), "after": after }),
            )
        }
        ImportKey::Reviews { prs, after } => (
            IMPORT_REVIEWS,
            json!({ "ids": prs.ids(), "login": me.as_str(), "after": after }),
        ),
    })
}

/// What one unit writes and queues, from the response to its request.
fn plan_unit(key: &ImportKey, data: &Node, me: &Login, now: Timestamp) -> Result<Done, String> {
    match key {
        ImportKey::Start => {
            let created = need(data.viewer.as_ref().and_then(|v| v.created_at), "createdAt")?;
            Ok(Done {
                statements: vec![facts::set_account_created(created)],
                follow_ups: year_units(created, now),
            })
        }
        ImportKey::SearchRange { from, to, after } => {
            let page = need(data.search.as_ref(), "search")?;
            let plan = plan_range(*from, *to, after.as_deref(), page, me, now)?;
            let mut statements = plan.facts.statements();
            if !plan.repos.is_empty() {
                statements.push(repos::upsert_meta(&plan.repos));
            }
            Ok(Done {
                statements,
                follow_ups: plan.follow_ups,
            })
        }
        ImportKey::Timeline { pr, after } => {
            let Some(node) = data.node.as_deref() else {
                return Ok(Done::default());
            };
            let name = need(
                node.repository.as_ref().and_then(|r| r.name.as_deref()),
                "repository.nameWithOwner",
            )?;
            let repo = RepoName::parse(name).map_err(|err| err.to_string())?;
            let step = timeline_step(
                pr.as_str(),
                &repo,
                need(node.created_at, "createdAt")?,
                need(node.timeline.as_ref(), "timelineItems")?,
                Some(after),
                me,
            )?;
            Ok(match step {
                TimelineStep::Answered(at, login) => Done {
                    statements: vec![facts::set_imported_first_response(pr.as_str(), at, &login)],
                    ..Done::default()
                },
                TimelineStep::Next(after) => Done {
                    follow_ups: vec![WorkKind::ImportFacts(ImportKey::Timeline {
                        pr: pr.clone(),
                        after,
                    })],
                    ..Done::default()
                },
                TimelineStep::End => Done::default(),
            })
        }
        ImportKey::ReviewedPrs { year, after } => {
            let page = need(
                data.viewer
                    .as_ref()
                    .and_then(|v| v.collection.as_ref())
                    .and_then(|c| c.review_contributions.as_ref()),
                "pullRequestReviewContributions",
            )?;
            Ok(Done {
                follow_ups: plan_reviewed(*year, after.as_deref(), page)?,
                ..Done::default()
            })
        }
        ImportKey::Reviews { after, .. } => {
            let (rows, follow_ups) = plan_reviews(&data.nodes, after.as_deref(), me)?;
            let statements = if rows.is_empty() {
                Vec::new()
            } else {
                vec![facts::upsert_reviews(&rows)]
            };
            Ok(Done {
                statements,
                follow_ups,
            })
        }
    }
}

fn retry(error: String) -> UnitOutcome {
    UnitOutcome::Retry { error }
}

/// What every unit opens: D1, a GitHub client, and my login.
struct Unit {
    db: D1Database,
    client: Client,
    me: Login,
}

impl Unit {
    /// Reads my login, then runs `reads` and returns their results.
    async fn open(
        env: &Env,
        mut reads: Vec<Statement>,
    ) -> Result<(Self, Vec<worker::D1Result>), UnitOutcome> {
        let Some(token) = auth::load_token(env).await else {
            return Err(UnitOutcome::NoToken);
        };
        let db = env
            .d1("DB")
            .map_err(|err| retry(format!("d1 binding: {err}")))?;
        let base = env
            .var("GITHUB_API_BASE")
            .map(|v| v.to_string())
            .unwrap_or_default();
        let client = Client::new(&base, token).map_err(|err| retry(err.to_string()))?;
        reads.push(batch::me());
        let mut results = crate::db::run(&db, reads)
            .await
            .map_err(|err| retry(format!("d1 read: {err}")))?;
        let me = results
            .pop()
            .ok_or_else(|| retry("d1 returned too few results".to_owned()))?
            .results::<MeRecord>()
            .map_err(|err| retry(err.to_string()))?
            .first()
            .ok_or_else(|| retry("no signed-in account".to_owned()))?
            .login()
            .map_err(|err| retry(err.to_string()))?;
        Ok((Self { db, client, me }, results))
    }

    async fn ask(
        &self,
        doc: &QueryDoc,
        variables: Value,
        state: &mut SyncState,
    ) -> Result<Node, UnitOutcome> {
        match self.client.query::<Node>(doc, variables).await {
            Ok(queried) => {
                state.record_rate(&queried.meta);
                if let Some(limit) = &queried.data.rate_limit {
                    state.rate.remaining = Some(limit.remaining);
                    state.rate.reset = Some(limit.reset_at);
                }
                Ok(queried.data)
            }
            Err(GithubError::RateLimited { retry_after_secs }) => {
                Err(UnitOutcome::RateLimited { retry_after_secs })
            }
            Err(err) => Err(retry(err.to_string())),
        }
    }
}

/// One import unit. The caller has awaited nothing else, so no other GitHub
/// request for the token is in flight (P5). `rows_left` says whether another
/// `import.` row is queued, so the last unit can set `facts_imported_at` in the
/// same D1 batch as its own rows.
pub async fn run(
    key: &ImportKey,
    env: &Env,
    state: &mut SyncState,
    now: Timestamp,
    rows_left: bool,
) -> UnitOutcome {
    let (unit, _) = match Unit::open(env, Vec::new()).await {
        Ok(opened) => opened,
        Err(outcome) => return outcome,
    };
    let Some((doc, variables)) = request(key, &unit.me) else {
        return retry(format!("{} names no calendar year", key.key()));
    };
    let data = match unit.ask(&doc, variables, state).await {
        Ok(data) => data,
        Err(outcome) => return outcome,
    };
    let mut done = match plan_unit(key, &data, &unit.me, now) {
        Ok(done) => done,
        Err(error) => return retry(error),
    };
    let finished = finishes(&done.follow_ups, rows_left);
    if finished {
        done.statements.push(facts::mark_imported(now));
    }
    let (writes, queued) = (done.statements.len(), done.follow_ups.len());
    if let Err(err) = crate::db::run(&unit.db, done.statements).await {
        return retry(format!("d1 write: {err}"));
    }
    console_log!(
        "sync.import key={} writes={writes} follow_ups={queued}",
        key.key()
    );
    if finished {
        state.facts_imported_at = Some(now);
        console_log!("sync.import facts_imported_at={now}");
    }
    UnitOutcome::Done {
        follow_ups: done.follow_ups,
    }
}

/// One page of the weekly repo refresh: up to 50 repos in one `nodes` query.
pub async fn refresh(
    page: &RepoPage,
    env: &Env,
    state: &mut SyncState,
    now: Timestamp,
) -> UnitOutcome {
    let reads = vec![repos::page(page.after.as_ref(), IMPORT_PAGE)];
    let (unit, results) = match Unit::open(env, reads).await {
        Ok(opened) => opened,
        Err(outcome) => return outcome,
    };
    let stored: Vec<(RepoName, String)> = match results.first().map(|r| r.results::<RepoIdRecord>())
    {
        Some(Ok(rows)) => rows
            .into_iter()
            .filter_map(|r| Some((RepoName::parse(&r.full_name).ok()?, r.node_id)))
            .collect(),
        Some(Err(err)) => return retry(err.to_string()),
        None => return retry("d1 returned too few results".to_owned()),
    };
    if stored.is_empty() {
        console_log!("sync.repos week={} repos=0", page.week.date());
        return UnitOutcome::Done { follow_ups: vec![] };
    }
    let ids: Vec<&str> = stored.iter().map(|(_, id)| id.as_str()).collect();
    let data = match unit.ask(&REPO_META, json!({ "ids": ids }), state).await {
        Ok(data) => data,
        Err(outcome) => return outcome,
    };
    let metas = plan_refresh(&stored, &data.nodes, &unit.me, now);
    if !metas.is_empty() {
        if let Err(err) = crate::db::run(&unit.db, vec![repos::upsert_meta(&metas)]).await {
            return retry(format!("d1 write: {err}"));
        }
    }
    let follow_ups = match stored.last() {
        Some((last, _)) if stored.len() == IMPORT_PAGE => vec![WorkKind::RefreshRepos(RepoPage {
            week: page.week,
            after: Some(last.clone()),
        })],
        _ => vec![],
    };
    console_log!(
        "sync.repos week={} repos={} refreshed={} next={}",
        page.week.date(),
        stored.len(),
        metas.len(),
        follow_ups.len()
    );
    UnitOutcome::Done { follow_ups }
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
