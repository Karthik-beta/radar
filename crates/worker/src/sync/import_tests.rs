use domain::rollup::WeekStart;
use serde::de::DeserializeOwned;
use serde_json::json;

use super::*;
use crate::db::testing::{count_rows, exec, memory_db, query_json};
use crate::sync::scheduler::kick;
use crate::sync::work::{KickReason, RowState};

const NOW: Timestamp = Timestamp(1_794_744_000);

fn at(s: &str) -> Timestamp {
    Timestamp::parse_rfc3339(s).expect("timestamp")
}

fn day(s: &str) -> Day {
    Day::parse(s).expect("day")
}

fn me() -> Login {
    Login::parse("karthik").expect("login")
}

fn repo(s: &str) -> RepoName {
    RepoName::parse(s).expect("repo")
}

fn parse<T: DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("shape")
}

fn page_info(next: Option<&str>) -> serde_json::Value {
    json!({ "endCursor": next, "hasNextPage": next.is_some() })
}

fn entry(who: &str, kind: &str, created: &str, submitted: Option<&str>) -> serde_json::Value {
    json!({
        "createdAt": created,
        "submittedAt": submitted,
        "author": { "__typename": kind, "login": who },
    })
}

fn pr_node(id: &str, name: &str, timeline: serde_json::Value) -> serde_json::Value {
    json!({
        "id": id,
        "number": 7,
        "createdAt": "2025-03-01T10:00:00Z",
        "mergedAt": "2025-03-04T10:00:00Z",
        "closedAt": "2025-03-04T10:00:00Z",
        "additions": 40,
        "deletions": 2,
        "changedFiles": 3,
        "headRefName": "agent/fix",
        "repository": {
            "id": format!("R_{}", name.replace('/', "_")),
            "nameWithOwner": name,
            "stargazerCount": 120,
            "primaryLanguage": { "name": "Rust" },
        },
        "closingIssuesReferences": { "nodes": [
            { "number": 3, "repository": { "nameWithOwner": name } },
        ] },
        "reviews": { "totalCount": 2 },
        "timelineItems": timeline,
    })
}

fn search(count: u32, nodes: Vec<serde_json::Value>, next: Option<&str>) -> Page {
    parse(json!({ "issueCount": count, "nodes": nodes, "pageInfo": page_info(next) }))
}

fn range_of(kind: &WorkKind) -> Option<(String, String, Option<String>)> {
    match kind {
        WorkKind::ImportFacts(ImportKey::SearchRange { from, to, after }) => {
            Some((from.date(), to.date(), after.clone()))
        }
        _ => None,
    }
}

#[test]
fn over_limit_range_splits_in_two() {
    let (from, to) = (day("2025-01-01"), day("2025-12-31"));
    let nodes = vec![pr_node(
        "PR_1",
        "octo/widgets",
        json!({ "nodes": [], "pageInfo": page_info(None) }),
    )];
    let over = search(1_500, nodes.clone(), Some("Y3I6NTA="));
    let plan = plan_range(from, to, None, &over, &me(), NOW).expect("plan");
    assert!(plan.facts.prs.is_empty() && plan.repos.is_empty());
    let halves: Vec<_> = plan.follow_ups.iter().filter_map(range_of).collect();
    assert_eq!(
        halves,
        [
            ("2025-01-01".into(), "2025-07-01".into(), None),
            ("2025-07-02".into(), "2025-12-31".into(), None),
        ]
    );
    let at_limit = search(1_000, nodes.clone(), None);
    let plan = plan_range(from, to, None, &at_limit, &me(), NOW).expect("plan");
    assert_eq!(plan.facts.prs.len(), 1);
    let paging = search(1_500, nodes.clone(), None);
    let plan = plan_range(from, to, Some("Y3I6NTA="), &paging, &me(), NOW).expect("plan");
    assert_eq!(
        plan.facts.prs.len(),
        1,
        "a later page of a range is never split again"
    );
    let one_day = day("2025-04-01");
    let plan = plan_range(one_day, one_day, None, &over, &me(), NOW).expect("plan");
    assert!(
        plan.facts.prs.len() == 1,
        "a single day cannot split, so it reads what search returns"
    );
}

#[test]
fn split_ranges_cover_the_year_without_overlap() {
    fn leaves(from: Day, to: Day, out: &mut Vec<(Day, Day)>) {
        match halves(from, to) {
            Some([a, b]) => {
                assert!(a.0.days_through(a.1) >= 1 && b.0.days_through(b.1) >= 1);
                leaves(a.0, a.1, out);
                leaves(b.0, b.1, out);
            }
            None => out.push((from, to)),
        }
    }
    for year in [2024, 2025] {
        let (from, to) = (
            Day::year_start(year).expect("start"),
            Day::year_end(year).expect("end"),
        );
        let mut out = Vec::new();
        leaves(from, to, &mut out);
        assert_eq!(out.len() as i64, from.days_through(to));
        assert_eq!(out[0].0, from);
        assert_eq!(out.last().map(|r| r.1), Some(to));
        for pair in out.windows(2) {
            assert_eq!(pair[0].1.plus_days(1), pair[1].0, "no gap and no overlap");
        }
        let [a, b] = halves(from, to).expect("a year splits");
        assert_eq!(a.1.plus_days(1), b.0);
        assert!((a.0.days_through(a.1) - b.0.days_through(b.1)).abs() <= 1);
    }
}

#[test]
fn one_range_and_one_reviewed_list_per_year() {
    let units = year_units(at("2024-03-10T08:00:00Z"), NOW);
    let ranges: Vec<_> = units.iter().filter_map(range_of).collect();
    assert_eq!(
        ranges,
        [
            ("2024-01-01".into(), "2024-12-31".into(), None),
            ("2025-01-01".into(), "2025-12-31".into(), None),
            ("2026-01-01".into(), "2026-12-31".into(), None),
        ]
    );
    let years: Vec<i32> = units
        .iter()
        .filter_map(|u| match u {
            WorkKind::ImportFacts(ImportKey::ReviewedPrs { year, after: None }) => Some(*year),
            _ => None,
        })
        .collect();
    assert_eq!(years, [2024, 2025, 2026]);
    assert_eq!(units.len(), 6);
    assert_eq!(year_units(at("2030-01-01T00:00:00Z"), NOW).len(), 2);
}

fn import_row(key: ImportKey, state: RowState) -> WorkRow {
    WorkRow {
        state,
        ..WorkRow::pending(WorkKind::ImportFacts(key), NOW)
    }
}

fn timeline_key(after: &str) -> ImportKey {
    ImportKey::Timeline {
        pr: NodeId::parse("PR_1").expect("id"),
        after: after.to_owned(),
    }
}

#[test]
fn marker_waits_for_every_import_row() {
    let running = WorkKind::ImportFacts(timeline_key("Y3I6MjA="));
    let alone = [import_row(timeline_key("Y3I6MjA="), RowState::Pending)];
    assert!(!rows_left(&alone, &running));
    assert!(finishes(&[], rows_left(&alone, &running)));

    let pending = [
        alone[0].clone(),
        import_row(ImportKey::Start, RowState::Pending),
    ];
    assert!(rows_left(&pending, &running));
    let dead = [
        alone[0].clone(),
        import_row(timeline_key("Y3I6NDA="), RowState::Dead),
    ];
    assert!(
        rows_left(&dead, &running),
        "a dead import row blocks the marker"
    );

    let follow_up = WorkKind::ImportFacts(timeline_key("Y3I6NDA="));
    assert!(!finishes(&[follow_up], false));
    let others = [
        WorkKind::SendAlerts,
        WorkKind::RefreshRepos(RepoPage::first(WeekStart::containing(NOW))),
    ];
    assert!(finishes(&others, false));
    let not_import = [WorkRow::pending(others[1].clone(), NOW)];
    assert!(!rows_left(&not_import, &running));
}

fn started_import(rows: &[WorkRow]) -> bool {
    rows.iter()
        .any(|r| r.kind == WorkKind::ImportFacts(ImportKey::Start))
}

fn refreshes(rows: &[WorkRow]) -> usize {
    rows.iter()
        .filter(|r| matches!(r.kind, WorkKind::RefreshRepos(_)))
        .count()
}

fn imported_state() -> SyncState {
    SyncState {
        backfill_done_at: Some(NOW),
        history_seeded_at: Some(NOW),
        ..SyncState::default()
    }
}

#[test]
fn no_enqueue_while_an_import_row_remains() {
    let mut state = imported_state();
    let waiting = [import_row(timeline_key("Y3I6MjA="), RowState::Pending)];
    let queued = kick(KickReason::Rollup, &waiting, &mut state, NOW);
    assert!(!started_import(&queued));
    assert!(queued.iter().all(|r| !r.kind.is_import()));

    let dead = [import_row(timeline_key("Y3I6MjA="), RowState::Dead)];
    let queued = kick(KickReason::Rollup, &dead, &mut state, NOW);
    assert!(!started_import(&queued));
    let revived: Vec<_> = queued.iter().filter(|r| r.kind.is_import()).collect();
    assert_eq!(revived.len(), 1);
    assert_eq!(revived[0].kind, dead[0].kind);
    assert_eq!(revived[0].attempts, 0);

    for reason in [
        KickReason::SignedIn,
        KickReason::Reconcile,
        KickReason::Email,
    ] {
        let queued = kick(reason, &[], &mut imported_state(), NOW);
        assert!(!started_import(&queued), "{reason:?}");
        assert_eq!(refreshes(&queued), 0, "{reason:?}");
    }
}

#[test]
fn a_second_night_enqueues_nothing() {
    let tuesday = at("2026-11-10T03:00:00Z");
    let mut state = imported_state();
    let first = kick(KickReason::Rollup, &[], &mut state, tuesday);
    assert!(started_import(&first));
    assert_eq!(refreshes(&first), 1);
    assert_eq!(
        state.repos_refreshed_week,
        Some(WeekStart::containing(tuesday))
    );

    state.facts_imported_at = Some(tuesday);
    let next_night = Timestamp(tuesday.0 + 86_400);
    let second = kick(KickReason::Rollup, &[], &mut state, next_night);
    assert!(second.iter().all(|r| !r.kind.is_paced()), "{second:?}");

    let next_week = Timestamp(tuesday.0 + 7 * 86_400);
    let third = kick(KickReason::Rollup, &[], &mut state, next_week);
    assert!(!started_import(&third));
    assert_eq!(refreshes(&third), 1);
}

fn timeline_page(entries: Vec<serde_json::Value>, next: Option<&str>) -> Page {
    parse(json!({ "nodes": entries, "pageInfo": page_info(next) }))
}

#[test]
fn timeline_pages_until_a_human_answers() {
    let created = at("2025-03-01T10:00:00Z");
    let widgets = repo("octo/widgets");
    let step = |page: &Page, after: Option<&str>| {
        timeline_step("PR_1", &widgets, created, page, after, &me())
    };
    let first = timeline_page(
        vec![
            entry("ci-bot", "Bot", "2025-03-01T10:05:00Z", None),
            entry("karthik", "User", "2025-03-01T11:00:00Z", None),
        ],
        Some("dGw6MjA="),
    );
    assert_eq!(
        step(&first, None),
        Ok(TimelineStep::Next("dGw6MjA=".into()))
    );
    let second = timeline_page(
        vec![
            entry("renovate", "Bot", "2025-03-02T10:00:00Z", None),
            entry(
                "karthik",
                "User",
                "2025-03-02T11:00:00Z",
                Some("2025-03-02T11:00:00Z"),
            ),
        ],
        Some("dGw6NDA="),
    );
    assert_eq!(
        step(&second, Some("dGw6MjA=")),
        Ok(TimelineStep::Next("dGw6NDA=".into()))
    );
    let third = timeline_page(
        vec![
            entry(
                "maint",
                "User",
                "2025-03-05T09:00:00Z",
                Some("2025-03-05T09:30:00Z"),
            ),
            entry("other", "User", "2025-03-06T09:00:00Z", None),
        ],
        Some("dGw6NjA="),
    );
    assert_eq!(
        step(&third, Some("dGw6NDA=")),
        Ok(TimelineStep::Answered(
            at("2025-03-05T09:30:00Z"),
            Login::parse("maint").expect("login")
        ))
    );
    let empty_end = timeline_page(vec![], None);
    assert_eq!(step(&empty_end, Some("dGw6NjA=")), Ok(TimelineStep::End));
    assert!(
        step(&first, Some("dGw6MjA=")).is_err(),
        "a repeated cursor stops the chain"
    );

    let conn = memory_db();
    let imported = PrFact {
        node_id: "PR_1".into(),
        repo: widgets.clone(),
        external: true,
        number: 7,
        created_at: created,
        merged_at: None,
        closed_at: None,
        first_response_at: None,
        first_response_login: None,
        changes_requested_rounds: 0,
        ci_failures: 0,
        additions: 1,
        deletions: 1,
        changed_files: 1,
        agent: false,
        source: FactSource::Imported,
    };
    exec(&conn, &facts::upsert_prs(&[imported])).expect("imported");
    let maint = Login::parse("maint").expect("login");
    exec(
        &conn,
        &facts::set_imported_first_response("PR_1", at("2025-03-05T09:30:00Z"), &maint),
    )
    .expect("answered");
    let rows = query_json(
        &conn,
        "SELECT first_response_at, first_response_login FROM pr_facts",
        &[],
    );
    assert_eq!(rows[0]["first_response_at"], at("2025-03-05T09:30:00Z").0);
    assert_eq!(rows[0]["first_response_login"], "maint");
}

#[test]
fn a_range_page_writes_imported_facts_and_live_rows_win() {
    let answered = json!({
        "nodes": [
            entry("ci-bot", "Bot", "2025-03-01T10:05:00Z", None),
            entry("maint", "User", "2025-03-02T10:00:00Z", None),
        ],
        "pageInfo": page_info(Some("dGw6MjA=")),
    });
    let silent = json!({
        "nodes": [entry("karthik", "User", "2025-03-01T11:00:00Z", None)],
        "pageInfo": page_info(Some("dGw6MjA=")),
    });
    let page = search(
        2,
        vec![
            pr_node("PR_live", "octo/widgets", answered),
            pr_node("PR_old", "Karthik/radar", silent),
        ],
        Some("aW1wOjUw"),
    );
    let (from, to) = (day("2025-01-01"), day("2025-12-31"));
    let plan = plan_range(from, to, None, &page, &me(), NOW).expect("plan");
    assert_eq!(plan.facts.prs.len(), 2);
    let live_pr = &plan.facts.prs[0];
    assert_eq!(
        live_pr.first_response_login.as_ref().map(Login::as_str),
        Some("maint")
    );
    assert!(live_pr.agent && live_pr.external);
    assert_eq!(live_pr.changes_requested_rounds, 2);
    assert!(!plan.facts.prs[1].external);
    assert_eq!(plan.facts.links.len(), 2);
    assert_eq!(
        plan.follow_ups,
        [
            WorkKind::ImportFacts(timeline_key_for("PR_old", "dGw6MjA=")),
            search_range(from, to, Some("aW1wOjUw".into())),
        ]
    );

    let conn = memory_db();
    let live = PrFact {
        source: FactSource::Live,
        additions: 99,
        ..live_pr.clone()
    };
    exec(&conn, &facts::upsert_prs(&[live])).expect("live first");
    let mut statements = plan.facts.statements();
    statements.push(repos::upsert_meta(&plan.repos));
    for stmt in &statements {
        exec(&conn, stmt).expect("import");
    }
    for stmt in &statements {
        exec(&conn, stmt).expect("the same page again");
    }
    let rows = query_json(
        &conn,
        "SELECT node_id, source, additions FROM pr_facts ORDER BY node_id",
        &[],
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0]["node_id"].as_str(), rows[0]["source"].as_str()),
        (Some("PR_live"), Some("live"))
    );
    assert_eq!(rows[0]["additions"], 99);
    assert_eq!(rows[1]["source"], "imported");
    assert_eq!(count_rows(&conn, "pr_issue_links"), 2);
    let repos = query_json(
        &conn,
        "SELECT full_name, owned_by_me, stars, primary_language FROM repos ORDER BY full_name",
        &[],
    );
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0]["full_name"], "Karthik/radar");
    assert_eq!(repos[0]["owned_by_me"], 1);
    assert_eq!(repos[1]["owned_by_me"], 0);
    assert_eq!(repos[1]["stars"], 120);
    assert_eq!(repos[1]["primary_language"], "Rust");
}

fn timeline_key_for(pr: &str, after: &str) -> ImportKey {
    ImportKey::Timeline {
        pr: NodeId::parse(pr).expect("id"),
        after: after.to_owned(),
    }
}

#[test]
fn a_reviewed_page_queues_reviews_fifty_at_a_time_and_its_next_page() {
    let nodes: Vec<_> = (0..100)
        .map(|n| json!({ "pullRequest": { "id": format!("PR_r{n}") } }))
        .collect();
    let page: Page = parse(json!({ "nodes": nodes, "pageInfo": page_info(Some("cmM6MTAw")) }));
    let follow_ups = plan_reviewed(2025, None, &page).expect("plan");
    let sizes: Vec<usize> = follow_ups
        .iter()
        .filter_map(|k| match k {
            WorkKind::ImportFacts(ImportKey::Reviews { prs, after: None }) => Some(prs.len()),
            _ => None,
        })
        .collect();
    assert_eq!(sizes, [50, 50]);
    assert_eq!(
        follow_ups.last(),
        Some(&WorkKind::ImportFacts(ImportKey::ReviewedPrs {
            year: 2025,
            after: Some("cmM6MTAw".into()),
        }))
    );
    let last: Page = parse(json!({ "nodes": [], "pageInfo": page_info(None) }));
    assert_eq!(plan_reviewed(2025, Some("cmM6MTAw"), &last), Ok(vec![]));
}

#[test]
fn three_reviews_on_one_pr_make_three_rows() {
    let review = |id: &str, state: &str, submitted: Option<&str>| json!({ "id": id, "state": state, "createdAt": "2026-02-01T10:00:00Z", "submittedAt": submitted });
    let data: Node = parse(json!({
        "nodes": [
            {
                "id": "PR_thrice",
                "number": 42,
                "repository": { "nameWithOwner": "rust-lang/rust" },
                "author": { "__typename": "User", "login": "alice" },
                "reviews": {
                    "nodes": [
                        review("PRR_1", "CHANGES_REQUESTED", Some("2026-02-01T11:00:00Z")),
                        review("PRR_2", "COMMENTED", Some("2026-02-03T11:00:00Z")),
                        review("PRR_3", "APPROVED", None),
                        review("PRR_4", "PENDING", None),
                    ],
                    "pageInfo": page_info(Some("cnY6NA==")),
                },
            },
            null,
        ],
        "rateLimit": { "cost": 1, "remaining": 4900, "resetAt": "2026-11-15T13:00:00Z" },
    }));
    let (rows, follow_ups) = plan_reviews(&data.nodes, None, &me()).expect("plan");
    assert_eq!(rows.len(), 3);
    assert!(rows
        .iter()
        .all(|r| r.source == FactSource::Imported && r.external));
    assert_eq!(rows[2].submitted_at, at("2026-02-01T10:00:00Z"));
    assert_eq!(
        follow_ups,
        [WorkKind::ImportFacts(ImportKey::Reviews {
            prs: PrSet::new(vec![NodeId::parse("PR_thrice").expect("id")]).expect("one"),
            after: Some("cnY6NA==".into()),
        })]
    );

    let conn = memory_db();
    exec(&conn, &facts::upsert_reviews(&rows)).expect("reviews");
    exec(&conn, &facts::upsert_reviews(&rows)).expect("again");
    let stored = query_json(
        &conn,
        "SELECT state, source, pr_author_login FROM review_facts ORDER BY review_id",
        &[],
    );
    assert_eq!(stored.len(), 3);
    assert_eq!(stored[0]["state"], "changes_requested");
    assert_eq!(stored[2]["state"], "approved");
    assert!(stored
        .iter()
        .all(|r| r["source"] == "imported" && r["pr_author_login"] == "alice"));
}

#[test]
fn refresh_matches_repos_by_node_id_and_owns_mine_whatever_the_case() {
    let stored = vec![
        (repo("Karthik/radar"), Some("R_1".to_owned())),
        (repo("octo/widgets"), Some("R_2".to_owned())),
    ];
    let data: Node = parse(json!({ "nodes": [
        { "id": "R_2", "nameWithOwner": "octo/widgets", "stargazerCount": 7, "primaryLanguage": null },
        { "id": "R_1", "nameWithOwner": "karthik/radar", "stargazerCount": 3, "primaryLanguage": { "name": "Rust" } },
        { "id": "R_9", "nameWithOwner": "x/y", "stargazerCount": 1, "primaryLanguage": null },
        null,
    ] }));
    let (taken, vars) = refresh_request(&stored);
    assert_eq!((taken, vars["byName"].as_bool()), (2, Some(false)));
    assert_eq!(vars["ids"], json!(["R_1", "R_2"]));
    let metas = plan_refresh(&stored, &data, &me(), NOW);
    assert_eq!(metas.len(), 2);
    assert_eq!(metas[0].full_name, repo("octo/widgets"));
    assert!(!metas[0].owned_by_me && metas[0].primary_language.is_none());
    assert_eq!(metas[1].full_name, repo("Karthik/radar"));
    assert!(metas[1].owned_by_me);
    assert_eq!(metas[1].refreshed_at, NOW);
}

#[test]
fn a_start_reads_my_creation_date_stores_it_and_queues_the_years() {
    let created = at("2024-03-10T08:00:00Z");
    let (doc, _) = request(&ImportKey::Start, &me()).expect("a viewer query");
    assert_eq!(doc.name(), "Viewer");

    let asked: Node = parse(json!({ "viewer": { "createdAt": "2024-03-10T08:00:00Z" } }));
    let done = plan_unit(&ImportKey::Start, &asked, &me(), NOW).expect("start");
    assert_eq!(done.follow_ups, year_units(created, NOW));
    let conn = memory_db();
    for stmt in &done.statements {
        exec(&conn, stmt).expect("store");
    }
    let state = query_json(&conn, "SELECT account_created_at FROM import_state", &[]);
    assert_eq!(state[0]["account_created_at"], created.0);
    assert!(plan_unit(&ImportKey::Start, &Node::default(), &me(), NOW).is_err());
}

#[test]
fn every_unit_sends_the_variables_its_query_names() {
    let range = ImportKey::SearchRange {
        from: day("2025-01-01"),
        to: day("2025-06-30"),
        after: Some("aW1wOjUw".into()),
    };
    let (doc, vars) = request(&range, &me()).expect("search");
    assert_eq!(doc.name(), "ImportPrs");
    assert_eq!(
        vars["q"],
        "is:pr author:karthik created:2025-01-01..2025-06-30"
    );
    assert_eq!(vars["first"], 50);
    assert_eq!(vars["after"], "aW1wOjUw");
    let reviewed = ImportKey::ReviewedPrs {
        year: 2025,
        after: None,
    };
    let (doc, vars) = request(&reviewed, &me()).expect("reviewed");
    assert_eq!(doc.name(), "ImportReviewedPrs");
    assert_eq!(vars["from"], "2025-01-01T00:00:00Z");
    assert_eq!(vars["to"], "2025-12-31T23:59:59Z");
    let reviews = ImportKey::Reviews {
        prs: PrSet::new(vec![NodeId::parse("PR_a").expect("id")]).expect("one"),
        after: None,
    };
    let (doc, vars) = request(&reviews, &me()).expect("reviews");
    assert_eq!(doc.name(), "ImportReviews");
    assert_eq!(vars["login"], "karthik");
    assert_eq!(vars["ids"], json!(["PR_a"]));
    let (doc, _) = request(&timeline_key("Y3I6MjA="), &me()).expect("timeline");
    assert_eq!(doc.name(), "ImportTimeline");
}

#[test]
fn a_response_missing_a_field_its_unit_needs_is_retried_not_stored() {
    let page: Node = parse(json!({
        "search": { "issueCount": 1, "nodes": [{ "id": "PR_1" }], "pageInfo": page_info(None) },
    }));
    let range = ImportKey::SearchRange {
        from: day("2025-01-01"),
        to: day("2025-12-31"),
        after: None,
    };
    assert!(plan_unit(&range, &page, &me(), NOW).is_err());
    assert!(plan_unit(&range, &Node::default(), &me(), NOW).is_err());
}

#[test]
fn a_repo_without_a_node_id_is_refreshed_by_name_and_keeps_its_id() {
    let stored = vec![
        (repo("octo/legacy"), None),
        (repo("octo/widgets"), Some("R_2".to_owned())),
    ];
    let (taken, vars) = refresh_request(&stored);
    assert_eq!(taken, 2);
    assert_eq!(vars["q"], "fork:true repo:octo/legacy");
    assert_eq!(vars["byName"], true);
    assert_eq!(vars["ids"], json!(["R_2"]));
    let data: Node = parse(json!({
        "nodes": [{ "id": "R_2", "nameWithOwner": "octo/widgets", "stargazerCount": 7, "primaryLanguage": null }],
        "search": { "nodes": [
            { "id": "R_legacy", "nameWithOwner": "Octo/Legacy", "stargazerCount": 4, "primaryLanguage": { "name": "Go" } },
            { "id": "R_other", "nameWithOwner": "octo/widgets", "stargazerCount": 99, "primaryLanguage": null },
        ] },
    }));
    let metas = plan_refresh(&stored, &data, &me(), NOW);
    assert_eq!(
        metas.len(),
        2,
        "the search cannot overwrite a repo matched by node ID"
    );
    let legacy = metas
        .iter()
        .find(|m| m.full_name == repo("octo/legacy"))
        .expect("legacy");
    assert_eq!(legacy.node_id, "R_legacy");
    assert_eq!(legacy.primary_language.as_deref(), Some("Go"));

    let conn = memory_db();
    conn.execute(
        "INSERT INTO repos (full_name, open_prs, merged_prs, closed_prs, updated_at) \
         VALUES ('octo/legacy', 1, 0, 0, 5)",
        [],
    )
    .expect("a repo card row with no node id");
    exec(&conn, &repos::upsert_meta(&metas)).expect("refresh");
    let rows = query_json(
        &conn,
        "SELECT node_id, stars, open_prs FROM repos WHERE full_name = 'octo/legacy'",
        &[],
    );
    assert_eq!(rows[0]["node_id"], "R_legacy");
    assert_eq!(rows[0]["stars"], 4);
    assert_eq!(rows[0]["open_prs"], 1);
}

#[test]
fn repos_without_a_node_id_fill_one_search_of_256_characters_and_the_page_ends_there() {
    let stored: Vec<(RepoName, Option<String>)> = (0..IMPORT_PAGE)
        .map(|n| (repo(&format!("some-owner/repository-number-{n:02}")), None))
        .collect();
    let (taken, vars) = refresh_request(&stored);
    let q = vars["q"].as_str().expect("query");
    assert!(q.len() <= 256, "{}", q.len());
    assert!(q.starts_with("fork:true repo:"), "{q}");
    assert!(taken > 0 && taken < IMPORT_PAGE);
    assert_eq!(q.matches("repo:").count(), taken);
    let next = format!("repo:{} ", stored[taken].0);
    assert!(
        q.len() + 1 + next.len() > 257,
        "the next repo would not fit"
    );
}

#[test]
fn the_name_search_includes_forks_and_is_empty_without_nameless_repos() {
    let one = vec![(repo("karthik/fork-of-tool"), None)];
    let (_, vars) = refresh_request(&one);
    assert_eq!(vars["q"], "fork:true repo:karthik/fork-of-tool");
    assert_eq!(vars["byName"], true);
    let with_ids = vec![(repo("octo/widgets"), Some("R_widgets".to_owned()))];
    let (_, vars) = refresh_request(&with_ids);
    assert_eq!(vars["q"], "");
    assert_eq!(vars["byName"], false);
}

#[test]
fn a_bad_timeline_cursor_fails_the_search_page_so_it_retries() {
    let no_cursor = json!({
        "nodes": [entry("karthik", "User", "2025-03-01T11:00:00Z", None)],
        "pageInfo": { "endCursor": null, "hasNextPage": true },
    });
    let page = search(1, vec![pr_node("PR_1", "octo/widgets", no_cursor)], None);
    let (from, to) = (day("2025-01-01"), day("2025-12-31"));
    assert!(plan_range(from, to, None, &page, &me(), NOW).is_err());
    let no_page_info = json!({ "nodes": [] });
    let page = search(1, vec![pr_node("PR_1", "octo/widgets", no_page_info)], None);
    assert!(plan_range(from, to, None, &page, &me(), NOW).is_err());
    let clean_end = json!({ "nodes": [], "pageInfo": page_info(None) });
    let page = search(1, vec![pr_node("PR_1", "octo/widgets", clean_end)], None);
    let plan = plan_range(from, to, None, &page, &me(), NOW).expect("a clean end");
    assert!(plan.follow_ups.is_empty());
}

#[test]
fn a_bad_review_cursor_fails_the_reviews_unit_so_it_retries() {
    let pr = |page: serde_json::Value| {
        json!({ "nodes": [{
            "id": "PR_r",
            "number": 4,
            "repository": { "nameWithOwner": "octo/widgets" },
            "author": { "__typename": "User", "login": "bob" },
            "reviews": { "nodes": [
                { "id": "PRR_1", "state": "APPROVED", "createdAt": "2025-01-01T00:00:00Z", "submittedAt": null },
            ], "pageInfo": page },
        }] })
    };
    let repeated: Node = parse(pr(json!({ "endCursor": "cnY6MTAw", "hasNextPage": true })));
    assert!(plan_reviews(&repeated.nodes, Some("cnY6MTAw"), &me()).is_err());
    let missing: Node = parse(pr(json!({ "endCursor": null, "hasNextPage": true })));
    assert!(plan_reviews(&missing.nodes, None, &me()).is_err());
    let next: Node = parse(pr(json!({ "endCursor": "cnY6MTAw", "hasNextPage": true })));
    let (rows, follow_ups) = plan_reviews(&next.nodes, None, &me()).expect("a usable cursor");
    assert_eq!(rows.len(), 1);
    assert_eq!(follow_ups.len(), 1);
}
