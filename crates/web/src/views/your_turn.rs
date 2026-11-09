use domain::api::{ItemId, TurnRow};
use leptos::prelude::*;

use crate::components::count_block::CountBlock;
use crate::components::empty_state::{threads_are, EmptyState};
use crate::components::key_hints::KeyHints;
use crate::components::snooze_menu::SnoozeMenu;
use crate::components::triage_row::{RowData, TriageRow};
use crate::heat::{age_words, Thresholds};
use crate::keys::{self, Effect, Key, Mode, State};
use crate::store::{use_store, Load, Triage};
use crate::url::GithubUrl;

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

const HINTS: &[(&str, &str)] = &[
    ("j", "next"),
    ("k", "previous"),
    ("enter", "open"),
    ("e", "dismiss"),
    ("s", "snooze"),
];

pub fn nothing_waiting(their_turn: u32) -> String {
    format!(
        "Nothing is waiting on you. {} waiting on others.",
        threads_are(their_turn)
    )
}

pub fn oldest_line(age_secs: i64) -> String {
    format!("The oldest has waited {}.", age_words(age_secs))
}

/// Every item waiting on the operator, oldest first, triaged from the
/// keyboard (F1, F2, F9).
#[component]
pub fn YourTurn() -> impl IntoView {
    let store = use_store();
    let now = store.now_secs();
    let rows = Memo::new(move |_| {
        store
            .dashboard()
            .with(|d| d.as_ref().map(|d| d.your_turn.clone()).unwrap_or_default())
    });
    let active = Memo::new(move |_| {
        rows.with(|rows| {
            rows.iter()
                .filter(|row| !store.is_leaving(&row.id))
                .cloned()
                .collect::<Vec<TurnRow>>()
        })
    });
    let keys_state = RwSignal::new(State::default());
    let selected = Memo::new(move |_| {
        let selection = keys_state.with(|s| s.selection);
        active.with(|rows| {
            selection
                .clamp(rows.len())
                .index()
                .and_then(|i| rows.get(i))
                .map(|row| row.id.clone())
        })
    });
    let snoozing = Memo::new(move |_| {
        keys_state.with(|s| match &s.mode {
            Mode::Snoozing { item } => Some(item.clone()),
            Mode::Browsing => None,
        })
    });

    let press = move |key: Key| {
        let (next, effect) =
            active.with_untracked(|rows| keys::step(keys_state.get_untracked(), key, rows));
        keys_state.set(next);
        match effect {
            Some(Effect::Open(url)) => open_tab(&url),
            Some(Effect::Dismiss(id)) => store.triage(id, Triage::Dismiss),
            Some(Effect::Snooze(id, snooze_for)) => store.triage(id, Triage::Snooze(snooze_for)),
            None => {}
        }
    };
    keys::listen(press);

    let count = Signal::derive(move || {
        store
            .dashboard()
            .with(|d| d.as_ref().map_or(0, |d| d.counters.your_turn))
    });
    let their_turn = move || {
        store
            .dashboard()
            .with(|d| d.as_ref().map_or(0, |d| d.counters.their_turn))
    };
    let oldest = Signal::derive(move || {
        active.with(|rows| {
            rows.iter()
                .map(|row| row.since.0)
                .min()
                .map(|since| oldest_line(now.get().saturating_sub(since)))
                .unwrap_or_default()
        })
    });
    let empty = move || active.with(Vec::is_empty) && rows.with(Vec::is_empty);

    let list = move || {
        view! {
            <ul class="rows" aria-label="Waiting on you">
                <For each=move || rows.get() key=|row| row.id.clone() let:row>
                    {
                        let id = row.id.clone();
                        let is = move |other: Option<ItemId>| other.as_ref() == Some(&id);
                        let (sel_id, snooze_id, leave_id) = (is.clone(), is.clone(), row.id.clone());
                        let open_menu = Signal::derive(move || snooze_id(snoozing.get()));
                        view! {
                            <TriageRow
                                row=RowData::from(&row)
                                thresholds=Thresholds::YOUR_TURN
                                now=now
                                selected=Signal::derive(move || sel_id(selected.get()))
                                leaving=Signal::derive(move || store.is_leaving(&leave_id))
                            >
                                <SnoozeMenu
                                    open=open_menu
                                    on_pick=move |snooze_for| {
                                        press(match snooze_for {
                                            domain::api::SnoozeFor::Day => Key::Day,
                                            domain::api::SnoozeFor::Week => Key::Week,
                                        })
                                    }
                                />
                            </TriageRow>
                        }
                    }
                </For>
            </ul>
        }
    };

    view! {
        <section class="your-turn">
            {move || match store.load().get() {
                Load::Loading => view! { <p class="text-muted">"Loading."</p> }.into_any(),
                Load::Failed(_) => {
                    view! { <p class="text-muted" role="alert">"Could not load your list. Radar will try again."</p> }
                        .into_any()
                }
                Load::SignedOut => ().into_any(),
                Load::Ready { .. } | Load::Stale { .. } => ().into_any(),
            }}
            <Show when=move || store.dashboard().with(Option::is_some)>
                <Show
                    when=move || !empty()
                    fallback=move || view! { <EmptyState message=Signal::derive(move || nothing_waiting(their_turn()))/> }
                >
                    <CountBlock count=count heading="waiting on you" line=oldest/>
                    {move || store.notice().get().map(|text| view! { <p class="notice" role="status">{text}</p> })}
                    {list}
                    <KeyHints hints=HINTS/>
                </Show>
            </Show>
        </section>
    }
}

fn open_tab(url: &GithubUrl) {
    let _ = window().open_with_url_and_target_and_features(
        url.as_str(),
        "_blank",
        "noopener,noreferrer",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_names_the_threads_waiting_on_others() {
        assert_eq!(
            nothing_waiting(0),
            "Nothing is waiting on you. 0 threads are waiting on others."
        );
        assert_eq!(
            nothing_waiting(1),
            "Nothing is waiting on you. 1 thread is waiting on others."
        );
        assert_eq!(
            nothing_waiting(7),
            "Nothing is waiting on you. 7 threads are waiting on others."
        );
    }

    #[test]
    fn oldest_line_uses_age_words() {
        assert_eq!(
            oldest_line(4 * 86_400 + 60),
            "The oldest has waited 4 days."
        );
    }
}
