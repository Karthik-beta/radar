//! Contribution facts (ADR 0010): one row per pull request I authored, review I
//! submitted, stage or turn change, and issue a pull request of mine closes.
//! The fact tables keep them for good, so no fact holds a title, a body, or
//! comment text (N5).

use crate::activity::ReviewState;
use crate::event::{Event, EventKind};
use crate::ids::{ItemKind, Login, RepoName, Timestamp};
use crate::item::Item;
use crate::repo_stats::first_responder;
use crate::stage::Stage;
use crate::turn::Turn;

const AGENT_PREFIX: &str = "agent/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactSource {
    Live,
    Imported,
}

/// The `turn` column of a transition. A change of reason inside my turn is not
/// a change of side, so it is not a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Me,
    Them,
}

impl From<Turn> for Side {
    fn from(turn: Turn) -> Self {
        match turn {
            Turn::Me(_) => Self::Me,
            Turn::Them => Self::Them,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrFact {
    pub node_id: String,
    pub repo: RepoName,
    pub external: bool,
    pub number: u32,
    pub created_at: Timestamp,
    pub merged_at: Option<Timestamp>,
    pub closed_at: Option<Timestamp>,
    pub first_response_at: Option<Timestamp>,
    pub first_response_login: Option<Login>,
    pub changes_requested_rounds: u32,
    pub ci_failures: u32,
    pub additions: u32,
    pub deletions: u32,
    pub changed_files: u32,
    pub agent: bool,
    pub source: FactSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewFact {
    pub review_id: String,
    pub repo: RepoName,
    pub external: bool,
    pub pr_number: u32,
    pub pr_author_login: Login,
    pub state: ReviewState,
    pub submitted_at: Timestamp,
    pub source: FactSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub item_node_id: String,
    pub stage: Stage,
    pub turn: Side,
    pub at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueLink {
    pub pr_node_id: String,
    pub issue_repo: RepoName,
    pub issue_number: u32,
    pub issue_authored_by_me: bool,
    pub issue_commented_by_me: bool,
}

/// The pull request fields that `Item` does not carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDetails {
    pub node_id: String,
    pub merged_at: Option<Timestamp>,
    pub head_ref: String,
    pub additions: u32,
    pub deletions: u32,
    pub changed_files: u32,
    /// Reviews in state `changes_requested`, from every reviewer.
    pub changes_requested: u32,
}

/// An issue a pull request closes, as GitHub names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosingIssue {
    pub repo: RepoName,
    pub number: u32,
}

/// An item Radar holds, enough to tell whether it is an issue I opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownItem {
    pub node_id: String,
    pub repo: RepoName,
    pub number: u32,
    pub kind: ItemKind,
    pub authored_by_me: bool,
}

/// A repo is external when its owner is not me. An organisation I belong to
/// still counts as external.
pub fn is_external(repo: &RepoName, me: &Login) -> bool {
    !repo.owner().eq_ignore_ascii_case(me.as_str())
}

/// The fact for a pull request I authored, or `None` for anyone else's.
/// `events` holds what Radar knows of the pull request, from `events` and
/// `own_events` together.
pub fn pr_fact(item: &Item, enriched: &PrDetails, events: &[Event], me: &Login) -> Option<PrFact> {
    if item.item_ref.kind != ItemKind::Pr || item.author.login != *me {
        return None;
    }
    let mine: Vec<Event> = events
        .iter()
        .filter(|e| e.item == enriched.node_id)
        .cloned()
        .collect();
    let first = first_responder(item.created_at, &mine);
    let ci_failures = mine
        .iter()
        .filter(|e| e.kind == EventKind::CiFailed)
        .count();
    Some(PrFact {
        node_id: enriched.node_id.clone(),
        repo: item.item_ref.repo.clone(),
        external: is_external(&item.item_ref.repo, me),
        number: item.item_ref.number,
        created_at: item.created_at,
        merged_at: enriched.merged_at,
        closed_at: item.closed_at,
        first_response_at: first.map(|e| e.occurred_at),
        first_response_login: first
            .and_then(|e| e.actor.as_ref())
            .map(|a| a.login.clone()),
        changes_requested_rounds: enriched.changes_requested,
        ci_failures: u32::try_from(ci_failures).unwrap_or(u32::MAX),
        additions: enriched.additions,
        deletions: enriched.deletions,
        changed_files: enriched.changed_files,
        agent: enriched.head_ref.starts_with(AGENT_PREFIX),
        source: FactSource::Live,
    })
}

/// A row when the stage or the side changed, or when Radar first sees the pull
/// request.
pub fn transition(
    item_node_id: &str,
    previous: Option<(Stage, Side)>,
    next: (Stage, Side),
    at: Timestamp,
) -> Option<Transition> {
    (previous != Some(next)).then(|| Transition {
        item_node_id: item_node_id.to_owned(),
        stage: next.0,
        turn: next.1,
        at,
    })
}

/// One link per issue the pull request closes. The flags say what Radar knows
/// from `items` and `events` at write time.
pub fn issue_links(
    pr_node_id: &str,
    closing: &[ClosingIssue],
    items: &[KnownItem],
    events: &[Event],
) -> Vec<IssueLink> {
    closing
        .iter()
        .map(|issue| {
            let known: Vec<&KnownItem> = items
                .iter()
                .filter(|i| {
                    i.kind == ItemKind::Issue && i.repo == issue.repo && i.number == issue.number
                })
                .collect();
            let commented = events.iter().any(|e| {
                e.kind == EventKind::Commented
                    && e.actor.as_ref().is_some_and(|a| a.is_me)
                    && known.iter().any(|i| i.node_id == e.item)
            });
            IssueLink {
                pr_node_id: pr_node_id.to_owned(),
                issue_repo: issue.repo.clone(),
                issue_number: issue.number,
                issue_authored_by_me: known.iter().any(|i| i.authored_by_me),
                issue_commented_by_me: commented,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{Actor, ActorKind};
    use crate::event::{EventActor, EventId};
    use crate::ids::ItemRef;
    use crate::item::ItemState;
    use crate::turn::Reason;

    fn login(s: &str) -> Login {
        Login::parse(s).expect("login")
    }

    fn repo(s: &str) -> RepoName {
        RepoName::parse(s).expect("repo")
    }

    fn me() -> Login {
        login("karthik")
    }

    fn item(author: &str, repo_name: &str) -> Item {
        Item {
            item_ref: ItemRef {
                repo: repo(repo_name),
                number: 7,
                kind: ItemKind::Pr,
            },
            author: Actor {
                login: login(author),
                kind: ActorKind::User,
            },
            state: ItemState::Open,
            created_at: Timestamp(1_000),
            updated_at: Timestamp(5_000),
            closed_at: None,
            review_decision: None,
            review_requests: Vec::new(),
            assignees: Vec::new(),
            ci: None,
            activity: Vec::new(),
        }
    }

    fn details(head: &str) -> PrDetails {
        PrDetails {
            node_id: "PR_7".to_owned(),
            merged_at: None,
            head_ref: head.to_owned(),
            additions: 40,
            deletions: 2,
            changed_files: 3,
            changes_requested: 2,
        }
    }

    fn event(
        id: &str,
        item: &str,
        kind: EventKind,
        who: Option<(&str, bool, bool)>,
        at: i64,
    ) -> Event {
        Event {
            id: EventId::for_node(id),
            item: item.to_owned(),
            repo: repo("octo/widgets"),
            kind,
            actor: who.map(|(name, is_me, is_bot)| EventActor {
                login: login(name),
                is_me,
                is_bot,
            }),
            occurred_at: Timestamp(at),
        }
    }

    #[test]
    fn a_repo_i_own_is_not_external_whatever_the_case() {
        assert!(!is_external(&repo("karthik/radar"), &me()));
        assert!(!is_external(&repo("Karthik/x"), &me()));
        assert!(is_external(&repo("rust-lang/rust"), &me()));
        assert!(is_external(&repo("karthik-beta/radar"), &me()));
    }

    #[test]
    fn pr_fact_is_none_for_a_pull_request_by_someone_else() {
        let fact = pr_fact(&item("alice", "octo/widgets"), &details("x"), &[], &me());
        assert_eq!(fact, None);
        let mut issue = item("karthik", "octo/widgets");
        issue.item_ref.kind = ItemKind::Issue;
        assert_eq!(pr_fact(&issue, &details("x"), &[], &me()), None);
    }

    #[test]
    fn pr_fact_sets_agent_only_for_agent_heads() {
        let mine = item("Karthik", "octo/widgets");
        let agent = |head| pr_fact(&mine, &details(head), &[], &me()).map(|f| f.agent);
        assert_eq!(agent("agent/fix-ci"), Some(true));
        assert_eq!(agent("feature/agent/x"), Some(false));
        assert_eq!(agent("agentic"), Some(false));
    }

    #[test]
    fn pr_fact_takes_size_rounds_ci_failures_and_the_first_human_reply() {
        let events = [
            event(
                "IC_1",
                "PR_7",
                EventKind::Commented,
                Some(("ci-bot", false, true)),
                1_100,
            ),
            event(
                "IC_2",
                "PR_7",
                EventKind::Commented,
                Some(("karthik", true, false)),
                1_200,
            ),
            event(
                "RV_1",
                "PR_7",
                EventKind::Reviewed(ReviewState::ChangesRequested),
                Some(("maint", false, false)),
                1_300,
            ),
            event("ci:a", "PR_7", EventKind::CiFailed, None, 1_400),
            event("ci:b", "PR_7", EventKind::CiFailed, None, 1_500),
            event("ci:c", "PR_8", EventKind::CiFailed, None, 1_500),
        ];
        let fact = pr_fact(
            &item("karthik", "karthik/radar"),
            &details("f"),
            &events,
            &me(),
        )
        .expect("mine");
        assert!(!fact.external);
        assert_eq!(fact.first_response_at, Some(Timestamp(1_300)));
        assert_eq!(fact.first_response_login, Some(login("maint")));
        assert_eq!(fact.ci_failures, 2);
        assert_eq!(fact.changes_requested_rounds, 2);
        assert_eq!(
            (fact.additions, fact.deletions, fact.changed_files),
            (40, 2, 3)
        );
        assert_eq!(fact.source, FactSource::Live);
    }

    #[test]
    fn transition_is_none_when_nothing_changed() {
        let at = Timestamp(9);
        let state = (Stage::AwaitingReview, Side::Them);
        assert_eq!(transition("PR_1", Some(state), state, at), None);
    }

    #[test]
    fn transition_is_a_row_when_only_the_turn_changes() {
        let at = Timestamp(9);
        let row = transition(
            "PR_1",
            Some((Stage::AwaitingReview, Side::Them)),
            (Stage::AwaitingReview, Side::Me),
            at,
        )
        .expect("a row");
        assert_eq!(row.turn, Side::Me);
        assert_eq!(row.stage, Stage::AwaitingReview);
        assert_eq!(row.at, at);
    }

    #[test]
    fn transition_is_a_row_when_only_the_stage_changes() {
        let row = transition(
            "PR_1",
            Some((Stage::CiRunning, Side::Them)),
            (Stage::AwaitingReview, Side::Them),
            Timestamp(9),
        );
        assert_eq!(row.map(|r| r.stage), Some(Stage::AwaitingReview));
    }

    #[test]
    fn transition_is_a_row_for_the_first_sight() {
        let row = transition("PR_1", None, (Stage::Draft, Side::Them), Timestamp(9));
        assert_eq!(row.map(|r| r.item_node_id), Some("PR_1".to_owned()));
    }

    #[test]
    fn a_change_of_reason_inside_my_turn_is_the_same_side() {
        assert_eq!(
            Side::from(Turn::Me(Reason::CiFailed)),
            Side::from(Turn::Me(Reason::Replied))
        );
        assert_ne!(
            Side::from(Turn::Me(Reason::CiFailed)),
            Side::from(Turn::Them)
        );
    }

    #[test]
    fn issue_links_mark_an_issue_i_opened_and_one_i_commented_on() {
        let closing = [
            ClosingIssue {
                repo: repo("karthik/radar"),
                number: 3,
            },
            ClosingIssue {
                repo: repo("octo/widgets"),
                number: 4,
            },
            ClosingIssue {
                repo: repo("octo/widgets"),
                number: 5,
            },
        ];
        let known = |node: &str, r: &str, number, mine| KnownItem {
            node_id: node.to_owned(),
            repo: repo(r),
            number,
            kind: ItemKind::Issue,
            authored_by_me: mine,
        };
        let items = [
            known("I_3", "karthik/radar", 3, true),
            known("I_4", "octo/widgets", 4, false),
        ];
        let events = [
            event(
                "IC_9",
                "I_4",
                EventKind::Commented,
                Some(("karthik", true, false)),
                10,
            ),
            event(
                "IC_8",
                "I_3",
                EventKind::Commented,
                Some(("alice", false, false)),
                10,
            ),
        ];
        let links = issue_links("PR_7", &closing, &items, &events);
        let flags: Vec<(u32, bool, bool)> = links
            .iter()
            .map(|l| {
                (
                    l.issue_number,
                    l.issue_authored_by_me,
                    l.issue_commented_by_me,
                )
            })
            .collect();
        assert_eq!(
            flags,
            [(3, true, false), (4, false, true), (5, false, false)]
        );
        assert!(links.iter().all(|l| l.pr_node_id == "PR_7"));
    }
}
