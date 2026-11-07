use std::collections::HashSet;

use domain::ids::RepoName;
use domain::turn::{Reason, Turn};
use serde_json::{json, Value};

use super::*;
use crate::github::client::interpret_query;
use crate::github::{Queried, Timestamp as GithubTimestamp};

const ENRICH_20: &str = include_str!("../../tests/fixtures/enrich_20.json");
const ENRICH_EDGE: &str = include_str!("../../tests/fixtures/enrich_edge.json");
/// The instant the fixtures were generated at.
const NOW: Timestamp = Timestamp(1_794_052_800);

fn me() -> Login {
    Login::parse("karthik").expect("login")
}

fn ago(minutes: i64) -> String {
    Timestamp(NOW.0 - minutes * 60).to_string()
}

fn data(body: &str) -> EnrichData {
    let now = GithubTimestamp(u64::try_from(NOW.0).expect("positive"));
    let queried: Queried<EnrichData> =
        interpret_query(200, &|_: &str| None, body, now).expect("fixture parses");
    queried.data
}

fn comment(id: &str, minutes: i64, who: &str, typename: &str) -> Value {
    json!({"__typename": "IssueComment", "id": id, "at": ago(minutes),
        "url": format!("https://github.com/octo/widgets/pull/1#{id}"),
        "actor": {"login": who, "__typename": typename}})
}

fn review(id: &str, minutes: i64, who: &str, state: &str) -> Value {
    json!({"__typename": "PullRequestReview", "id": id, "at": ago(minutes),
        "url": format!("https://github.com/octo/widgets/pull/1#{id}"), "state": state,
        "actor": {"login": who, "__typename": "User"}})
}

struct Pr {
    author: &'static str,
    decision: Value,
    rollup: Value,
    timeline: Vec<Value>,
    latest: Vec<Value>,
}

impl Pr {
    fn by(author: &'static str, timeline: Vec<Value>) -> Self {
        Self {
            author,
            decision: Value::Null,
            rollup: json!("SUCCESS"),
            timeline,
            latest: Vec::new(),
        }
    }

    fn node(&self) -> ItemNode {
        let commits = match &self.rollup {
            Value::Null => json!([]),
            state => json!([{"commit": {"oid": "abc123", "statusCheckRollup": {"state": state}}}]),
        };
        serde_json::from_value(json!({
            "__typename": "PullRequest", "id": "PR_1", "number": 1, "title": "t",
            "url": "https://github.com/octo/widgets/pull/1", "prState": "OPEN",
            "isDraft": false, "createdAt": ago(3 * 24 * 60), "updatedAt": ago(5),
            "closedAt": null, "mergedAt": null, "headRefName": "feature/x",
            "reviewDecision": self.decision,
            "author": {"login": self.author, "__typename": "User"},
            "assignees": {"nodes": []}, "reviewRequests": {"nodes": []},
            "latestReviews": {"nodes": self.latest}, "commits": {"nodes": commits},
            "timelineItems": {"nodes": self.timeline},
        }))
        .expect("pull request node")
    }
}

fn thread(reason: &str) -> Thread {
    Thread {
        id: "8001".to_owned(),
        item: ItemRef {
            repo: RepoName::parse("octo/widgets").expect("repo"),
            number: 1,
            kind: ItemKind::Pr,
        },
        reason: reason.to_owned(),
    }
}

fn enrich(pr: &Pr, reason: &str, stored: Option<&Stored>) -> Enriched {
    let repo = RepoNode {
        item: Some(pr.node()),
    };
    match outcome(&thread(reason), Some(&repo), stored, &me(), NOW) {
        Outcome::Enriched(enriched) => *enriched,
        Outcome::Gone => panic!("item is gone"),
    }
}

fn stored(ci: Option<CiState>, stage: Option<Stage>, updated_minutes_ago: i64) -> Stored {
    Stored {
        repo: RepoName::parse("octo/widgets").expect("repo"),
        number: 1,
        node_id: "PR_1".to_owned(),
        ci_state: ci,
        stage,
        stage_since: Timestamp(NOW.0 - 7200),
        updated_at: Timestamp(NOW.0 - updated_minutes_ago * 60),
    }
}

fn ids(events: &[Event]) -> HashSet<String> {
    events.iter().map(|e| e.id.as_str().to_owned()).collect()
}

fn kinds(events: &[Event]) -> Vec<&'static str> {
    events.iter().map(|e| e.kind.tag()).collect()
}

fn threads_for(numbers: &[(u32, ItemKind, &str)]) -> Vec<Thread> {
    numbers
        .iter()
        .map(|(number, kind, reason)| Thread {
            id: format!("t{number}"),
            item: ItemRef {
                repo: RepoName::parse("octo/widgets").expect("repo"),
                number: *number,
                kind: *kind,
            },
            reason: (*reason).to_owned(),
        })
        .collect()
}

#[test]
fn the_fixture_items_get_the_turn_and_stage_the_scenario_expects() {
    let data = data(ENRICH_20);
    let numbers: Vec<(u32, ItemKind, &str)> = data
        .aliases
        .iter()
        .map(
            |(_, repo)| match repo.as_ref().and_then(|r| r.item.as_ref()).expect("item") {
                ItemNode::PullRequest(pr) => (pr.number, ItemKind::Pr, "comment"),
                ItemNode::Issue(issue) => (issue.number, ItemKind::Issue, "comment"),
            },
        )
        .collect();
    let planned = plan(&threads_for(&numbers), &[], &data, &me(), NOW).expect("plan");
    assert_eq!(planned.items.len(), 20);
    let by_number = |n: u32| {
        planned
            .items
            .iter()
            .find(|r| r.item_ref.number == n)
            .expect("row")
    };
    let expected = [
        (
            12,
            Turn::Me(Reason::ReviewRequested),
            Some(Stage::AwaitingReview),
        ),
        (
            13,
            Turn::Me(Reason::ChangesRequested),
            Some(Stage::ChangesRequested),
        ),
        (14, Turn::Me(Reason::CiFailed), Some(Stage::CiFailing)),
        (15, Turn::Them, Some(Stage::AwaitingReview)),
        (9, Turn::Me(Reason::Replied), None),
        (16, Turn::Them, Some(Stage::Draft)),
    ];
    for (number, turn, stage) in expected {
        let row = by_number(number);
        assert_eq!((row.turn, row.stage), (turn, stage), "#{number}");
    }
    assert_eq!(by_number(16).state, ItemState::Draft);
    assert_eq!(by_number(14).ci_state, Some(CiState::Failing));
    assert_eq!(by_number(12).requested_reviewers, vec![me()]);
    assert_eq!(by_number(12).head_ref.as_deref(), Some("feature/12"));
    assert_eq!(by_number(9).head_ref, None);
}

#[test]
fn event_ids_are_deterministic_and_one_per_timeline_node() {
    let pr = Pr::by(
        "karthik",
        vec![
            comment("IC_1", 300, "bob", "User"),
            review("PRR_1", 200, "bob", "COMMENTED"),
        ],
    );
    let first = enrich(&pr, "comment", None).events;
    let second = enrich(&pr, "comment", None).events;
    assert_eq!(first, second);
    assert_eq!(
        ids(&first),
        HashSet::from([
            "open:PR_1".to_owned(),
            "IC_1".to_owned(),
            "PRR_1".to_owned(),
            "ci:PR_1:abc123:passed".to_owned(),
        ])
    );
    assert_eq!(first.len(), 4);

    let mut longer = Pr::by("karthik", pr.timeline.clone());
    longer.timeline.push(comment("IC_2", 10, "bob", "User"));
    let after = ids(&enrich(&longer, "comment", None).events);
    assert_eq!(after.difference(&ids(&first)).collect::<Vec<_>>(), ["IC_2"]);
}

#[test]
fn a_second_enrich_of_a_stored_item_writes_only_events_since_the_stored_update() {
    let pr = Pr::by(
        "karthik",
        vec![
            comment("IC_old", 600, "bob", "User"),
            comment("IC_new", 20, "bob", "User"),
        ],
    );
    let known = stored(Some(CiState::Passing), Some(Stage::AwaitingReview), 60);
    let events = enrich(&pr, "comment", Some(&known)).events;
    assert_eq!(ids(&events), HashSet::from(["IC_new".to_owned()]));
}

#[test]
fn a_ci_rollup_change_is_one_event_and_no_change_is_none() {
    let pr = Pr::by("karthik", vec![]);
    let was_failing = stored(Some(CiState::Failing), Some(Stage::CiFailing), 60);
    let events = enrich(&pr, "comment", Some(&was_failing)).events;
    assert_eq!(kinds(&events), ["ci_passed"]);
    assert_eq!(
        events[0].id,
        EventId::for_ci("PR_1", "abc123", CiOutcome::Passed)
    );
    assert_eq!(events[0].actor, None);
    assert_eq!(events[0].occurred_at, NOW);

    let unchanged = stored(Some(CiState::Passing), Some(Stage::AwaitingReview), 60);
    assert!(enrich(&pr, "comment", Some(&unchanged)).events.is_empty());

    let mut failing = Pr::by("karthik", vec![]);
    failing.rollup = json!("FAILURE");
    let still = stored(Some(CiState::Failing), Some(Stage::CiFailing), 60);
    assert!(enrich(&failing, "comment", Some(&still)).events.is_empty());
    let fresh = enrich(&failing, "comment", Some(&unchanged)).events;
    assert_eq!(kinds(&fresh), ["ci_failed"]);

    let mut running = Pr::by("karthik", vec![]);
    running.rollup = json!("PENDING");
    assert!(enrich(&running, "comment", Some(&unchanged))
        .events
        .is_empty());
}

#[test]
fn the_stage_clock_restarts_on_a_ci_change_and_the_failure_keeps_its_time() {
    let mut failing = Pr::by("karthik", vec![]);
    failing.rollup = json!("FAILURE");
    let was_passing = stored(Some(CiState::Passing), Some(Stage::AwaitingReview), 60);
    let changed = enrich(&failing, "ci_activity", Some(&was_passing));
    assert_eq!(changed.row.stage, Some(Stage::CiFailing));
    assert_eq!(changed.row.stage_since, NOW);
    assert_eq!(changed.row.turn, Turn::Me(Reason::CiFailed));
    assert_eq!(changed.row.since, NOW);

    let was_failing = stored(Some(CiState::Failing), Some(Stage::CiFailing), 60);
    let again = enrich(&failing, "ci_activity", Some(&was_failing));
    assert_eq!(again.row.since, was_failing.stage_since);
}

#[test]
fn a_mention_reason_marks_the_newest_human_comment_after_mine_and_never_a_bot() {
    let pr = Pr::by(
        "alice",
        vec![
            comment("IC_a", 500, "alice", "User"),
            comment("IC_me", 400, "karthik", "User"),
            comment("IC_b", 300, "bob", "User"),
            comment("IC_c", 200, "carol", "User"),
            comment("IC_bot", 100, "dependabot", "Bot"),
        ],
    );
    let mention = enrich(&pr, "mention", None);
    let mentioned: Vec<&Event> = mention
        .events
        .iter()
        .filter(|e| e.kind == EventKind::Mentioned)
        .collect();
    assert_eq!(mentioned.len(), 1);
    assert_eq!(mentioned[0].id, EventId::for_mention("IC_c"));
    assert_eq!(
        mentioned[0].actor.as_ref().map(|a| a.login.as_str()),
        Some("carol")
    );
    assert_eq!(mention.row.turn, Turn::Me(Reason::Mentioned));
    assert_eq!(
        mention.row.last_human.as_ref().and_then(|h| h.url.clone()),
        Some("https://github.com/octo/widgets/pull/1#IC_c".to_owned())
    );

    let team = enrich(&pr, "team_mention", None);
    let marks = kinds(&team.events)
        .iter()
        .filter(|k| **k == "mentioned")
        .count();
    assert_eq!(marks, 1);
    let plain = enrich(&pr, "comment", None);
    assert!(!kinds(&plain.events).contains(&"mentioned"));
    assert_eq!(plain.row.turn, Turn::Them);

    let only_bot = Pr::by("alice", vec![comment("IC_bot", 100, "dependabot", "Bot")]);
    let none = enrich(&only_bot, "mention", None);
    assert!(!kinds(&none.events).contains(&"mentioned"));
}

#[test]
fn a_bot_comment_after_mine_leaves_the_turn_with_them() {
    let pr = Pr::by(
        "karthik",
        vec![
            comment("IC_me", 180, "karthik", "User"),
            comment("IC_bot", 60, "dependabot", "Bot"),
        ],
    );
    let done = enrich(&pr, "comment", None);
    assert_eq!(done.row.turn, Turn::Them);
    assert_eq!(done.row.since, Timestamp(NOW.0 - 180 * 60));
    let bot = done
        .events
        .iter()
        .find(|e| e.id == EventId::for_node("IC_bot"))
        .expect("the bot comment is an event");
    assert!(bot.actor.as_ref().is_some_and(|a| a.is_bot && !a.is_me));
}

#[test]
fn an_empty_review_decision_takes_the_stage_from_the_latest_review_per_reviewer() {
    let approved_later = Pr::by(
        "karthik",
        vec![
            review("PRR_1", 500, "bob", "CHANGES_REQUESTED"),
            review("PRR_2", 100, "bob", "APPROVED"),
        ],
    );
    assert_eq!(
        enrich(&approved_later, "comment", None).row.stage,
        Some(Stage::Approved)
    );

    let mut from_latest = Pr::by("karthik", vec![]);
    from_latest.latest = vec![json!({
        "id": "PRR_9", "state": "CHANGES_REQUESTED", "at": ago(400),
        "url": "https://github.com/octo/widgets/pull/1#PRR_9",
        "author": {"login": "bob", "__typename": "User"}
    })];
    let row = enrich(&from_latest, "comment", None);
    assert_eq!(row.row.stage, Some(Stage::ChangesRequested));
    assert_eq!(row.row.turn, Turn::Me(Reason::ChangesRequested));
    assert!(
        !row.events
            .iter()
            .any(|e| e.id == EventId::for_node("PRR_9")),
        "a review outside the timeline window is not a new event"
    );
}

#[test]
fn the_assign_reason_counts_even_before_the_item_lists_me() {
    let issue: ItemNode = serde_json::from_value(json!({
        "__typename": "Issue", "id": "I_1", "number": 1, "title": "t",
        "url": "https://github.com/octo/widgets/issues/1", "issueState": "OPEN",
        "createdAt": ago(1000), "updatedAt": ago(10), "closedAt": null,
        "author": {"login": "alice", "__typename": "User"},
        "assignees": {"nodes": []}, "timelineItems": {"nodes": []}
    }))
    .expect("issue");
    let repo = RepoNode { item: Some(issue) };
    let mut assigned = thread("assign");
    assigned.item.kind = ItemKind::Issue;
    match outcome(&assigned, Some(&repo), None, &me(), NOW) {
        Outcome::Enriched(e) => assert_eq!(e.row.turn, Turn::Me(Reason::Assigned)),
        Outcome::Gone => panic!("gone"),
    }
    match outcome(&thread("comment"), Some(&repo), None, &me(), NOW) {
        Outcome::Enriched(e) => assert_eq!(e.row.turn, Turn::Them),
        Outcome::Gone => panic!("gone"),
    }
}

#[test]
fn the_edge_response_has_a_gone_alias_a_ghost_and_a_bot() {
    let data = data(ENRICH_EDGE);
    let threads = threads_for(&[
        (41, ItemKind::Pr, "comment"),
        (42, ItemKind::Pr, "comment"),
        (43, ItemKind::Issue, "assign"),
    ]);
    let planned = plan(&threads, &[], &data, &me(), NOW).expect("plan");
    assert_eq!(planned.gone, ["t42"]);
    assert_eq!(planned.items.len(), 2);
    let ghost = &planned.items[0];
    assert_eq!(ghost.author.login.as_str(), "ghost");
    assert_eq!(ghost.author.kind, ActorKind::User);
    assert_eq!(ghost.turn, Turn::Them);
    assert!(planned
        .events
        .iter()
        .any(|e| e.actor.as_ref().is_some_and(|a| a.is_bot)));
    assert_eq!(planned.items[1].turn, Turn::Me(Reason::Assigned));
}

#[test]
fn a_null_item_is_gone_and_so_is_a_null_alias() {
    let t = thread("comment");
    assert_eq!(outcome(&t, None, None, &me(), NOW), Outcome::Gone);
    let empty = RepoNode { item: None };
    assert_eq!(outcome(&t, Some(&empty), None, &me(), NOW), Outcome::Gone);
}

#[test]
fn a_response_that_skips_an_alias_writes_nothing() {
    let data = data(ENRICH_EDGE);
    let threads = threads_for(&[
        (41, ItemKind::Pr, "comment"),
        (42, ItemKind::Pr, "comment"),
        (43, ItemKind::Pr, "comment"),
        (44, ItemKind::Pr, "comment"),
    ]);
    assert_eq!(plan(&threads, &[], &data, &me(), NOW), Err(MissingAlias(3)));
}

#[test]
fn the_latest_url_is_the_newest_node_with_a_link_and_falls_back_to_the_item() {
    let pr = Pr::by(
        "karthik",
        vec![
            comment("IC_1", 300, "bob", "User"),
            comment("IC_2", 20, "bob", "User"),
        ],
    );
    assert_eq!(
        enrich(&pr, "comment", None).row.latest_url,
        "https://github.com/octo/widgets/pull/1#IC_2"
    );
    let quiet = Pr::by("karthik", vec![]);
    assert_eq!(
        enrich(&quiet, "comment", None).row.latest_url,
        "https://github.com/octo/widgets/pull/1"
    );
}

#[test]
fn a_pull_request_with_no_rollup_has_no_ci_and_no_ci_event() {
    let mut pr = Pr::by("karthik", vec![]);
    pr.rollup = Value::Null;
    let done = enrich(&pr, "comment", None);
    assert_eq!(done.row.ci_state, None);
    assert_eq!(kinds(&done.events), ["opened"]);
}

#[test]
fn a_bad_login_in_the_response_becomes_the_ghost() {
    let pr = Pr::by("not a login!", vec![comment("IC_1", 10, "bo b", "User")]);
    let done = enrich(&pr, "comment", None);
    assert_eq!(done.row.author.login.as_str(), "ghost");
    assert!(done
        .events
        .iter()
        .all(|e| e.actor.as_ref().is_none_or(|a| a.login.as_str() == "ghost")));
}
