use domain::activity::{Actor, ActorKind};
use domain::ids::{IdError, ItemKind, ItemRef, Login, RepoName, Timestamp};
use domain::item::{CiState, ItemState, ReviewDecision};
use domain::stage::Stage;
use domain::turn::{Reason, Turn};

use super::items::{ItemRow, LastHuman};

/// 2026-11-06T12:34:56Z. Every synthetic timestamp is relative to it, so the
/// output depends only on `n`.
pub const NOW: Timestamp = Timestamp(1_793_968_496);

const ME: &str = "karthik-beta";
const HOUR: i64 = 3600;
const DAY: i64 = 86_400;

#[derive(Clone, Copy)]
struct Template {
    kind: ItemKind,
    state: ItemState,
    mine: bool,
    turn: Turn,
    stage: Option<Stage>,
}

const PR: ItemKind = ItemKind::Pr;
const ISSUE: ItemKind = ItemKind::Issue;

const fn open(kind: ItemKind, mine: bool, turn: Turn, stage: Option<Stage>) -> Template {
    Template {
        kind,
        state: ItemState::Open,
        mine,
        turn,
        stage,
    }
}

const fn ended(kind: ItemKind, state: ItemState, stage: Option<Stage>) -> Template {
    Template {
        kind,
        state,
        mine: true,
        turn: Turn::Them,
        stage,
    }
}

/// Waiting on me. Every reason appears once.
const ME_TEMPLATES: [Template; 6] = [
    open(
        PR,
        false,
        Turn::Me(Reason::ReviewRequested),
        Some(Stage::AwaitingReview),
    ),
    open(
        PR,
        true,
        Turn::Me(Reason::ChangesRequested),
        Some(Stage::ChangesRequested),
    ),
    open(PR, true, Turn::Me(Reason::CiFailed), Some(Stage::CiFailing)),
    open(ISSUE, false, Turn::Me(Reason::Mentioned), None),
    open(ISSUE, false, Turn::Me(Reason::Assigned), None),
    open(
        PR,
        true,
        Turn::Me(Reason::Replied),
        Some(Stage::AwaitingReview),
    ),
];

/// My open items waiting on someone else: each open stage left over, and an issue.
const THEM_TEMPLATES: [Template; 5] = [
    Template {
        state: ItemState::Draft,
        ..open(PR, true, Turn::Them, Some(Stage::Draft))
    },
    open(PR, true, Turn::Them, Some(Stage::CiRunning)),
    open(PR, true, Turn::Them, Some(Stage::AwaitingReview)),
    open(PR, true, Turn::Them, Some(Stage::Approved)),
    open(ISSUE, true, Turn::Them, None),
];

const THIRD_PARTY: Template = open(PR, false, Turn::Them, Some(Stage::AwaitingReview));

const ENDED_TEMPLATES: [Template; 3] = [
    ended(PR, ItemState::Merged, Some(Stage::Merged)),
    ended(PR, ItemState::Closed, Some(Stage::Closed)),
    ended(ISSUE, ItemState::Closed, None),
];

/// The shape of one maintainer's database with `n` items. The first 14 cover
/// every turn reason and every stage. After that, out of every 1000 items,
/// 8 wait on me, 12 are my open items waiting on others, 10 are other
/// people's open threads, 20 closed in the last 14 days, and the rest closed
/// 15 to 90 days ago. At 5000 items, about 40 wait on me.
pub fn items(n: u32) -> Result<Vec<ItemRow>, IdError> {
    (0..n).map(item).collect()
}

fn pick(i: u32) -> (Template, bool) {
    if i < 14 {
        let cover = ME_TEMPLATES
            .iter()
            .chain(&THEM_TEMPLATES)
            .chain(&ENDED_TEMPLATES)
            .nth(i as usize);
        return (cover.copied().unwrap_or(THIRD_PARTY), true);
    }
    let r = (i % 1000) as usize;
    match r {
        0..=7 => (ME_TEMPLATES[r % ME_TEMPLATES.len()], false),
        8..=19 => (THEM_TEMPLATES[r % THEM_TEMPLATES.len()], false),
        20..=29 => (THIRD_PARTY, false),
        30..=49 => (ENDED_TEMPLATES[r % ENDED_TEMPLATES.len()], true),
        _ => (ENDED_TEMPLATES[r % ENDED_TEMPLATES.len()], false),
    }
}

fn ci_state(stage: Option<Stage>) -> Option<CiState> {
    match stage? {
        Stage::CiRunning => Some(CiState::Pending),
        Stage::CiFailing => Some(CiState::Failing),
        Stage::AwaitingReview | Stage::ChangesRequested | Stage::Approved => Some(CiState::Passing),
        Stage::Draft | Stage::Merged | Stage::Closed => None,
    }
}

fn review_decision(stage: Option<Stage>) -> Option<ReviewDecision> {
    match stage? {
        Stage::AwaitingReview => Some(ReviewDecision::ReviewRequired),
        Stage::ChangesRequested => Some(ReviewDecision::ChangesRequested),
        Stage::Approved => Some(ReviewDecision::Approved),
        _ => None,
    }
}

fn item(i: u32) -> Result<ItemRow, IdError> {
    let (template, recent) = pick(i);
    let offset = |floor: i64, span: i64| NOW.0 - floor - (i64::from(i) * 7919).rem_euclid(span);
    let closed_at = matches!(template.state, ItemState::Merged | ItemState::Closed).then(|| {
        Timestamp(if recent {
            offset(HOUR, 14 * DAY - HOUR)
        } else {
            offset(15 * DAY, 75 * DAY)
        })
    });
    let since = closed_at.unwrap_or(Timestamp(offset(HOUR, 20 * DAY)));
    let repo = format!("{ME}/synthetic-{}", i % 20);
    let number = i + 1;
    let maintainer = Login::parse(&format!("maintainer-{}", i % 7))?;
    let author = if template.mine {
        Login::parse(ME)?
    } else {
        maintainer.clone()
    };
    let (path, head_ref) = match template.kind {
        ItemKind::Pr if i.is_multiple_of(4) => ("pull", Some(format!("agent/task-{i}"))),
        ItemKind::Pr => ("pull", Some(format!("feature/{i}"))),
        ItemKind::Issue => ("issues", None),
    };
    let heard_from_a_human = !template.mine || matches!(template.turn, Turn::Me(_));
    Ok(ItemRow {
        node_id: format!("S_{i}"),
        item_ref: ItemRef {
            repo: RepoName::parse(&repo)?,
            number,
            kind: template.kind,
        },
        title: format!("Synthetic item {i}"),
        url: format!("https://github.com/{repo}/{path}/{number}"),
        head_ref,
        author: Actor {
            login: author,
            kind: ActorKind::User,
        },
        authored_by_me: template.mine,
        state: template.state,
        created_at: Timestamp(since.0 - DAY),
        updated_at: since,
        closed_at,
        turn: template.turn,
        since,
        stage: template.stage,
        ci_state: ci_state(template.stage),
        review_decision: review_decision(template.stage),
        last_human: heard_from_a_human.then(|| LastHuman {
            at: since,
            login: maintainer,
            kind: "comment".to_owned(),
            url: Some(format!(
                "https://github.com/{repo}/{path}/{number}#issuecomment-{i}"
            )),
        }),
        requested_reviewers: Vec::new(),
        stage_since: since,
        latest_url: format!("https://github.com/{repo}/{path}/{number}"),
        synced_at: NOW,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::items::upsert;
    use crate::db::testing::{count_rows, exec, memory_db};

    #[test]
    fn small_sets_cover_every_reason_and_stage() {
        let rows = items(14).expect("items");
        for reason in Reason::PRECEDENCE {
            assert!(
                rows.iter().any(|r| r.turn == Turn::Me(reason)),
                "{reason:?}"
            );
        }
        for stage in Stage::ALL {
            assert!(rows.iter().any(|r| r.stage == Some(stage)), "{stage:?}");
        }
        assert!(rows.iter().any(|r| r.state == ItemState::Draft));
    }

    #[test]
    fn output_depends_only_on_n() {
        let big = items(300).expect("items");
        assert_eq!(big, items(300).expect("items"));
        assert_eq!(items(40).expect("items"), big[..40]);
    }

    #[test]
    fn five_thousand_items_satisfy_every_check_and_wait_on_me_about_forty_times() {
        let conn = memory_db();
        let rows = items(5000).expect("items");
        for row in &rows {
            exec(&conn, &upsert(row)).expect("synthetic row passes the CHECKs");
        }
        assert_eq!(count_rows(&conn, "items"), 5000);
        let waiting = count_rows(&conn, "items WHERE turn = 'me'");
        assert!((35..=50).contains(&waiting), "{waiting} items wait on me");
    }
}
