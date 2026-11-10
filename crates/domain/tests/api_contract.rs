use domain::api::{
    Action, Counters, Dashboard, Ended, ItemId, LastAction, Pipeline, PipelineCard, RecentRow,
    TheirRow, TurnRow,
};
use domain::ids::{ItemKind, Login, RepoName, Timestamp};
use domain::settings::NudgeThresholds;
use domain::turn::Reason;

const GOLDEN: &str = include_str!("golden/dashboard.json");

fn id(s: &str) -> ItemId {
    ItemId::parse(s).unwrap()
}

fn repo() -> RepoName {
    RepoName::parse("rust-lang/rust").unwrap()
}

fn login(s: &str) -> Login {
    Login::parse(s).unwrap()
}

fn fixed() -> Dashboard {
    let mut pipeline = Pipeline::empty();
    pipeline.columns[3].cards.push(PipelineCard {
        id: id("PR_mine"),
        repo: repo(),
        number: 3,
        title: "Speed up the parser".into(),
        url: "https://github.com/rust-lang/rust/pull/3".into(),
        agent: true,
        stage_since: Timestamp(1_793_000_000),
    });
    Dashboard {
        generated_at: Timestamp(1_793_900_000),
        counters: Counters {
            your_turn: 2,
            their_turn: 1,
            open_prs: 1,
            merged_this_month: 4,
        },
        your_turn: vec![
            TurnRow {
                id: id("PR_review"),
                kind: ItemKind::Pr,
                repo: repo(),
                number: 1,
                title: "Fix the lexer".into(),
                url: "https://github.com/rust-lang/rust/pull/1#issuecomment-9".into(),
                reason: Reason::ReviewRequested,
                actor: Some(login("octocat")),
                what: Some(Action::ReviewRequested),
                since: Timestamp(1_793_100_000),
                agent: false,
            },
            TurnRow {
                id: id("PR_ci"),
                kind: ItemKind::Pr,
                repo: repo(),
                number: 2,
                title: "Add a test".into(),
                url: "https://github.com/rust-lang/rust/pull/2/checks".into(),
                reason: Reason::CiFailed,
                actor: None,
                what: None,
                since: Timestamp(1_793_200_000),
                agent: true,
            },
        ],
        their_turn: vec![TheirRow {
            id: id("I_wait"),
            kind: ItemKind::Issue,
            repo: repo(),
            number: 4,
            title: "Docs are wrong".into(),
            url: "https://github.com/rust-lang/rust/issues/4".into(),
            since: Timestamp(1_792_000_000),
            agent: false,
            last_action: Some(LastAction {
                actor: login("karthik"),
                what: Action::Comment,
                at: Timestamp(1_792_000_000),
            }),
            waiting_on: vec![login("alice"), login("bob")],
            usual_reply_secs: Some(259_200),
        }],
        pipeline,
        recent: vec![
            RecentRow {
                id: id("PR_merged"),
                kind: ItemKind::Pr,
                repo: repo(),
                number: 5,
                title: "Ship it".into(),
                url: "https://github.com/rust-lang/rust/pull/5".into(),
                state: Ended::Merged,
                ended_at: Timestamp(1_793_800_000),
            },
            RecentRow {
                id: id("PR_closed"),
                kind: ItemKind::Pr,
                repo: repo(),
                number: 6,
                title: "Not this one".into(),
                url: "https://github.com/rust-lang/rust/pull/6".into(),
                state: Ended::Closed,
                ended_at: Timestamp(1_793_700_000),
            },
        ],
        nudge: NudgeThresholds::default(),
    }
}

#[test]
fn dashboard_matches_the_golden_file() {
    let actual = serde_json::to_value(fixed()).unwrap();
    let golden: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(
        actual,
        golden,
        "dashboard JSON changed:\n{}",
        serde_json::to_string_pretty(&actual).unwrap()
    );
}

#[test]
fn golden_file_reads_back_as_the_same_dashboard() {
    let parsed: Dashboard = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(parsed, fixed());
}
