//! The handler for `WorkKind::Enrich`. A unit loads its threads from D1, asks
//! GitHub about all of them in one query, classifies each item, and writes every
//! row in one D1 batch. A batch holds notification thread ids. A REST
//! notification has no node id, so each item is addressed by repository and number.

use domain::activity::{Activity, ActivityKind, Actor, ActorKind, ReviewState};
use domain::classify::classify;
use domain::event::{CiOutcome, Event, EventActor, EventId, EventKind};
use domain::ids::{ItemKind, ItemRef, Login, RepoName, Timestamp};
use domain::item::{Ci, CiState, Item, ItemState, ReviewDecision};
use domain::stage::Stage;
use worker::{console_log, Env};

use super::scheduler::UnitOutcome;
use super::state::SyncState;
use super::work::{EnrichBatch, WorkKind};
use crate::auth;
use crate::db::batch::{self, MeRecord, Stored, StoredRecord, Thread, ThreadRecord};
use crate::db::events::{self, EventRow, EventSource};
use crate::db::items::{self, ItemMeta, ItemRow};
use crate::db::Statement;
use crate::github::enrich_query::{build_enrich_query, enrich_variables, Target};
use crate::github::enrich_response::{
    ActorNode, Decision, EnrichData, IssueState, ItemNode, PrState, RepoNode, ReviewStateNode,
    Reviewer, RollupState, TimelineNode,
};
use crate::github::{Client, GithubError};

/// What one thread became.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Enriched(Box<Enriched>),
    /// GitHub no longer has the item. The rest of the batch still writes.
    Gone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Enriched {
    pub row: ItemRow,
    pub events: Vec<Event>,
}

/// Everything a unit writes, decided before any write.
#[derive(Debug, Default, PartialEq)]
pub struct Planned {
    pub items: Vec<ItemRow>,
    pub events: Vec<Event>,
    pub gone: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MissingAlias(pub usize);

/// The reasons GitHub gives when a comment mentions me or my team.
const MENTION_REASONS: [&str; 2] = ["mention", "team_mention"];
const ASSIGN_REASON: &str = "assign";

/// One timeline item, or a latest review that the timeline window did not include.
struct Entry {
    id: String,
    at: Timestamp,
    actor: Actor,
    what: What,
    url: Option<String>,
    in_timeline: bool,
}

enum What {
    Comment,
    Review(ReviewState),
    ReviewRequested(Option<Login>),
    Assigned(Option<Login>),
    Commit,
    Merged,
    Closed,
    Reopened,
}

fn login(raw: &str) -> Login {
    Login::parse(raw).unwrap_or_else(|_| Login::ghost())
}

fn actor(node: Option<&ActorNode>) -> Actor {
    match node {
        Some(node) => Actor {
            login: login(&node.login),
            kind: if node.typename == "Bot" {
                ActorKind::Bot
            } else {
                ActorKind::User
            },
        },
        None => Actor {
            login: Login::ghost(),
            kind: ActorKind::User,
        },
    }
}

/// A team has no login, so a request for one is nobody's turn.
fn person(reviewer: Option<&Reviewer>) -> Option<Login> {
    reviewer
        .filter(|r| r.typename != "Team")
        .and_then(|r| r.login.as_deref())
        .map(login)
}

fn review_state(node: ReviewStateNode) -> ReviewState {
    match node {
        ReviewStateNode::Approved => ReviewState::Approved,
        ReviewStateNode::ChangesRequested => ReviewState::ChangesRequested,
        ReviewStateNode::Commented => ReviewState::Commented,
        ReviewStateNode::Dismissed => ReviewState::Dismissed,
        ReviewStateNode::Pending => ReviewState::Pending,
    }
}

fn entry(node: &TimelineNode) -> Option<Entry> {
    let simple = |id: &String, at: Timestamp, who: Option<&ActorNode>, what: What| Entry {
        id: id.clone(),
        at,
        actor: actor(who),
        what,
        url: None,
        in_timeline: true,
    };
    Some(match node {
        TimelineNode::IssueComment(n) => Entry {
            url: Some(n.url.clone()),
            ..simple(&n.id, n.at, n.actor.as_ref(), What::Comment)
        },
        TimelineNode::PullRequestReview(n) => Entry {
            url: Some(n.url.clone()),
            ..simple(
                &n.id,
                n.at,
                n.actor.as_ref(),
                What::Review(review_state(n.state)),
            )
        },
        TimelineNode::ReviewRequestedEvent(n) => simple(
            &n.id,
            n.at,
            n.actor.as_ref(),
            What::ReviewRequested(person(n.reviewer.as_ref())),
        ),
        TimelineNode::AssignedEvent(n) => simple(
            &n.id,
            n.at,
            n.actor.as_ref(),
            What::Assigned(person(n.assignee.as_ref())),
        ),
        TimelineNode::PullRequestCommit(n) => Entry {
            id: n.id.clone(),
            at: n.commit.at,
            actor: Actor {
                login: n
                    .commit
                    .author
                    .as_ref()
                    .and_then(|a| a.user.as_ref())
                    .map_or_else(Login::ghost, |u| login(&u.login)),
                kind: ActorKind::User,
            },
            what: What::Commit,
            url: Some(n.url.clone()),
            in_timeline: true,
        },
        TimelineNode::MergedEvent(n) => simple(&n.id, n.at, n.actor.as_ref(), What::Merged),
        TimelineNode::ClosedEvent(n) => simple(&n.id, n.at, n.actor.as_ref(), What::Closed),
        TimelineNode::ReopenedEvent(n) => simple(&n.id, n.at, n.actor.as_ref(), What::Reopened),
        TimelineNode::Other => return None,
    })
}

/// The fields the two item types share, in the form the classifier takes.
struct Parts<'a> {
    kind: ItemKind,
    number: u32,
    title: &'a str,
    url: &'a str,
    head_ref: Option<&'a str>,
    state: ItemState,
    created_at: Timestamp,
    updated_at: Timestamp,
    closed_at: Option<Timestamp>,
    author: Actor,
    decision: Option<ReviewDecision>,
    assignees: Vec<Login>,
    requests: Vec<Login>,
    rollup: Option<(&'a str, CiState)>,
    entries: Vec<Entry>,
}

fn rollup_state(state: RollupState) -> CiState {
    match state {
        RollupState::Success => CiState::Passing,
        RollupState::Failure | RollupState::Error => CiState::Failing,
        RollupState::Pending | RollupState::Expected => CiState::Pending,
    }
}

fn parts(node: &ItemNode) -> Parts<'_> {
    match node {
        ItemNode::Issue(issue) => {
            let mut entries: Vec<Entry> = issue.timeline.iter().filter_map(entry).collect();
            entries.sort_by_key(|e| e.at);
            Parts {
                kind: ItemKind::Issue,
                number: issue.number,
                title: &issue.title,
                url: &issue.url,
                head_ref: None,
                state: match issue.state {
                    IssueState::Open => ItemState::Open,
                    IssueState::Closed => ItemState::Closed,
                },
                created_at: issue.created_at,
                updated_at: issue.updated_at,
                closed_at: issue.closed_at,
                author: actor(issue.author.as_ref()),
                decision: None,
                assignees: issue.assignees.iter().map(|a| login(&a.login)).collect(),
                requests: Vec::new(),
                rollup: None,
                entries,
            }
        }
        ItemNode::PullRequest(pr) => {
            let mut entries: Vec<Entry> = pr.timeline.iter().filter_map(entry).collect();
            // The timeline window can end before a reviewer's latest review, and the
            // classifier needs the latest review of every reviewer.
            for review in pr.latest_reviews.iter() {
                if !entries.iter().any(|e| e.id == review.id) {
                    entries.push(Entry {
                        id: review.id.clone(),
                        at: review.at,
                        actor: actor(review.author.as_ref()),
                        what: What::Review(review_state(review.state)),
                        url: Some(review.url.clone()),
                        in_timeline: false,
                    });
                }
            }
            entries.sort_by_key(|e| e.at);
            Parts {
                kind: ItemKind::Pr,
                number: pr.number,
                title: &pr.title,
                url: &pr.url,
                head_ref: Some(&pr.head_ref),
                state: match (pr.state, pr.is_draft) {
                    (PrState::Merged, _) => ItemState::Merged,
                    (PrState::Closed, _) => ItemState::Closed,
                    (PrState::Open, true) => ItemState::Draft,
                    (PrState::Open, false) => ItemState::Open,
                },
                created_at: pr.created_at,
                updated_at: pr.updated_at,
                closed_at: pr.closed_at.or(pr.merged_at),
                author: actor(pr.author.as_ref()),
                decision: pr.review_decision.map(|d| match d {
                    Decision::Approved => ReviewDecision::Approved,
                    Decision::ChangesRequested => ReviewDecision::ChangesRequested,
                    Decision::ReviewRequired => ReviewDecision::ReviewRequired,
                }),
                assignees: pr.assignees.iter().map(|a| login(&a.login)).collect(),
                requests: pr
                    .review_requests
                    .iter()
                    .filter_map(|r| person(r.reviewer.as_ref()))
                    .collect(),
                rollup: pr
                    .commits
                    .iter()
                    .last()
                    .and_then(|c| {
                        c.commit
                            .rollup
                            .as_ref()
                            .map(|r| (c.commit.oid.as_str(), r.state))
                    })
                    .map(|(oid, state)| (oid, rollup_state(state))),
                entries,
            }
        }
    }
}

fn activity_kind(what: &What) -> Option<ActivityKind> {
    match what {
        What::Comment => Some(ActivityKind::Comment { mentions_me: false }),
        What::Review(state) => Some(ActivityKind::Review { state: *state }),
        What::ReviewRequested(reviewer) => reviewer
            .clone()
            .map(|reviewer| ActivityKind::ReviewRequested { reviewer }),
        What::Assigned(assignee) => assignee
            .clone()
            .map(|assignee| ActivityKind::Assigned { assignee }),
        What::Commit => Some(ActivityKind::Pushed),
        What::Merged | What::Closed | What::Reopened => None,
    }
}

/// An activity of mine that answers the other side, as the classifier counts it.
fn answers(kind: &ActivityKind) -> bool {
    match kind {
        ActivityKind::Comment { .. } | ActivityKind::Pushed => true,
        ActivityKind::Review { state } => *state != ReviewState::Pending,
        ActivityKind::ReviewRequested { .. } | ActivityKind::Assigned { .. } => false,
    }
}

/// The notification reason says a comment mentioned me, not which one, so the
/// newest human comment after my last answer carries the mark. Radar never reads
/// a comment body (N5).
fn mentioned(activity: &[(usize, Activity)], me: &Login) -> Option<usize> {
    let my_last_answer = activity
        .iter()
        .filter(|(_, a)| a.actor.login == *me && answers(&a.kind))
        .map(|(_, a)| a.at)
        .max();
    activity
        .iter()
        .filter(|(_, a)| {
            matches!(a.kind, ActivityKind::Comment { .. })
                && a.actor.kind == ActorKind::User
                && a.actor.login != *me
                && my_last_answer.is_none_or(|mine| mine <= a.at)
        })
        .max_by_key(|(_, a)| a.at)
        .map(|(index, _)| *index)
}

fn event_kind(what: &What) -> EventKind {
    match what {
        What::Comment => EventKind::Commented,
        What::Review(state) => EventKind::Reviewed(*state),
        What::ReviewRequested(_) => EventKind::ReviewRequested,
        What::Assigned(_) => EventKind::Assigned,
        What::Commit => EventKind::Committed,
        What::Merged => EventKind::Merged,
        What::Closed => EventKind::Closed,
        What::Reopened => EventKind::Reopened,
    }
}

fn event_actor(actor: &Actor, me: &Login) -> EventActor {
    EventActor {
        login: actor.login.clone(),
        is_me: actor.login == *me,
        is_bot: actor.kind == ActorKind::Bot,
    }
}

/// Turns one answered alias into the rows it writes.
///
/// The classifier needs my last answer, which can predate the last sync, so the
/// query reads the newest timeline items every time and only the events are
/// limited to what happened since the stored `updated_at`.
pub fn outcome(
    thread: &Thread,
    repo: Option<&RepoNode>,
    stored: Option<&Stored>,
    me: &Login,
    now: Timestamp,
) -> Outcome {
    let Some(node) = repo.and_then(|r| r.item.as_ref()) else {
        return Outcome::Gone;
    };
    Outcome::Enriched(Box::new(classify_node(
        node,
        &thread.item.repo,
        &thread.reason,
        stored,
        me,
        now,
    )))
}

/// Classifies one item node and lists the events it carries. `reason` is the
/// notification reason, or empty for a node a search found. The backfill reuses
/// this, so an item found by a notification and by a search is written alike.
pub fn classify_node(
    node: &ItemNode,
    repo: &RepoName,
    reason: &str,
    stored: Option<&Stored>,
    me: &Login,
    now: Timestamp,
) -> Enriched {
    let p = parts(node);
    let node_id = node.id();
    let mut activity: Vec<(usize, Activity)> = p
        .entries
        .iter()
        .enumerate()
        .filter_map(|(index, e)| {
            activity_kind(&e.what).map(|kind| {
                (
                    index,
                    Activity {
                        at: e.at,
                        actor: e.actor.clone(),
                        kind,
                    },
                )
            })
        })
        .collect();
    let mention = MENTION_REASONS
        .contains(&reason)
        .then(|| mentioned(&activity, me))
        .flatten();
    if let Some(index) = mention {
        for (i, a) in &mut activity {
            if *i == index {
                a.kind = ActivityKind::Comment { mentions_me: true };
            }
        }
    }
    let mut assignees = p.assignees.clone();
    if reason == ASSIGN_REASON && !assignees.contains(me) {
        assignees.push(me.clone());
    }

    let latest_known = p
        .entries
        .iter()
        .map(|e| e.at)
        .max()
        .map_or(p.created_at, |at| at.max(p.created_at));
    let previous_ci = stored.and_then(|s| s.ci_state);
    let ci_changed = p
        .rollup
        .is_some_and(|(_, state)| previous_ci != Some(state));
    let observed_at = stored.map_or(latest_known, |_| now);
    let ci = p.rollup.map(|(_, state)| Ci {
        state,
        at: match state {
            CiState::Failing if ci_changed => observed_at,
            CiState::Failing => stored
                .filter(|s| s.stage == Some(Stage::CiFailing))
                .map_or(observed_at, |s| s.stage_since),
            CiState::Passing | CiState::Pending => p.updated_at,
        },
    });

    let item = Item {
        item_ref: ItemRef {
            repo: repo.clone(),
            number: p.number,
            kind: p.kind,
        },
        author: p.author.clone(),
        state: p.state,
        created_at: p.created_at,
        updated_at: p.updated_at,
        closed_at: p.closed_at,
        review_decision: p.decision,
        review_requests: p.requests.clone(),
        assignees,
        ci,
        activity: activity.iter().map(|(_, a)| a.clone()).collect(),
    };
    let classification = classify(&item, me);
    let entry_url = |at: &Activity| {
        activity
            .iter()
            .find(|(_, a)| a == at)
            .and_then(|(i, _)| p.entries.get(*i))
            .and_then(|e| e.url.clone())
    };
    let latest_url = p
        .entries
        .iter()
        .filter(|e| e.url.is_some())
        .max_by_key(|e| e.at)
        .and_then(|e| e.url.clone())
        .unwrap_or_else(|| p.url.to_owned());
    let meta = ItemMeta {
        node_id: node_id.to_owned(),
        title: p.title.to_owned(),
        url: p.url.to_owned(),
        head_ref: p.head_ref.map(str::to_owned),
        last_human_url: classification.last_human.as_ref().and_then(entry_url),
        latest_url,
        stage_since: if ci_changed {
            observed_at
        } else {
            latest_known
        },
    };
    let row = ItemRow::from_classified(meta, &item, me, &classification, now);

    let since = stored.map(|s| s.updated_at);
    let mut events = Vec::new();
    let repo_name = repo;
    let event = |id: EventId, kind: EventKind, actor: Option<EventActor>, at: Timestamp| Event {
        id,
        item: node_id.to_owned(),
        repo: repo_name.clone(),
        kind,
        actor,
        occurred_at: at,
    };
    if stored.is_none() {
        events.push(event(
            EventId::for_open(node_id),
            EventKind::Opened,
            Some(event_actor(&p.author, me)),
            p.created_at,
        ));
    }
    for (index, e) in p.entries.iter().enumerate() {
        if !e.in_timeline || since.is_some_and(|since| e.at < since) {
            continue;
        }
        let who = Some(event_actor(&e.actor, me));
        events.push(event(
            EventId::for_node(&e.id),
            event_kind(&e.what),
            who.clone(),
            e.at,
        ));
        if mention == Some(index) {
            events.push(event(
                EventId::for_mention(&e.id),
                EventKind::Mentioned,
                who,
                e.at,
            ));
        }
    }
    if let (true, Some((oid, state))) = (ci_changed, p.rollup) {
        let outcome = match state {
            CiState::Passing => Some((CiOutcome::Passed, EventKind::CiPassed)),
            CiState::Failing => Some((CiOutcome::Failed, EventKind::CiFailed)),
            CiState::Pending => None,
        };
        if let Some((outcome, kind)) = outcome {
            events.push(event(
                EventId::for_ci(node_id, oid, outcome),
                kind,
                None,
                observed_at,
            ));
        }
    }
    Enriched { row, events }
}

/// Maps every thread to its alias. A response that leaves out an alias is not
/// trusted, so nothing is written for the batch.
pub fn plan(
    threads: &[Thread],
    stored: &[Stored],
    data: &EnrichData,
    me: &Login,
    now: Timestamp,
) -> Result<Planned, MissingAlias> {
    let mut planned = Planned::default();
    for (index, thread) in threads.iter().enumerate() {
        let (_, repo) = data
            .aliases
            .iter()
            .find(|(i, _)| *i == index)
            .ok_or(MissingAlias(index))?;
        let known = stored
            .iter()
            .find(|s| s.repo == thread.item.repo && s.number == thread.item.number);
        match outcome(thread, repo.as_ref(), known, me, now) {
            Outcome::Enriched(enriched) => {
                let Enriched { row, events } = *enriched;
                planned.items.push(row);
                planned.events.extend(events);
            }
            Outcome::Gone => planned.gone.push(thread.id.clone()),
        }
    }
    Ok(planned)
}

/// The statements that store classified items: one upsert per item, then one
/// insert for every event. An event whose id is already stored is left alone, so
/// an item written twice keeps each timeline node once.
pub fn writes(
    rows: &[ItemRow],
    events: &[Event],
    source: EventSource,
    now: Timestamp,
) -> Vec<Statement> {
    let mut statements: Vec<Statement> = rows.iter().map(items::upsert).collect();
    if !events.is_empty() {
        let event_rows: Vec<EventRow> = events
            .iter()
            .map(|e| EventRow::from_event(e, source, now))
            .collect();
        statements.push(events::insert_many(&event_rows));
    }
    statements
}

/// One Enrich unit. The caller has awaited nothing else, so no other GitHub
/// request for the token is in flight (P5).
pub async fn run(
    batch: &EnrichBatch,
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
    let ids = batch.ids();
    let reads = crate::db::run(
        &db,
        vec![batch::threads(&ids), batch::stored_items(&ids), batch::me()],
    )
    .await;
    let loaded = match reads {
        Ok(results) => load(&results),
        Err(err) => Err(format!("d1 read: {err}")),
    };
    let (threads, stored, me) = match loaded {
        Ok(loaded) => loaded,
        Err(error) => return retry(error),
    };
    if threads.is_empty() {
        console_log!(
            "sync.enrich batch={} enriched=0 gone=0 events=0 remaining=-1",
            ids.len()
        );
        return UnitOutcome::Done { follow_ups: vec![] };
    }

    let targets: Vec<Target> = threads
        .iter()
        .map(|t| Target {
            owner: t.item.repo.owner().to_owned(),
            name: t.item.repo.name().to_owned(),
            number: t.item.number,
        })
        .collect();
    let base = env
        .var("GITHUB_API_BASE")
        .map(|v| v.to_string())
        .unwrap_or_default();
    let client = match Client::new(&base, token) {
        Ok(client) => client,
        Err(err) => return retry(err.to_string()),
    };
    let doc = build_enrich_query(targets.len());
    let queried = match client
        .query::<EnrichData>(&doc, enrich_variables(&targets))
        .await
    {
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

    let planned = match plan(&threads, &stored, &queried.data, &me, now) {
        Ok(planned) => planned,
        Err(MissingAlias(index)) => return retry(format!("response has no alias a{index}")),
    };
    for id in &planned.gone {
        console_log!("enrich.gone thread={id}");
    }
    let statements = writes(&planned.items, &planned.events, EventSource::Poll, now);
    if let Err(err) = crate::db::run(&db, statements).await {
        return retry(format!("d1 write: {err}"));
    }
    console_log!(
        "sync.enrich batch={} enriched={} gone={} events={} remaining={}",
        ids.len(),
        planned.items.len(),
        planned.gone.len(),
        planned.events.len(),
        limit.remaining
    );
    let follow_ups = if planned.events.is_empty() {
        vec![]
    } else {
        vec![WorkKind::SendAlerts]
    };
    UnitOutcome::Done { follow_ups }
}

#[cfg(test)]
#[path = "enrich_tests.rs"]
mod tests;

type Loaded = (Vec<Thread>, Vec<Stored>, Login);

fn load(results: &[worker::D1Result]) -> Result<Loaded, String> {
    let read = |index: usize| {
        results
            .get(index)
            .ok_or_else(|| "d1 returned too few results".to_owned())
    };
    let threads = read(0)?
        .results::<ThreadRecord>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(Thread::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let stored = read(1)?
        .results::<StoredRecord>()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(Stored::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let me = read(2)?
        .results::<MeRecord>()
        .map_err(|e| e.to_string())?
        .first()
        .ok_or_else(|| "no signed-in account".to_owned())?
        .login()
        .map_err(|e| e.to_string())?;
    Ok((threads, stored, me))
}
