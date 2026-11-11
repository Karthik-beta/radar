use domain::api::{Action, LastAction, TheirRow};
use domain::ids::Login;
use leptos::prelude::*;

use crate::components::empty_state::EmptyState;
use crate::components::key_hints::KeyHints;
use crate::components::triage_list::{triage_list, ListKind, TriageList};
use crate::components::triage_row::RowData;
use crate::heat::{days_phrase, heat, Heat, Thresholds};
use crate::store::{use_store, Load};
use crate::url::GithubUrl;

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

const HINTS: &[(&str, &str)] = &[
    ("j", "next"),
    ("k", "previous"),
    ("enter", "open"),
    ("s", "snooze"),
];

pub fn waiting_on(reviewers: &[Login]) -> String {
    if reviewers.is_empty() {
        return "maintainers".to_owned();
    }
    reviewers
        .iter()
        .map(Login::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

fn action_words(what: Action) -> &'static str {
    match what {
        Action::Comment => "commented",
        Action::Review => "reviewed",
        Action::ReviewRequested => "requested a review",
        Action::Assigned => "assigned",
        Action::Pushed => "pushed",
    }
}

/// "alice commented, waiting on bob", or "Waiting on maintainers" when no
/// human has acted.
pub fn who_line(last: Option<&LastAction>, reviewers: &[Login]) -> String {
    let on = waiting_on(reviewers);
    match last {
        Some(last) => format!(
            "{} {}, waiting on {on}",
            last.actor,
            action_words(last.what)
        ),
        None => format!("Waiting on {on}"),
    }
}

/// How many rows have waited past the warm threshold, as of `now`.
pub fn stale_count(since: impl IntoIterator<Item = i64>, now: i64, t: Thresholds) -> usize {
    since
        .into_iter()
        .filter(|since| heat(now.saturating_sub(*since), t) != Heat::Fresh)
        .count()
}

pub fn summary(stale: usize, warm_days: u32) -> String {
    let phrase = days_phrase(warm_days);
    match stale {
        0 => format!("None has waited over {phrase}"),
        1 => format!("1 has waited over {phrase}"),
        n => format!("{n} have waited over {phrase}"),
    }
}

pub fn nudge_line(warm_days: u32) -> String {
    format!("Nudging is fair after {}.", days_phrase(warm_days))
}

pub fn nothing_waiting(your_turn: u32) -> String {
    let verb = if your_turn == 1 { "is" } else { "are" };
    format!("Nothing is waiting on others. {your_turn} {verb} waiting on you.")
}

fn row_data(row: &TheirRow) -> RowData {
    RowData {
        id: row.id.clone(),
        url: GithubUrl::parse(&row.url),
        reason: None,
        repo: row.repo.clone(),
        number: row.number,
        title: row.title.clone(),
        agent: row.agent,
        who: Some(who_line(row.last_action.as_ref(), &row.waiting_on)),
        usual_reply: row.usual_reply_secs,
        since: row.since,
    }
}

/// Every item waiting on someone else, longest wait first, with a warm or hot
/// age when a nudge is fair (F3, F4).
#[component]
pub fn TheirTurn() -> impl IntoView {
    let store = use_store();
    let now = store.now_secs();
    let rows = Memo::new(move |_| {
        store.dashboard().with(|d| {
            d.as_ref()
                .map(|d| d.their_turn.iter().map(row_data).collect())
                .unwrap_or_default()
        })
    });
    let nudge = Memo::new(move |_| {
        store
            .dashboard()
            .with(|d| d.as_ref().map(|d| d.nudge).unwrap_or_default())
    });
    let thresholds = Memo::new(move |_| Thresholds::from_nudge(nudge.get()));
    let TriageList { active, view: list } =
        triage_list(ListKind::TheirTurn, rows, thresholds.into());

    let your_turn = move || {
        store
            .dashboard()
            .with(|d| d.as_ref().map_or(0, |d| d.counters.your_turn))
    };
    let summary_text = Signal::derive(move || {
        let stale = rows.with(|rows| {
            stale_count(
                rows.iter().map(|row| row.since.0),
                now.get(),
                thresholds.get(),
            )
        });
        summary(stale, nudge.get().warm_days())
    });
    let empty = move || active.with(Vec::is_empty) && rows.with(Vec::is_empty);
    let list = StoredValue::new(list);

    view! {
        <section class="their-turn">
            {move || match store.load().get() {
                Load::Loading => view! { <p class="text-muted">"Loading."</p> }.into_any(),
                Load::Failed(_) => {
                    view! { <p class="text-muted" role="alert">"Could not load your list. Radar will try again."</p> }
                        .into_any()
                }
                Load::SignedOut | Load::Ready { .. } | Load::Stale { .. } => ().into_any(),
            }}
            <Show when=move || store.dashboard().with(Option::is_some)>
                <Show
                    when=move || !empty()
                    fallback=move || view! { <EmptyState message=Signal::derive(move || nothing_waiting(your_turn()))/> }
                >
                    <div class="summary" aria-label="Summary">
                        <h1 class="summary-title" data-summary>{move || summary_text.get()}</h1>
                        <p class="summary-line">{move || nudge_line(nudge.get().warm_days())}</p>
                    </div>
                    {move || store.notice().get().map(|text| view! { <p class="notice" role="status">{text}</p> })}
                    {move || list.with_value(ViewFn::run)}
                    <KeyHints hints=HINTS/>
                </Show>
            </Show>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heat::Thresholds;

    const DAY: i64 = 86_400;

    fn login(name: &str) -> Login {
        Login::parse(name).expect("login")
    }

    #[test]
    fn summary_names_how_many_waited_past_the_warm_threshold() {
        assert_eq!(summary(0, 7), "None has waited over a week");
        assert_eq!(summary(1, 7), "1 has waited over a week");
        assert_eq!(summary(5, 7), "5 have waited over a week");
        assert_eq!(summary(2, 3), "2 have waited over 3 days");
        assert_eq!(nudge_line(7), "Nudging is fair after a week.");
        assert_eq!(nudge_line(14), "Nudging is fair after two weeks.");
    }

    #[test]
    fn stale_rows_are_the_ones_at_or_past_the_warm_threshold() {
        let t = Thresholds {
            warm: 7 * DAY,
            hot: 14 * DAY,
        };
        let now = 100 * DAY;
        let ages = [6 * DAY + 86_399, 7 * DAY, 13 * DAY, 15 * DAY];
        assert_eq!(stale_count(ages.map(|a| now - a), now, t), 3);
        assert_eq!(stale_count([], now, t), 0);
    }

    #[test]
    fn waiting_on_lists_reviewers_or_names_the_maintainers() {
        assert_eq!(waiting_on(&[]), "maintainers");
        assert_eq!(waiting_on(&[login("alice")]), "alice");
        assert_eq!(
            waiting_on(&[login("alice"), login("bob"), login("carol")]),
            "alice, bob, carol"
        );
    }

    #[test]
    fn who_line_joins_the_last_action_and_who_it_waits_on() {
        let last = LastAction {
            actor: login("karthik"),
            what: Action::Comment,
            at: domain::ids::Timestamp(1),
        };
        assert_eq!(
            who_line(Some(&last), &[login("alice"), login("bob")]),
            "karthik commented, waiting on alice, bob"
        );
        assert_eq!(who_line(None, &[]), "Waiting on maintainers");
    }

    #[test]
    fn empty_state_counts_what_waits_on_the_operator() {
        assert_eq!(
            nothing_waiting(4),
            "Nothing is waiting on others. 4 are waiting on you."
        );
        assert_eq!(
            nothing_waiting(1),
            "Nothing is waiting on others. 1 is waiting on you."
        );
    }
}
