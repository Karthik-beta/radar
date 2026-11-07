//! The notifications poll. `plan_poll` and `replay_poll` hold the rules as pure
//! functions, and `run` does the GitHub request and the D1 write around them.

use domain::ids::{RepoName, Timestamp};
use serde::Deserialize;
use worker::{console_log, Env};

use super::config::{DEFAULT_POLL_INTERVAL_SECS, ENRICH_BATCH};
use super::handlers::Handled;
use super::scheduler::{rate_limit_until, UnitOutcome};
use super::state::SyncState;
use super::work::{EnrichBatch, NodeId, WorkKind};
use crate::auth;
use crate::db::threads::{self, KnownThread, SubjectKind, ThreadRow};
use crate::github::{ApiPath, Client, Conditional, Fetched, GithubError};

const PAGE_SIZE: u32 = 50;

/// Where the poll is. A scan commits its new `Last-Modified` only when its last
/// page succeeds, so a failed page 2 can never hide the threads on it behind a
/// later 304.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollPhase {
    Idle {
        last_modified: Option<String>,
    },
    Scanning {
        committed: Option<String>,
        /// The `Last-Modified` of page 1, committed when the last page arrives.
        incoming: Option<String>,
        next_page: u32,
        /// When page 1 was requested. A kick after this time asks for another poll.
        started_at: Timestamp,
    },
}

impl Default for PollPhase {
    fn default() -> Self {
        Self::Idle {
            last_modified: None,
        }
    }
}

impl PollPhase {
    /// One `|`-separated line, because serde's enum deserializers cost more Worker
    /// size than this whole module. A `Last-Modified` value never holds `|` or is empty.
    pub fn encode(&self) -> String {
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        match self {
            Self::Idle { last_modified } => format!("idle|{}", text(last_modified)),
            Self::Scanning {
                committed,
                incoming,
                next_page,
                started_at,
            } => format!(
                "scan|{}|{}|{next_page}|{}",
                text(committed),
                text(incoming),
                started_at.0
            ),
        }
    }

    pub fn decode(raw: &str) -> Option<Self> {
        let value = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        let parts: Vec<&str> = raw.split('|').collect();
        match parts.as_slice() {
            ["idle", last_modified] => Some(Self::Idle {
                last_modified: value(last_modified),
            }),
            ["scan", committed, incoming, next_page, started_at] => Some(Self::Scanning {
                committed: value(committed),
                incoming: value(incoming),
                next_page: next_page.parse().ok()?,
                started_at: Timestamp(started_at.parse().ok()?),
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub title: String,
    /// The issue or pull request number from `subject.url`. `None` for other subjects.
    pub number: Option<u32>,
    pub kind: SubjectKind,
}

/// One notification thread as `GET /notifications` sends it. Radar keeps no unread
/// flag, because reading never changes anything in Radar (ADR 0002).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawThread")]
pub struct NotificationThread {
    pub id: NodeId,
    pub reason: String,
    pub updated_at: Timestamp,
    pub subject: Subject,
    pub repository: RepoName,
}

#[derive(Deserialize)]
struct RawThread {
    id: String,
    reason: String,
    updated_at: String,
    subject: RawSubject,
    repository: RawRepo,
}

#[derive(Deserialize)]
struct RawSubject {
    title: String,
    url: Option<String>,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct RawRepo {
    full_name: String,
}

impl TryFrom<RawThread> for NotificationThread {
    type Error = String;

    fn try_from(raw: RawThread) -> Result<Self, String> {
        let id = NodeId::parse(&raw.id).map_err(|err| err.to_string())?;
        let updated_at =
            Timestamp::parse_rfc3339(&raw.updated_at).map_err(|err| err.to_string())?;
        let repository =
            RepoName::parse(&raw.repository.full_name).map_err(|err| err.to_string())?;
        let kind = match raw.subject.kind.as_str() {
            "PullRequest" => SubjectKind::Pr,
            "Issue" => SubjectKind::Issue,
            _ => SubjectKind::Other,
        };
        let number = raw.subject.url.as_deref().and_then(url_number);
        if kind != SubjectKind::Other && number.is_none() {
            return Err(format!(
                "thread {} has no item number in its subject url",
                raw.id
            ));
        }
        Ok(Self {
            id,
            reason: raw.reason,
            updated_at,
            subject: Subject {
                title: raw.subject.title,
                number,
                kind,
            },
            repository,
        })
    }
}

fn url_number(url: &str) -> Option<u32> {
    let last = url.trim_end_matches('/').rsplit('/').next()?;
    last.parse().ok().filter(|n| *n > 0)
}

impl NotificationThread {
    fn row(&self, synced_at: Timestamp) -> ThreadRow {
        ThreadRow {
            id: self.id.as_str().to_owned(),
            repo: self.repository.clone(),
            subject_kind: self.subject.kind,
            subject_title: self.subject.title.clone(),
            subject_number: self.subject.number,
            item_node_id: None,
            reason: self.reason.clone(),
            updated_at: self.updated_at,
            synced_at,
        }
    }

    fn is_item(&self) -> bool {
        self.subject.kind != SubjectKind::Other
    }
}

/// What one request brought back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Received {
    pub page: u32,
    pub poll_interval: Option<u32>,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    NotModified,
    Page {
        threads: Vec<NotificationThread>,
        last_modified: Option<String>,
        next: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowUp {
    None,
    NextPage(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollPlan {
    pub upserts: Vec<ThreadRow>,
    /// Pull request and issue threads whose `updated_at` is new to Radar, in groups of
    /// at most `ENRICH_BATCH` (P3).
    pub enrich_batches: Vec<Vec<NodeId>>,
    /// Threads on the page that are new or changed, of any subject kind.
    pub changed: usize,
    pub next_phase: PollPhase,
    /// `None` while a scan is under way, so its next page runs at once.
    pub next_poll_at: Option<Timestamp>,
    pub follow_up: FollowUp,
    /// The page-1 request time of a poll that finished with this response.
    pub finished_from: Option<Timestamp>,
}

pub fn plan_poll(
    phase: &PollPhase,
    received: &Received,
    known: &[KnownThread],
    now: Timestamp,
) -> PollPlan {
    let interval = received.poll_interval.unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
    let poll_at = Timestamp(now.0.saturating_add(i64::from(interval)));
    let (threads, last_modified, next) = match &received.body {
        Body::NotModified => {
            return PollPlan {
                upserts: Vec::new(),
                enrich_batches: Vec::new(),
                changed: 0,
                next_phase: phase.clone(),
                next_poll_at: Some(poll_at),
                follow_up: FollowUp::None,
                finished_from: Some(now),
            }
        }
        Body::Page {
            threads,
            last_modified,
            next,
        } => (threads, last_modified, next.filter(|n| *n > received.page)),
    };
    let changed: Vec<&NotificationThread> = threads
        .iter()
        .filter(|t| {
            let stored = known.iter().find(|row| row.id == t.id.as_str());
            stored.map(|row| row.updated_at) != Some(t.updated_at.0)
        })
        .collect();
    let to_enrich: Vec<NodeId> = changed
        .iter()
        .filter(|t| t.is_item())
        .map(|t| t.id.clone())
        .collect();
    let (committed, incoming, started_at) = match (phase, received.page) {
        (
            PollPhase::Scanning {
                committed,
                incoming,
                started_at,
                ..
            },
            page,
        ) if page > 1 => (committed.clone(), incoming.clone(), *started_at),
        (
            PollPhase::Scanning { committed, .. }
            | PollPhase::Idle {
                last_modified: committed,
            },
            _,
        ) => (committed.clone(), last_modified.clone(), now),
    };
    let (next_phase, next_poll_at, follow_up, finished_from) = match next {
        Some(page) => (
            PollPhase::Scanning {
                committed,
                incoming,
                next_page: page,
                started_at,
            },
            None,
            FollowUp::NextPage(page),
            None,
        ),
        None => (
            PollPhase::Idle {
                last_modified: incoming,
            },
            Some(poll_at),
            FollowUp::None,
            Some(started_at),
        ),
    };
    PollPlan {
        upserts: threads.iter().map(|t| t.row(now)).collect(),
        enrich_batches: to_enrich
            .chunks(ENRICH_BATCH)
            .map(<[NodeId]>::to_vec)
            .collect(),
        changed: changed.len(),
        next_phase,
        next_poll_at,
        follow_up,
        finished_from,
    }
}

/// A kick that landed after page 1 was requested coalesced into a poll that may not
/// have seen what the kick was about, so poll page 1 again once `next_poll_at` allows (P2).
pub fn replay_poll(
    finished_from: Option<Timestamp>,
    last_kick_at: Option<Timestamp>,
    next_poll_at: Option<Timestamp>,
    now: Timestamp,
) -> Option<(WorkKind, Timestamp)> {
    let started = finished_from?;
    last_kick_at
        .filter(|kick| *kick > started)
        .map(|_| (WorkKind::PollNotifications, next_poll_at.unwrap_or(now)))
}

/// The page number of the `rel="next"` relation in a `Link` header.
pub fn next_page(link: Option<&str>) -> Option<u32> {
    link?.split(',').find_map(|part| {
        let (target, params) = part.split_once('>')?;
        let is_next = params.split(';').any(|p| p.trim() == "rel=\"next\"");
        let query = target.split_once('?')?.1;
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix("page=")?.parse().ok())
            .filter(|_| is_next)
    })
}

pub async fn run(env: &Env, state: &mut SyncState, now: Timestamp) -> Handled {
    poll(env, state, now).await.unwrap_or_else(Handled::from)
}

fn retry(err: impl ToString) -> UnitOutcome {
    UnitOutcome::Retry {
        error: err.to_string(),
    }
}

async fn poll(env: &Env, state: &mut SyncState, now: Timestamp) -> Result<Handled, UnitOutcome> {
    let token = auth::load_token(env).await.ok_or(UnitOutcome::NoToken)?;
    let base = env
        .var("GITHUB_API_BASE")
        .map(|v| v.to_string())
        .unwrap_or_default();
    let client = Client::new(&base, token).map_err(retry)?;
    let (page, cond) = match &state.poll_phase {
        PollPhase::Idle {
            last_modified: Some(since),
        } => (1, Conditional::IfModifiedSince(since.clone())),
        PollPhase::Idle { .. } => (1, Conditional::None),
        PollPhase::Scanning { next_page, .. } => (*next_page, Conditional::None),
    };
    let sent_condition = cond != Conditional::None;
    let path = ApiPath::new(&format!(
        "/notifications?all=true&per_page={PAGE_SIZE}&page={page}"
    ))
    .map_err(retry)?;
    let received = match client.get::<Vec<NotificationThread>>(path, cond).await {
        Ok(Fetched::NotModified { meta }) if sent_condition => {
            state.record_rate(&meta);
            Received {
                page,
                poll_interval: meta.poll_interval,
                body: Body::NotModified,
            }
        }
        Ok(Fetched::NotModified { .. }) => return Err(retry("304 to a request with no condition")),
        Ok(Fetched::Modified {
            body,
            validators,
            link,
            meta,
        }) => {
            state.record_rate(&meta);
            Received {
                page,
                poll_interval: meta.poll_interval,
                body: Body::Page {
                    threads: body,
                    last_modified: validators.last_modified,
                    next: next_page(link.as_deref()),
                },
            }
        }
        Err(GithubError::RateLimited { retry_after_secs }) => {
            let retry_at = rate_limit_until(state.throttle.strikes, retry_after_secs, now);
            console_log!("sync.poll status=429 retry_at={retry_at}");
            return Err(UnitOutcome::RateLimited { retry_after_secs });
        }
        Err(err) => return Err(retry(err)),
    };
    let known = match &received.body {
        Body::Page { threads, .. } => known_updated_at(env, threads).await.map_err(retry)?,
        Body::NotModified => Vec::new(),
    };
    let plan = plan_poll(&state.poll_phase, &received, &known, now);
    let db = env.d1("DB").map_err(retry)?;
    let writes = plan.upserts.iter().map(threads::upsert).collect();
    crate::db::run(&db, writes).await.map_err(retry)?;

    let mut follow_ups = Vec::new();
    for ids in &plan.enrich_batches {
        let batch = EnrichBatch::new(ids.clone()).map_err(retry)?;
        console_log!("sync.enqueue kind=enrich threads={}", ids.len());
        follow_ups.push(WorkKind::Enrich(batch));
    }
    if let FollowUp::NextPage(_) = plan.follow_up {
        follow_ups.push(WorkKind::PollNotifications);
    }
    match &received.body {
        Body::NotModified => console_log!(
            "sync.poll status=304 next_poll_at={}",
            display_at(plan.next_poll_at)
        ),
        Body::Page { threads, .. } => console_log!(
            "sync.poll page={page} status=200 threads={} changed={} next_poll_at={}",
            threads.len(),
            plan.changed,
            display_at(plan.next_poll_at)
        ),
    }
    state.poll_phase = plan.next_phase;
    if plan.next_poll_at.is_some() {
        state.next_poll_at = plan.next_poll_at;
    }
    Ok(Handled {
        outcome: UnitOutcome::Done { follow_ups },
        finished_from: plan.finished_from,
    })
}

fn display_at(at: Option<Timestamp>) -> String {
    at.map_or_else(|| "none".to_owned(), |at| at.to_string())
}

async fn known_updated_at(
    env: &Env,
    page: &[NotificationThread],
) -> worker::Result<Vec<KnownThread>> {
    let ids: Vec<&str> = page.iter().map(|t| t.id.as_str()).collect();
    let db = env.d1("DB")?;
    threads::known_updated_at(&ids)
        .prepare(&db)?
        .all()
        .await?
        .results::<KnownThread>()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE1: &str = include_str!("../../tests/fixtures/notifications_page1.json");
    const PAGE2: &str = include_str!("../../tests/fixtures/notifications_page2.json");
    const NOW: Timestamp = Timestamp(1_793_000_000);
    const LAST_MODIFIED: &str = "Sat, 07 Nov 2026 09:55:00 GMT";

    fn parse(json: &str) -> Vec<NotificationThread> {
        serde_json::from_str(json).unwrap()
    }

    fn at(offset: i64) -> Timestamp {
        Timestamp(NOW.0 + offset)
    }

    fn idle() -> PollPhase {
        PollPhase::Idle {
            last_modified: Some("old".into()),
        }
    }

    fn page(number: u32, threads: Vec<NotificationThread>, next: Option<u32>) -> Received {
        Received {
            page: number,
            poll_interval: Some(60),
            body: Body::Page {
                threads,
                last_modified: Some(LAST_MODIFIED.into()),
                next,
            },
        }
    }

    fn not_modified(interval: Option<u32>) -> Received {
        Received {
            page: 1,
            poll_interval: interval,
            body: Body::NotModified,
        }
    }

    fn item_threads(count: usize) -> Vec<NotificationThread> {
        (0..count)
            .map(|i| NotificationThread {
                id: NodeId::parse(&(9000 + i).to_string()).unwrap(),
                reason: "subscribed".into(),
                updated_at: at(-100),
                subject: Subject {
                    title: format!("PR {i}"),
                    number: Some(1 + u32::try_from(i).unwrap()),
                    kind: SubjectKind::Pr,
                },
                repository: RepoName::parse("octo/widgets").unwrap(),
            })
            .collect()
    }

    fn ids(batch: &[NodeId]) -> Vec<&str> {
        batch.iter().map(NodeId::as_str).collect()
    }

    #[test]
    fn a_first_page_of_three_threads_yields_three_upserts_and_one_batch_of_two() {
        let plan = plan_poll(&idle(), &page(1, parse(PAGE1), None), &[], NOW);
        assert_eq!(plan.upserts.len(), 3);
        assert_eq!(plan.changed, 3);
        assert_eq!(plan.enrich_batches.len(), 1);
        assert_eq!(
            ids(&plan.enrich_batches[0]),
            ["18402011001", "18402011002"],
            "the CheckSuite is stored and never enriched"
        );
        assert!(plan.upserts.iter().all(|row| row.synced_at == NOW));
        assert_eq!(plan.upserts[0].subject_number, Some(412));
        assert_eq!(plan.upserts[1].subject_kind, SubjectKind::Issue);
        assert_eq!(plan.upserts[2].subject_kind, SubjectKind::Other);
        assert_eq!(plan.upserts[2].subject_number, None);
    }

    #[test]
    fn a_thread_with_an_unchanged_updated_at_is_stored_and_not_enriched() {
        let threads = parse(PAGE1);
        let known = [KnownThread {
            id: threads[0].id.as_str().to_owned(),
            updated_at: threads[0].updated_at.0,
        }];
        let plan = plan_poll(&idle(), &page(1, threads, None), &known, NOW);
        assert_eq!(plan.upserts.len(), 3);
        assert_eq!(plan.changed, 2);
        assert_eq!(ids(&plan.enrich_batches[0]), ["18402011002"]);
    }

    #[test]
    fn a_stored_thread_with_a_different_updated_at_is_enriched_again() {
        let threads = parse(PAGE1);
        let known = [KnownThread {
            id: threads[0].id.as_str().to_owned(),
            updated_at: at(-99_999).0,
        }];
        let plan = plan_poll(&idle(), &page(1, threads, None), &known, NOW);
        assert_eq!(plan.enrich_batches[0].len(), 2);
    }

    #[test]
    fn forty_five_changed_threads_yield_full_batches_then_the_remainder() {
        let plan = plan_poll(&idle(), &page(1, item_threads(45), None), &[], NOW);
        let sizes: Vec<usize> = plan.enrich_batches.iter().map(Vec::len).collect();
        let (last, full) = sizes.split_last().unwrap();
        assert!(full.iter().all(|n| *n == ENRICH_BATCH), "{sizes:?}");
        assert_eq!(*last, 45 - ENRICH_BATCH * full.len());
        assert!((1..=ENRICH_BATCH).contains(last), "{sizes:?}");
        assert_eq!(sizes.iter().sum::<usize>(), 45);
    }

    #[test]
    fn enrich_batches_hold_each_changed_id_once_in_page_order() {
        let plan = plan_poll(&idle(), &page(1, item_threads(45), None), &[], NOW);
        let flat: Vec<&str> = plan.enrich_batches.iter().flat_map(|b| ids(b)).collect();
        let expected: Vec<String> = (0..45).map(|i| (9000 + i).to_string()).collect();
        assert_eq!(
            flat,
            expected.iter().map(String::as_str).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_page_with_a_next_link_scans_and_does_not_commit_the_new_last_modified() {
        let plan = plan_poll(&idle(), &page(1, parse(PAGE1), Some(2)), &[], NOW);
        assert_eq!(
            plan.next_phase,
            PollPhase::Scanning {
                committed: Some("old".into()),
                incoming: Some(LAST_MODIFIED.into()),
                next_page: 2,
                started_at: NOW,
            }
        );
        assert_eq!(plan.follow_up, FollowUp::NextPage(2));
        assert_eq!(plan.next_poll_at, None);
        assert_eq!(plan.finished_from, None);
    }

    #[test]
    fn the_last_page_commits_the_page_one_last_modified() {
        let scanning = PollPhase::Scanning {
            committed: Some("old".into()),
            incoming: Some("page one value".into()),
            next_page: 3,
            started_at: at(-30),
        };
        let mut last = page(3, parse(PAGE2), None);
        if let Body::Page { last_modified, .. } = &mut last.body {
            *last_modified = Some("page three value".into());
        }
        let plan = plan_poll(&scanning, &last, &[], NOW);
        assert_eq!(
            plan.next_phase,
            PollPhase::Idle {
                last_modified: Some("page one value".into())
            }
        );
        assert_eq!(plan.follow_up, FollowUp::None);
        assert_eq!(plan.next_poll_at, Some(at(60)));
        assert_eq!(plan.finished_from, Some(at(-30)));
    }

    #[test]
    fn a_middle_page_keeps_the_scan_and_its_start() {
        let scanning = PollPhase::Scanning {
            committed: None,
            incoming: Some("one".into()),
            next_page: 2,
            started_at: at(-10),
        };
        let plan = plan_poll(&scanning, &page(2, parse(PAGE2), Some(3)), &[], NOW);
        assert_eq!(
            plan.next_phase,
            PollPhase::Scanning {
                committed: None,
                incoming: Some("one".into()),
                next_page: 3,
                started_at: at(-10),
            }
        );
    }

    #[test]
    fn a_single_page_commits_its_own_last_modified() {
        let plan = plan_poll(
            &PollPhase::default(),
            &page(1, parse(PAGE1), None),
            &[],
            NOW,
        );
        assert_eq!(
            plan.next_phase,
            PollPhase::Idle {
                last_modified: Some(LAST_MODIFIED.into())
            }
        );
        assert_eq!(plan.finished_from, Some(NOW));
    }

    #[test]
    fn a_next_link_that_does_not_advance_ends_the_scan() {
        let plan = plan_poll(&idle(), &page(1, parse(PAGE1), Some(1)), &[], NOW);
        assert_eq!(plan.follow_up, FollowUp::None);
        assert!(matches!(plan.next_phase, PollPhase::Idle { .. }));
    }

    #[test]
    fn a_304_keeps_the_phase_and_waits_the_interval() {
        let plan = plan_poll(&idle(), &not_modified(Some(60)), &[], NOW);
        assert_eq!(plan.next_phase, idle());
        assert_eq!(plan.next_poll_at, Some(at(60)));
        assert!(plan.upserts.is_empty() && plan.enrich_batches.is_empty());
        assert_eq!(plan.follow_up, FollowUp::None);
    }

    #[test]
    fn a_missing_poll_interval_is_60_seconds_and_a_header_value_is_obeyed() {
        let missing = plan_poll(&idle(), &not_modified(None), &[], NOW);
        assert_eq!(missing.next_poll_at, Some(at(60)));
        let slow = plan_poll(&idle(), &not_modified(Some(120)), &[], NOW);
        assert_eq!(slow.next_poll_at, Some(at(120)));
        let mut no_header = page(1, parse(PAGE1), None);
        no_header.poll_interval = None;
        let page_plan = plan_poll(&idle(), &no_header, &[], NOW);
        assert_eq!(
            page_plan.next_poll_at,
            Some(at(DEFAULT_POLL_INTERVAL_SECS.into()))
        );
    }

    #[test]
    fn a_page_missing_updated_at_fails_as_a_whole() {
        let mut value: serde_json::Value = serde_json::from_str(PAGE1).unwrap();
        value[1].as_object_mut().unwrap().remove("updated_at");
        let parsed = serde_json::from_value::<Vec<NotificationThread>>(value);
        assert!(parsed.is_err());
    }

    #[test]
    fn a_page_with_a_bad_timestamp_or_repo_fails_as_a_whole() {
        for (path, bad) in [
            ("updated_at", serde_json::json!("yesterday")),
            ("repository", serde_json::json!({ "full_name": "no-slash" })),
        ] {
            let mut value: serde_json::Value = serde_json::from_str(PAGE1).unwrap();
            value[0][path] = bad;
            assert!(
                serde_json::from_value::<Vec<NotificationThread>>(value).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn a_pull_request_without_a_number_fails_the_page() {
        let mut value: serde_json::Value = serde_json::from_str(PAGE1).unwrap();
        value[0]["subject"]["url"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<Vec<NotificationThread>>(value).is_err());
    }

    #[test]
    fn an_unknown_subject_type_parses_as_other_and_unknown_fields_are_ignored() {
        let mut value: serde_json::Value = serde_json::from_str(PAGE1).unwrap();
        value[0]["subject"]["type"] = "Discussion".into();
        value[0]["brand_new_field"] = serde_json::json!({ "anything": [1, 2, 3] });
        let threads: Vec<NotificationThread> = serde_json::from_value(value).unwrap();
        assert_eq!(threads[0].subject.kind, SubjectKind::Other);
        assert_eq!(threads[0].subject.number, Some(412));
    }

    #[test]
    fn both_fixture_pages_parse_into_six_distinct_threads() {
        let mut all = parse(PAGE1);
        all.extend(parse(PAGE2));
        let mut ids: Vec<&str> = all.iter().map(|t| t.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 6);
    }

    #[test]
    fn link_parser_finds_one_two_and_no_next_relation() {
        let one =
            "<https://api.github.com/notifications?all=true&per_page=50&page=2>; rel=\"next\"";
        assert_eq!(next_page(Some(one)), Some(2));
        let two = "<https://api.github.com/notifications?all=true&per_page=50&page=3>; rel=\"next\", \
                   <https://api.github.com/notifications?all=true&per_page=50&page=9>; rel=\"last\"";
        assert_eq!(next_page(Some(two)), Some(3));
        let last_first = "<https://api.github.com/notifications?page=9>; rel=\"last\", \
                          <https://api.github.com/notifications?page=4>; rel=\"next\"";
        assert_eq!(next_page(Some(last_first)), Some(4));
        let none = "<https://api.github.com/notifications?page=1>; rel=\"prev\", \
                    <https://api.github.com/notifications?page=1>; rel=\"first\"";
        assert_eq!(next_page(Some(none)), None);
        assert_eq!(next_page(None), None);
        assert_eq!(next_page(Some("")), None);
        assert_eq!(next_page(Some("<https://x/y>; rel=\"next\"")), None);
    }

    #[test]
    fn a_kick_during_a_page_one_304_polls_again() {
        let plan = plan_poll(&idle(), &not_modified(Some(60)), &[], NOW);
        let again = replay_poll(plan.finished_from, Some(at(1)), plan.next_poll_at, at(2));
        assert_eq!(again, Some((WorkKind::PollNotifications, at(60))));
        let quiet = replay_poll(plan.finished_from, Some(at(-5)), plan.next_poll_at, at(2));
        assert_eq!(quiet, None);
        assert_eq!(
            replay_poll(plan.finished_from, None, plan.next_poll_at, at(2)),
            None
        );
    }

    #[test]
    fn a_kick_during_a_single_page_poll_polls_again() {
        let plan = plan_poll(
            &PollPhase::default(),
            &page(1, parse(PAGE1), None),
            &[],
            NOW,
        );
        let again = replay_poll(plan.finished_from, Some(at(1)), plan.next_poll_at, at(2));
        assert_eq!(again, Some((WorkKind::PollNotifications, at(60))));
        assert_eq!(
            replay_poll(plan.finished_from, Some(NOW), plan.next_poll_at, at(2)),
            None,
            "a kick in the second the request started is not after it"
        );
    }

    #[test]
    fn a_kick_during_a_scan_polls_page_one_after_the_scan() {
        let started = at(-40);
        let scanning = PollPhase::Scanning {
            committed: None,
            incoming: Some("one".into()),
            next_page: 3,
            started_at: started,
        };
        let middle = plan_poll(
            &PollPhase::Scanning {
                committed: None,
                incoming: Some("one".into()),
                next_page: 2,
                started_at: started,
            },
            &page(2, parse(PAGE2), Some(3)),
            &[],
            at(-20),
        );
        assert_eq!(
            replay_poll(
                middle.finished_from,
                Some(at(-30)),
                middle.next_poll_at,
                at(-20)
            ),
            None,
            "the scan is not over, so nothing replays yet"
        );
        let last = plan_poll(&scanning, &page(3, parse(PAGE2), None), &[], NOW);
        let again = replay_poll(last.finished_from, Some(at(-30)), last.next_poll_at, NOW);
        assert_eq!(again, Some((WorkKind::PollNotifications, at(60))));
        let before_page_one =
            replay_poll(last.finished_from, Some(at(-41)), last.next_poll_at, NOW);
        assert_eq!(before_page_one, None);
    }

    #[test]
    fn the_phase_round_trips_through_its_text_form() {
        for phase in [
            PollPhase::default(),
            idle(),
            PollPhase::Scanning {
                committed: Some("a".into()),
                incoming: None,
                next_page: 4,
                started_at: NOW,
            },
            PollPhase::Scanning {
                committed: None,
                incoming: Some(LAST_MODIFIED.into()),
                next_page: 2,
                started_at: at(-7),
            },
        ] {
            assert_eq!(PollPhase::decode(&phase.encode()).as_ref(), Some(&phase));
        }
    }

    #[test]
    fn a_text_that_is_not_a_phase_decodes_to_none() {
        for bad in [
            "",
            "idle",
            "idle|a|b",
            "scan|a|b|x|1",
            "scan|a|b|2",
            "scan||||",
            "other|a",
        ] {
            assert_eq!(PollPhase::decode(bad), None, "{bad:?}");
        }
    }
}
