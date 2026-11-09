use std::collections::HashSet;

use domain::turn::{Reason, Turn};
use serde_json::Value;

use super::*;
use crate::github::client::interpret_query;
use crate::github::{Queried, Timestamp as GithubTimestamp};

const PAGE: &str = include_str!("../../tests/fixtures/search_page.json");
const LAST_PAGE: &str = include_str!("../../tests/fixtures/search_last_page.json");
/// The instant the fixtures were generated at, 2026-11-07T12:00:00Z.
const NOW: Timestamp = Timestamp(1_794_052_800);

fn me() -> Login {
    Login::parse("karthik").expect("login")
}

fn data(body: &str) -> SearchData {
    let now = GithubTimestamp(u64::try_from(NOW.0).expect("positive"));
    let queried: Queried<SearchData> =
        interpret_query(200, &|_: &str| None, body, now).expect("fixture parses");
    queried.data
}

fn cursor(kind: SearchKind, page: u32, after: Option<&str>) -> BackfillCursor {
    BackfillCursor {
        window: SearchWindow::Backfill90d,
        since: SearchWindow::Backfill90d.since(NOW),
        kind,
        page,
        after: after.map(str::to_owned),
    }
}

fn nodes(data: &SearchData) -> Vec<&ItemNode> {
    data.search.nodes.iter().flatten().collect()
}

#[test]
fn a_page_with_more_yields_the_same_kind_at_the_new_cursor() {
    let here = cursor(SearchKind::Authored, 1, None);
    assert_eq!(
        next_step(&here, Some("Y3Vyc29yOjEw".into())),
        Step::Page(cursor(SearchKind::Authored, 2, Some("Y3Vyc29yOjEw")))
    );
    let later = cursor(SearchKind::Involves, 4, Some("Y3Vyc29yOjMw"));
    assert_eq!(
        next_step(&later, Some("Y3Vyc29yOjQw".into())),
        Step::Page(cursor(SearchKind::Involves, 5, Some("Y3Vyc29yOjQw")))
    );
}

#[test]
fn the_last_page_of_each_kind_starts_the_next_kind_and_involves_ends() {
    assert_eq!(
        next_step(
            &cursor(SearchKind::Authored, 15, Some("Y3Vyc29yOjE0MA==")),
            None
        ),
        Step::Page(cursor(SearchKind::AuthoredIssues, 1, None))
    );
    assert_eq!(
        next_step(
            &cursor(SearchKind::AuthoredIssues, 2, Some("Y3Vyc29yOjEw")),
            None
        ),
        Step::Page(cursor(SearchKind::Involves, 1, None))
    );
    assert_eq!(
        next_step(&cursor(SearchKind::Involves, 6, Some("Y3Vyc29yOjUw")), None),
        Step::Done
    );
}

#[test]
fn the_nightly_window_runs_the_same_three_searches() {
    let mut here = BackfillCursor::first(SearchWindow::Nightly7d, NOW);
    let mut kinds = vec![here.kind];
    while let Step::Page(next) = next_step(&here, None) {
        assert_eq!(next.window, SearchWindow::Nightly7d);
        kinds.push(next.kind);
        here = next;
    }
    assert_eq!(
        kinds,
        [
            SearchKind::Authored,
            SearchKind::AuthoredIssues,
            SearchKind::Involves
        ]
    );
}

#[test]
fn each_window_reaches_back_its_own_number_of_days() {
    assert_eq!(SearchWindow::Backfill90d.since(NOW), "2026-08-09");
    assert_eq!(SearchWindow::Nightly7d.since(NOW), "2026-10-31");
    assert_eq!(
        SearchWindow::Backfill90d.since(Timestamp(86_400 * 3)),
        "1969-10-06"
    );
}

#[test]
fn a_query_is_the_kind_terms_and_the_date() {
    let query = |kind, window| {
        BackfillCursor {
            kind,
            ..BackfillCursor::first(window, NOW)
        }
        .query()
    };
    assert_eq!(
        query(SearchKind::Authored, SearchWindow::Backfill90d),
        "is:pr author:@me updated:>=2026-08-09"
    );
    assert_eq!(
        query(SearchKind::AuthoredIssues, SearchWindow::Backfill90d),
        "is:issue author:@me updated:>=2026-08-09"
    );
    assert_eq!(
        query(SearchKind::Involves, SearchWindow::Nightly7d),
        "involves:@me updated:>=2026-10-31"
    );
}

#[test]
fn a_cursor_round_trips_through_its_row_key() {
    for original in [
        BackfillCursor::first(SearchWindow::Backfill90d, NOW),
        BackfillCursor::first(SearchWindow::Nightly7d, NOW),
        cursor(SearchKind::Involves, 12, Some("Y3Vyc29yOjEyMA==")),
    ] {
        assert_eq!(BackfillCursor::parse(&original.key()), Some(original));
    }
    assert_eq!(
        BackfillCursor::first(SearchWindow::Backfill90d, NOW).key(),
        "b90.2026-08-09.authored.1.start"
    );
}

#[test]
fn keys_that_are_not_cursors_are_rejected() {
    for bad in [
        "",
        "start",
        "b90.authored.0.start",
        "b90.authored.x.start",
        "b30.authored.1.start",
        "b90.repos.1.start",
        "b90.authored.1.",
        "b90.authored.1.not a cursor",
        "b90.authored.1.start.extra",
        "b90.authored.1",
    ] {
        assert_eq!(BackfillCursor::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn the_fixtures_parse_as_a_full_page_and_a_last_page() {
    let page = data(PAGE);
    assert_eq!(nodes(&page).len(), ENRICH_BATCH);
    assert!(page.search.page_info.has_next_page);
    assert!(page.search.page_info.end_cursor.is_some());
    let last = data(LAST_PAGE);
    assert_eq!(nodes(&last).len(), 2);
    assert!(!last.search.page_info.has_next_page);
}

#[test]
fn next_cursor_trusts_only_a_new_well_formed_cursor() {
    let here = cursor(SearchKind::Authored, 2, Some("Y3Vyc29yOjEw"));
    let info = |end: Option<&str>, more: bool| PageInfo {
        end_cursor: end.map(str::to_owned),
        has_next_page: more,
    };
    assert_eq!(
        next_cursor(&here, &info(Some("Y3Vyc29yOjIw"), true)),
        Ok(Some("Y3Vyc29yOjIw".into()))
    );
    assert_eq!(
        next_cursor(&here, &info(Some("Y3Vyc29yOjIw"), false)),
        Ok(None)
    );
    assert_eq!(next_cursor(&here, &info(None, false)), Ok(None));
    assert!(next_cursor(&here, &info(None, true)).is_err());
    assert!(next_cursor(&here, &info(Some("Y3Vyc29yOjEw"), true)).is_err());
    assert!(next_cursor(&here, &info(Some("a b"), true)).is_err());
    assert!(next_cursor(&here, &info(Some(""), true)).is_err());
}

#[test]
fn a_node_url_names_its_repository() {
    let repo = |url| repo_of(url).map(|r| r.to_string());
    assert_eq!(
        repo("https://github.com/octo/widgets/pull/12"),
        Some("octo/widgets".into())
    );
    assert_eq!(
        repo("https://github.com/octo/widgets/issues/9"),
        Some("octo/widgets".into())
    );
    assert_eq!(repo("https://github.com/octo/widgets/discussions/9"), None);
    assert_eq!(repo("https://example.com/octo/widgets/pull/12"), None);
    assert_eq!(repo("https://github.com/octo"), None);
}

#[test]
fn every_node_of_a_page_becomes_an_item_with_its_events() {
    let page = data(PAGE);
    let planned = plan_page(&nodes(&page), &[], &[], &me(), NOW);
    assert_eq!(planned.items.len(), ENRICH_BATCH);
    assert!(planned.skipped.is_empty());
    let ids: HashSet<String> = planned
        .events
        .iter()
        .map(|e| format!("{:?}", e.id))
        .collect();
    assert_eq!(ids.len(), planned.events.len());
    assert!(planned.events.len() >= ENRICH_BATCH * 2);
}

#[test]
fn a_node_found_twice_writes_the_same_events() {
    let page = data(PAGE);
    let first = plan_page(&nodes(&page), &[], &[], &me(), NOW);
    let second = plan_page(&nodes(&page), &[], &[], &me(), NOW);
    assert_eq!(first.events, second.events);
    assert_eq!(first.items, second.items);
}

#[test]
fn a_node_with_a_foreign_url_is_skipped_and_the_rest_are_kept() {
    let mut body: Value = serde_json::from_str(PAGE).expect("json");
    body["data"]["search"]["nodes"][0]["url"] =
        Value::from("https://example.com/octo/widgets/pull/1");
    let page = data(&body.to_string());
    let planned = plan_page(&nodes(&page), &[], &[], &me(), NOW);
    assert_eq!(planned.items.len(), ENRICH_BATCH - 1);
    assert_eq!(planned.skipped.len(), 1);
}

#[test]
fn an_unknown_item_type_fails_the_whole_page() {
    let mut body: Value = serde_json::from_str(PAGE).expect("json");
    body["data"]["search"]["nodes"][0] =
        serde_json::json!({"__typename": "Discussion", "id": "D_1"});
    let now = GithubTimestamp(u64::try_from(NOW.0).expect("positive"));
    let parsed = interpret_query::<SearchData>(200, &|_: &str| None, &body.to_string(), now);
    assert!(parsed.is_err());
}

#[test]
fn a_search_keeps_the_mention_its_notification_thread_reported() {
    let page = data(PAGE);
    let nodes = nodes(&page);
    let reasons: Vec<ThreadReason> = nodes
        .iter()
        .map(|n| ThreadReason {
            item_node_id: n.id().to_owned(),
            reason: "mention".to_owned(),
        })
        .collect();
    let mentioned = |items: &[ItemRow]| {
        items
            .iter()
            .filter(|i| i.turn == Turn::Me(Reason::Mentioned))
            .count()
    };
    let without = plan_page(&nodes, &[], &[], &me(), NOW);
    let with = plan_page(&nodes, &[], &reasons, &me(), NOW);
    assert_eq!(mentioned(&without.items), 0);
    assert!(
        mentioned(&with.items) > 0,
        "a stored mention reason survives the search"
    );
}

#[test]
fn every_later_page_keeps_the_cutoff_its_window_started_with() {
    let start = BackfillCursor::first(SearchWindow::Backfill90d, NOW);
    let Step::Page(next) = next_step(&start, Some("Y3I=".into())) else {
        panic!("a next page");
    };
    let Step::Page(issues) = next_step(&next, None) else {
        panic!("the next kind");
    };
    assert_eq!(next.since, start.since);
    assert_eq!(issues.since, start.since);
    assert_eq!(
        BackfillCursor::parse(&issues.key()).map(|c| c.query()),
        Some("is:issue author:@me updated:>=2026-08-09".to_owned()),
        "a page resumed on any later day sends the same date"
    );
}

#[test]
fn a_key_with_a_bad_cutoff_is_rejected() {
    for key in [
        "b90.2026-8-09.authored.1.start",
        "b90.20260809xx.authored.1.start",
        "b90.authored.1.start",
    ] {
        assert_eq!(BackfillCursor::parse(key), None, "{key}");
    }
}
