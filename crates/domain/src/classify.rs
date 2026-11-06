use crate::activity::{Activity, ActivityKind, ActorKind, ReviewState};
use crate::ids::{ItemKind, Login, Timestamp};
use crate::item::{CiState, Item, ItemState, ReviewDecision};
use crate::stage::Stage;
use crate::turn::{Classification, Reason, Turn};

/// Decides whose move an item waits on and where a pull request sits in its
/// pipeline. Bot activity is dropped first, so bots never flip the turn.
pub fn classify(item: &Item, me: &Login) -> Classification {
    let mut human: Vec<&Activity> = item
        .activity
        .iter()
        .filter(|a| a.actor.kind == ActorKind::User)
        .collect();
    human.sort_by_key(|a| a.at);
    let reviews = Reviews::of(item, &human);
    let latest_human = human.last().copied();

    let (turn, since) = match item.state {
        ItemState::Merged | ItemState::Closed => {
            (Turn::Them, item.closed_at.unwrap_or(item.updated_at))
        }
        ItemState::Open | ItemState::Draft => {
            let triggers = triggers(item, me, &human, &reviews);
            let reason = Reason::PRECEDENCE
                .into_iter()
                .find(|r| triggers.iter().any(|(t, at)| t == r && at.is_some()));
            let oldest = triggers.iter().filter_map(|(_, at)| *at).min();
            match (reason, oldest) {
                (Some(reason), Some(since)) => (Turn::Me(reason), since),
                _ => (Turn::Them, latest_human.map_or(item.created_at, |a| a.at)),
            }
        }
    };

    Classification {
        turn,
        since,
        stage: stage(item, &reviews),
        last_human: latest_human.cloned(),
    }
}

struct Reviews {
    decision: Option<ReviewDecision>,
    changes_outstanding_since: Option<Timestamp>,
}

impl Reviews {
    fn of(item: &Item, human: &[&Activity]) -> Self {
        let author = &item.author.login;
        let mut opinions: Vec<(&Login, Option<(ReviewDecision, Timestamp)>)> = Vec::new();
        for a in human {
            let ActivityKind::Review { state } = a.kind else {
                continue;
            };
            if a.actor.login == *author {
                continue;
            }
            let opinion = match state {
                ReviewState::Approved => Some((ReviewDecision::Approved, a.at)),
                ReviewState::ChangesRequested => Some((ReviewDecision::ChangesRequested, a.at)),
                ReviewState::Dismissed => None,
                ReviewState::Commented | ReviewState::Pending => continue,
            };
            match opinions.iter_mut().find(|(who, _)| **who == a.actor.login) {
                Some(slot) => slot.1 = opinion,
                None => opinions.push((&a.actor.login, opinion)),
            }
        }

        let held = |decision: ReviewDecision| {
            opinions
                .iter()
                .filter_map(|(_, opinion)| *opinion)
                .filter(move |(d, _)| *d == decision)
                .map(|(_, at)| at)
        };
        let last_author_push = human
            .iter()
            .filter(|a| matches!(a.kind, ActivityKind::Pushed) && a.actor.login == *author)
            .map(|a| a.at)
            .max();
        let changes_outstanding_since = held(ReviewDecision::ChangesRequested)
            .filter(|at| last_author_push.is_none_or(|push| push <= *at))
            .min();
        let derived = if held(ReviewDecision::ChangesRequested).next().is_some() {
            Some(ReviewDecision::ChangesRequested)
        } else if held(ReviewDecision::Approved).next().is_some() {
            Some(ReviewDecision::Approved)
        } else {
            None
        };

        Self {
            decision: item.review_decision.or(derived),
            changes_outstanding_since,
        }
    }
}

fn stage(item: &Item, reviews: &Reviews) -> Option<Stage> {
    match (item.item_ref.kind, item.state) {
        (_, ItemState::Merged) => Some(Stage::Merged),
        (_, ItemState::Closed) => Some(Stage::Closed),
        (ItemKind::Issue, _) => None,
        (ItemKind::Pr, ItemState::Draft) => Some(Stage::Draft),
        (ItemKind::Pr, ItemState::Open) => {
            let ci = item.ci.map(|ci| ci.state);
            Some(
                if reviews.decision == Some(ReviewDecision::ChangesRequested)
                    && reviews.changes_outstanding_since.is_some()
                {
                    Stage::ChangesRequested
                } else if ci == Some(CiState::Failing) {
                    Stage::CiFailing
                } else if ci == Some(CiState::Pending) {
                    Stage::CiRunning
                } else if reviews.decision == Some(ReviewDecision::Approved) {
                    Stage::Approved
                } else {
                    Stage::AwaitingReview
                },
            )
        }
    }
}

fn answers(kind: &ActivityKind) -> bool {
    match kind {
        ActivityKind::Comment { .. } | ActivityKind::Pushed => true,
        ActivityKind::Review { state } => *state != ReviewState::Pending,
        ActivityKind::ReviewRequested { .. } | ActivityKind::Assigned { .. } => false,
    }
}

fn triggers(
    item: &Item,
    me: &Login,
    human: &[&Activity],
    reviews: &Reviews,
) -> [(Reason, Option<Timestamp>); 6] {
    let mine = item.author.login == *me;
    let pr = item.item_ref.kind == ItemKind::Pr;
    let ready = item.state == ItemState::Open;
    let last_answer = human
        .iter()
        .filter(|a| a.actor.login == *me && answers(&a.kind))
        .map(|a| a.at)
        .max();
    // An answer in the same second as a trigger does not count, because the
    // order of the two is unknowable.
    let unanswered = |at: &Timestamp| last_answer.is_none_or(|answer| answer <= *at);
    let from_others = || human.iter().filter(|a| a.actor.login != *me);
    let latest_event = |matches: &dyn Fn(&ActivityKind) -> bool| {
        human
            .iter()
            .filter(|a| matches(&a.kind))
            .map(|a| a.at)
            .max()
            .unwrap_or(item.created_at)
    };

    let changes_requested = reviews.changes_outstanding_since.filter(|_| mine && pr);
    let ci_failed = item
        .ci
        .filter(|ci| mine && pr && ready && ci.state == CiState::Failing)
        .map(|ci| ci.at);
    let review_requested = (!mine && pr && ready && item.review_requests.contains(me)).then(|| {
        latest_event(
            &|kind| matches!(kind, ActivityKind::ReviewRequested { reviewer } if reviewer == me),
        )
    });
    let mentioned = from_others()
        .filter(|a| matches!(a.kind, ActivityKind::Comment { mentions_me: true }))
        .map(|a| a.at)
        .filter(unanswered)
        .min();
    let assigned = (!mine && item.assignees.contains(me))
        .then(|| {
            latest_event(
                &|kind| matches!(kind, ActivityKind::Assigned { assignee } if assignee == me),
            )
        })
        .filter(unanswered);
    let replied = from_others()
        .filter(|a| {
            matches!(
                a.kind,
                ActivityKind::Comment { .. }
                    | ActivityKind::Review {
                        state: ReviewState::Commented
                    }
            )
        })
        .map(|a| a.at)
        .filter(unanswered)
        .min()
        .filter(|_| mine);

    [
        (Reason::ChangesRequested, changes_requested),
        (Reason::CiFailed, ci_failed),
        (Reason::ReviewRequested, review_requested),
        (Reason::Mentioned, mentioned),
        (Reason::Assigned, assigned),
        (Reason::Replied, replied),
    ]
}
