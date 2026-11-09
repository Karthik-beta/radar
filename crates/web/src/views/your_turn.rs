use leptos::prelude::*;

use crate::components::count_block::CountBlock;
use crate::components::empty_state::{threads_are, EmptyState};
use crate::components::key_hints::KeyHints;
use crate::components::triage_list::{triage_list, ListKind, TriageList};
use crate::components::triage_row::RowData;
use crate::heat::{age_words, Thresholds};
use crate::store::{use_store, Load};

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
        store.dashboard().with(|d| {
            d.as_ref()
                .map(|d| d.your_turn.iter().map(RowData::from).collect())
                .unwrap_or_default()
        })
    });
    let TriageList { active, view: list } = triage_list(
        ListKind::YourTurn,
        rows,
        Signal::stored(Thresholds::YOUR_TURN),
    );

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
    let list = StoredValue::new(list);

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
