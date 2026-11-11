use domain::api::{
    Counters, Dashboard, Ended, Pipeline, PipelineCard, RecentRow, TheirRow, TurnRow,
    RECENT_WINDOW_SECS,
};
use domain::ids::Timestamp;
use domain::item::ItemState;
use domain::local_state::{visible, LocalState};
use domain::turn::Reason;

use crate::db::dashboard::{DashboardRows, ItemRecord};

const AGENT_PREFIX: &str = "agent/";

/// Builds the response from validated rows. It sorts and filters again, so
/// the result does not depend on the order or window the SQL used.
pub fn assemble(rows: DashboardRows, now: Timestamp) -> Dashboard {
    let mut waiting: Vec<ItemRecord> = rows
        .your_turn
        .into_iter()
        .filter(|w| visible(w.local, w.item.last_human.as_ref().map(|h| h.at), now))
        .map(|w| w.item)
        .collect();
    sort_by(&mut waiting, |item| item.since.0);
    let your_turn: Vec<TurnRow> = waiting.into_iter().filter_map(turn_row).collect();

    let mut theirs: Vec<ItemRecord> = rows
        .their_turn
        .into_iter()
        // A dismissal belongs to Your turn. Their turn has no dismiss key, so only a snooze hides an item here.
        .filter(|w| !matches!(w.local, Some(LocalState::Snoozed { until }) if until > now))
        .map(|w| w.item)
        .collect();
    sort_by(&mut theirs, |item| item.since.0);
    let their_turn: Vec<TheirRow> = theirs.into_iter().map(their_row).collect();

    let mut mine = rows.pipeline;
    sort_by(&mut mine, |item| item.stage_since.unwrap_or(item.since).0);
    let mut pipeline = Pipeline::empty();
    for item in mine {
        let column = pipeline
            .columns
            .iter_mut()
            .find(|c| Some(c.stage) == item.stage);
        if let Some(column) = column {
            column.cards.push(card(item));
        }
    }

    let window_start = now.0.saturating_sub(RECENT_WINDOW_SECS);
    let mut ended = rows.recent;
    sort_by(&mut ended, |item| item.closed_at.map_or(i64::MAX, |t| -t.0));
    let recent: Vec<RecentRow> = ended
        .into_iter()
        .filter_map(recent_row)
        .filter(|row| row.ended_at.0 >= window_start)
        .collect();

    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Dashboard {
        generated_at: now,
        counters: Counters {
            your_turn: count(your_turn.len()),
            their_turn: count(their_turn.len()),
            open_prs: count(pipeline.card_count()),
            merged_this_month: rows.merged_this_month,
        },
        your_turn,
        their_turn,
        pipeline,
        recent,
        nudge: rows.nudge,
    }
}

/// One stable sort for every list. A key function pointer keeps it to one
/// copy of the sort in the Worker, where each closure would add its own.
fn sort_by(items: &mut [ItemRecord], key: fn(&ItemRecord) -> i64) {
    items.sort_by_key(key);
}

fn is_agent(item: &ItemRecord) -> bool {
    item.head_ref
        .as_deref()
        .is_some_and(|r| r.starts_with(AGENT_PREFIX))
}

fn turn_row(item: ItemRecord) -> Option<TurnRow> {
    let reason = item.reason?;
    let agent = is_agent(&item);
    let url = match reason {
        Reason::CiFailed => format!("{}/checks", item.url),
        _ => item.latest_url.unwrap_or(item.url),
    };
    let (actor, what) = match item.last_human {
        Some(h) => (Some(h.actor), Some(h.what)),
        None => (None, None),
    };
    Some(TurnRow {
        id: item.id,
        kind: item.kind,
        repo: item.repo,
        number: item.number,
        title: item.title,
        url,
        reason,
        actor,
        what,
        since: item.since,
        agent,
    })
}

fn their_row(item: ItemRecord) -> TheirRow {
    let agent = is_agent(&item);
    TheirRow {
        id: item.id,
        kind: item.kind,
        repo: item.repo,
        number: item.number,
        title: item.title,
        url: item.latest_url.unwrap_or(item.url),
        since: item.since,
        agent,
        last_action: item.last_human,
        waiting_on: item.requested_reviewers,
    }
}

fn card(item: ItemRecord) -> PipelineCard {
    let agent = is_agent(&item);
    PipelineCard {
        id: item.id,
        repo: item.repo,
        number: item.number,
        title: item.title,
        url: item.url,
        agent,
        stage_since: item.stage_since.unwrap_or(item.since),
    }
}

fn recent_row(item: ItemRecord) -> Option<RecentRow> {
    let state = match item.state {
        ItemState::Merged => Ended::Merged,
        ItemState::Closed => Ended::Closed,
        ItemState::Open | ItemState::Draft => return None,
    };
    Some(RecentRow {
        id: item.id,
        kind: item.kind,
        repo: item.repo,
        number: item.number,
        title: item.title,
        url: item.url,
        state,
        ended_at: item.closed_at?,
    })
}

#[cfg(test)]
mod tests {
    use domain::api::{Action, ItemId, LastAction};
    use domain::ids::{ItemKind, Login, RepoName};
    use domain::stage::Stage;

    use domain::settings::NudgeThresholds;

    use super::*;
    use crate::db::dashboard::Waiting;

    const NOW: Timestamp = Timestamp(1_794_009_600);
    const DAY: i64 = 86_400;

    fn record(n: u32, since: i64) -> ItemRecord {
        ItemRecord {
            id: ItemId::parse(&format!("PR_{n}")).expect("id"),
            kind: ItemKind::Pr,
            repo: RepoName::parse("rust-lang/rust").expect("repo"),
            number: n,
            title: format!("Title {n}"),
            url: format!("https://github.com/rust-lang/rust/pull/{n}"),
            latest_url: None,
            head_ref: Some("feature/x".into()),
            state: ItemState::Open,
            reason: None,
            since: Timestamp(since),
            stage: None,
            stage_since: None,
            closed_at: None,
            last_human: None,
            requested_reviewers: Vec::new(),
        }
    }

    fn mine(n: u32, since: i64, reason: Reason) -> Waiting {
        Waiting {
            item: ItemRecord {
                reason: Some(reason),
                ..record(n, since)
            },
            local: None,
        }
    }

    fn waiting(item: ItemRecord) -> Waiting {
        Waiting { item, local: None }
    }

    fn staged(n: u32, stage: Stage, stage_since: i64) -> ItemRecord {
        ItemRecord {
            stage: Some(stage),
            stage_since: Some(Timestamp(stage_since)),
            ..record(n, stage_since)
        }
    }

    fn ended(n: u32, state: ItemState, days_ago: i64) -> ItemRecord {
        ItemRecord {
            state,
            closed_at: Some(Timestamp(NOW.0 - days_ago * DAY)),
            ..record(n, 0)
        }
    }

    fn ids<T>(rows: &[T], id: impl Fn(&T) -> &ItemId) -> Vec<String> {
        rows.iter().map(|r| id(r).to_string()).collect()
    }

    #[test]
    fn empty_input_yields_zero_counters_and_six_empty_columns() {
        let dashboard = assemble(DashboardRows::default(), NOW);
        assert_eq!(
            dashboard.counters,
            Counters {
                your_turn: 0,
                their_turn: 0,
                open_prs: 0,
                merged_this_month: 0
            }
        );
        assert!(dashboard.your_turn.is_empty() && dashboard.their_turn.is_empty());
        assert!(dashboard.recent.is_empty());
        let stages: Vec<Stage> = dashboard.pipeline.columns.iter().map(|c| c.stage).collect();
        assert_eq!(stages, Pipeline::STAGES);
        assert!(dashboard
            .pipeline
            .columns
            .iter()
            .all(|c| c.cards.is_empty()));
        assert_eq!(dashboard.generated_at, NOW);
        assert_eq!(dashboard.nudge, NudgeThresholds::default());
    }

    #[test]
    fn counters_equal_list_lengths_and_lists_sort_oldest_first() {
        let rows = DashboardRows {
            your_turn: vec![
                mine(1, 300, Reason::Mentioned),
                mine(2, 100, Reason::Assigned),
                mine(3, 200, Reason::Replied),
            ],
            their_turn: vec![waiting(record(4, 50)), waiting(record(5, 10))],
            pipeline: vec![
                staged(6, Stage::Draft, 30),
                staged(7, Stage::Approved, 20),
                staged(8, Stage::Draft, 10),
                ItemRecord {
                    stage: Some(Stage::Merged),
                    ..record(9, 5)
                },
            ],
            recent: vec![],
            merged_this_month: 7,
            ..DashboardRows::default()
        };
        let d = assemble(rows, NOW);
        assert_eq!(ids(&d.your_turn, |r| &r.id), ["PR_2", "PR_3", "PR_1"]);
        assert_eq!(ids(&d.their_turn, |r| &r.id), ["PR_5", "PR_4"]);
        assert_eq!(
            ids(&d.pipeline.columns[0].cards, |c| &c.id),
            ["PR_8", "PR_6"]
        );
        assert_eq!(ids(&d.pipeline.columns[5].cards, |c| &c.id), ["PR_7"]);
        assert_eq!(
            d.counters,
            Counters {
                your_turn: 3,
                their_turn: 2,
                open_prs: 3,
                merged_this_month: 7
            }
        );
    }

    #[test]
    fn recent_keeps_the_last_fourteen_days_newest_first() {
        let rows = DashboardRows {
            recent: vec![
                ended(1, ItemState::Merged, 13),
                ended(2, ItemState::Merged, 15),
                ended(3, ItemState::Closed, 3),
                ended(4, ItemState::Open, 1),
            ],
            ..DashboardRows::default()
        };
        let d = assemble(rows, NOW);
        assert_eq!(ids(&d.recent, |r| &r.id), ["PR_3", "PR_1"]);
        assert_eq!(d.recent[0].state, Ended::Closed);
        assert_eq!(d.recent[1].state, Ended::Merged);
        assert_eq!(d.recent[1].ended_at, Timestamp(NOW.0 - 13 * DAY));
    }

    #[test]
    fn a_snoozed_item_leaves_their_turn_and_the_stored_nudge_reaches_the_response() {
        let snoozed = Waiting {
            local: Some(LocalState::Snoozed {
                until: Timestamp(NOW.0 + 1),
            }),
            ..waiting(record(1, 100))
        };
        let nudge = NudgeThresholds::new(3, 5).expect("nudge");
        let rows = DashboardRows {
            their_turn: vec![snoozed, waiting(record(2, 200))],
            nudge,
            ..DashboardRows::default()
        };
        let d = assemble(rows, NOW);
        assert_eq!(ids(&d.their_turn, |r| &r.id), ["PR_2"]);
        assert_eq!(d.counters.their_turn, 1);
        assert_eq!(d.nudge, nudge);
    }

    #[test]
    fn a_dismissal_from_your_turn_does_not_hide_the_item_in_their_turn() {
        let dismissed = Waiting {
            local: Some(LocalState::Dismissed {
                seen_human_at: Timestamp(NOW.0),
            }),
            ..waiting(record(1, 100))
        };
        let rows = DashboardRows {
            their_turn: vec![dismissed],
            ..DashboardRows::default()
        };
        let d = assemble(rows, NOW);
        assert_eq!(ids(&d.their_turn, |r| &r.id), ["PR_1"]);
        assert_eq!(d.counters.their_turn, 1);
    }

    #[test]
    fn hidden_items_leave_your_turn_and_its_counter() {
        let human = |at: i64| {
            Some(LastAction {
                actor: Login::parse("octocat").expect("login"),
                what: Action::Comment,
                at: Timestamp(at),
            })
        };
        let with = |n: u32, last: i64, local: LocalState| Waiting {
            item: ItemRecord {
                last_human: human(last),
                ..mine(n, 100, Reason::Replied).item
            },
            local: Some(local),
        };
        let seen = |at: i64| LocalState::Dismissed {
            seen_human_at: Timestamp(at),
        };
        let rows = DashboardRows {
            your_turn: vec![
                with(1, 500, seen(500)),
                with(2, 501, seen(500)),
                with(
                    3,
                    500,
                    LocalState::Snoozed {
                        until: Timestamp(NOW.0 + 1),
                    },
                ),
                with(4, 500, LocalState::Snoozed { until: NOW }),
                mine(5, 100, Reason::Mentioned),
            ],
            ..DashboardRows::default()
        };
        let d = assemble(rows, NOW);
        assert_eq!(ids(&d.your_turn, |r| &r.id), ["PR_2", "PR_4", "PR_5"]);
        assert_eq!(d.counters.your_turn, 3);
    }

    #[test]
    fn rows_link_to_the_latest_activity_or_the_checks_page() {
        let link = "https://github.com/rust-lang/rust/pull/1#issuecomment-9";
        let latest = Some(link.to_owned());
        let review = Waiting {
            item: ItemRecord {
                latest_url: latest.clone(),
                head_ref: Some("agent/fix-lexer".into()),
                ..mine(1, 100, Reason::ReviewRequested).item
            },
            local: None,
        };
        let ci = Waiting {
            item: ItemRecord {
                latest_url: latest.clone(),
                ..mine(2, 200, Reason::CiFailed).item
            },
            local: None,
        };
        let plain = mine(3, 300, Reason::Mentioned);
        let rows = DashboardRows {
            your_turn: vec![review, ci, plain],
            their_turn: vec![waiting(ItemRecord {
                latest_url: latest.clone(),
                requested_reviewers: vec![Login::parse("alice").expect("login")],
                ..record(4, 10)
            })],
            ..DashboardRows::default()
        };
        let d = assemble(rows, NOW);
        let urls: Vec<&str> = d.your_turn.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://github.com/rust-lang/rust/pull/1#issuecomment-9",
                "https://github.com/rust-lang/rust/pull/2/checks",
                "https://github.com/rust-lang/rust/pull/3",
            ]
        );
        let agents: Vec<bool> = d.your_turn.iter().map(|r| r.agent).collect();
        assert_eq!(agents, [true, false, false]);
        assert_eq!(d.their_turn[0].url, link);
        assert_eq!(d.their_turn[0].waiting_on[0].as_str(), "alice");
    }

    #[test]
    fn a_card_without_a_stage_clock_falls_back_to_since() {
        let rows = DashboardRows {
            pipeline: vec![ItemRecord {
                stage: Some(Stage::CiRunning),
                ..record(1, 1234)
            }],
            ..DashboardRows::default()
        };
        let d = assemble(rows, NOW);
        assert_eq!(d.pipeline.columns[1].cards[0].stage_since, Timestamp(1234));
    }
}
