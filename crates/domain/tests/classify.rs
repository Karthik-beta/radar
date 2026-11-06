use std::collections::HashSet;
use std::path::PathBuf;

use domain::activity::{Activity, ActivityKind, Actor, ActorKind, ReviewState};
use domain::classify::classify;
use domain::ids::{Login, Timestamp};
use domain::item::Item;
use domain::stage::Stage;
use domain::turn::{Reason, Turn};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    me: Login,
    item: Item,
    expect: Expect,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    turn: Turn,
    stage: Option<Stage>,
    since: Timestamp,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load(name: &str) -> Fixture {
    let path = fixtures_dir().join(format!("{name}.json"));
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("{name}.json: {err}"))
}

fn check(name: &str) {
    let fixture = load(name);
    let got = classify(&fixture.item, &fixture.me);
    let expect = fixture.expect;
    assert_eq!(
        (got.turn, got.stage, got.since),
        (expect.turn, expect.stage, expect.since),
        "{name}.json: (turn, stage, since) differ"
    );
}

macro_rules! named_cases {
    ($($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                check(stringify!($name));
            }
        )*
        const NAMED: &[&str] = &[$(stringify!($name)),*];
    };
}

named_cases!(
    reason_review_requested,
    reason_changes_requested,
    reason_ci_failed,
    reason_mentioned,
    reason_assigned,
    reason_replied,
    stage_draft,
    stage_ci_running,
    stage_ci_failing,
    stage_awaiting_review,
    stage_changes_requested,
    stage_approved,
    stage_merged,
    stage_closed,
    rule_bot_comment_after_mine,
    rule_bot_review_ignored,
    rule_bot_mention_ignored,
    rule_empty_decision_latest_per_reviewer,
    rule_empty_decision_second_reviewer_blocks,
    rule_dismissed_review_ignored,
    rule_commented_review_keeps_approval,
    rule_push_answers_changes_requested,
    rule_comment_does_not_answer_changes_requested,
    rule_reply_answers_mention,
    rule_same_second_does_not_answer,
    rule_draft_ci_failure_stays_them,
    rule_third_party_thread_stays_them,
    rule_author_case_insensitive,
    rule_new_own_pr_waits_on_them,
    rule_issue_replied,
    rule_issue_assigned_to_me,
    rule_precedence_changes_over_ci,
    rule_since_is_oldest_trigger,
    rule_review_request_on_draft_ignored,
);

#[test]
fn every_fixture_file_is_a_named_case() {
    let mut on_disk: Vec<String> = std::fs::read_dir(fixtures_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut named: Vec<String> = NAMED.iter().map(|n| (*n).to_owned()).collect();
    named.sort();
    assert_eq!(on_disk, named);
}

#[test]
fn every_reason_and_stage_is_expected_by_a_fixture() {
    let expects: Vec<Expect> = NAMED.iter().map(|name| load(name).expect).collect();
    let reasons: HashSet<Reason> = expects
        .iter()
        .filter_map(|e| match e.turn {
            Turn::Me(reason) => Some(reason),
            Turn::Them => None,
        })
        .collect();
    let stages: HashSet<Stage> = expects.iter().filter_map(|e| e.stage).collect();
    for reason in Reason::PRECEDENCE {
        assert!(reasons.contains(&reason), "no fixture expects {reason:?}");
    }
    for stage in Stage::ALL {
        assert!(stages.contains(&stage), "no fixture expects {stage:?}");
    }
}

#[test]
fn bot_activity_never_changes_classification() {
    for name in NAMED {
        let fixture = load(name);
        let before = classify(&fixture.item, &fixture.me);

        let item = &fixture.item;
        let newest = [Some(item.created_at), Some(item.updated_at), item.closed_at]
            .into_iter()
            .flatten()
            .chain(item.ci.map(|ci| ci.at))
            .chain(item.activity.iter().map(|a| a.at))
            .max()
            .unwrap();
        let bot = Actor {
            login: Login::parse("github-actions").unwrap(),
            kind: ActorKind::Bot,
        };
        let mut with_bots = item.clone();
        let kinds = [
            ActivityKind::Comment { mentions_me: false },
            ActivityKind::Review {
                state: ReviewState::ChangesRequested,
            },
            ActivityKind::Comment { mentions_me: true },
            ActivityKind::Pushed,
        ];
        for (offset, kind) in (1..).zip(kinds) {
            with_bots.activity.push(Activity {
                at: Timestamp(newest.0 + 60 * offset),
                actor: bot.clone(),
                kind,
            });
        }

        let after = classify(&with_bots, &fixture.me);
        assert_eq!(
            before, after,
            "{name}.json: bot activity changed the result"
        );
    }
}
