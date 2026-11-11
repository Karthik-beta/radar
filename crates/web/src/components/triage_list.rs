use domain::api::{ItemId, SnoozeFor};
use leptos::prelude::*;

use super::snooze_menu::SnoozeMenu;
use super::triage_row::{RowData, TriageRow};
use crate::a11y::{live_text, Announce};
use crate::heat::Thresholds;
use crate::keys::{self, Effect, Key, Mode, State};
use crate::store::{use_store, Triage};
use crate::url::GithubUrl;

/// Which triage list a view draws. Both share one row list, so the UI bundle
/// carries one copy of the keyed list and its key handling.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    YourTurn,
    TheirTurn,
}

impl ListKind {
    fn label(self) -> &'static str {
        match self {
            Self::YourTurn => "Waiting on you",
            Self::TheirTurn => "Waiting on others",
        }
    }

    fn dismisses(self) -> bool {
        self == Self::YourTurn
    }

    fn usual_reply(self) -> bool {
        self == Self::TheirTurn
    }
}

const USUAL_REPLY: &str = "-";

/// The rows a triage view lists, and the list itself.
pub struct TriageList {
    /// The rows not sliding out, in order. The keys act on these.
    pub active: Memo<Vec<RowData>>,
    pub view: ViewFn,
}

/// Sets up selection, the snooze menu, and the triage keys for `rows`, which
/// last until the calling view unmounts.
pub fn triage_list(
    kind: ListKind,
    rows: Memo<Vec<RowData>>,
    thresholds: Signal<Thresholds>,
) -> TriageList {
    let store = use_store();
    let now = store.now_secs();
    let active = Memo::new(move |_| {
        rows.with(|rows| {
            rows.iter()
                .filter(|row| !store.is_leaving(&row.id))
                .cloned()
                .collect::<Vec<RowData>>()
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

    let announce = move |store: &crate::store::Store, action: Announce| {
        let left = active.with_untracked(Vec::len);
        store.announce(live_text(action, left));
    };
    let press_on = move |key: Key, target: Option<ItemId>| {
        let state = keys_state.get_untracked();
        let (next, effect) = active.with_untracked(|rows| match &target {
            Some(id) => keys::step_on(state, id, key, rows),
            None => keys::step(state, key, rows),
        });
        if matches!(effect, Some(Effect::Dismiss(_))) && !kind.dismisses() {
            return;
        }
        keys_state.set(next);
        match effect {
            Some(Effect::Open(url)) => open_tab(&url),
            Some(Effect::Dismiss(id)) => {
                store.triage(id, Triage::Dismiss);
                announce(&store, Announce::Dismissed);
            }
            Some(Effect::Snooze(id, snooze_for)) => {
                store.triage(id, Triage::Snooze(snooze_for));
                announce(&store, Announce::Snoozed(snooze_for));
            }
            None => {}
        }
    };
    let press = move |key: Key| press_on(key, None);
    keys::listen(press);

    let view = ViewFn::from(move || {
        view! {
            <ul class="rows" aria-label=kind.label()>
                <For
                    each=move || {
                        let th = thresholds.get();
                        rows.get().into_iter().map(|row| (row, th)).collect::<Vec<_>>()
                    }
                    key=|(row, th)| (row.clone(), th.warm, th.hot)
                    let:entry
                >
                    {
                        let (row, th) = entry;
                        let id = row.id.clone();
                        let is = move |other: Option<ItemId>| other.as_ref() == Some(&id);
                        let (sel_id, snooze_id, leave_id) = (is.clone(), is.clone(), row.id.clone());
                        let open_menu = Signal::derive(move || snooze_id(snoozing.get()));
                        let (snooze_label, dismiss_label) = (
                            format!("Snooze {}#{}", row.repo, row.number),
                            format!("Dismiss {}#{}", row.repo, row.number),
                        );
                        let (snooze_target, dismiss_target) = (row.id.clone(), row.id.clone());
                        view! {
                            <TriageRow
                                row=row
                                thresholds=th
                                now=now
                                selected=Signal::derive(move || sel_id(selected.get()))
                                leaving=Signal::derive(move || store.is_leaving(&leave_id))
                            >
                                {kind.usual_reply().then(|| view! { <span class="row-usual text-faint">{USUAL_REPLY}</span> })}
                                <span class="row-actions">
                                    <button
                                        type="button"
                                        class="row-action"
                                        aria-label=snooze_label
                                        aria-haspopup="menu"
                                        aria-expanded=move || open_menu.get().to_string()
                                        on:click=move |_| {
                                            let key = if open_menu.get_untracked() { Key::Escape } else { Key::Snooze };
                                            press_on(key, Some(snooze_target.clone()))
                                        }
                                    >
                                        "Snooze"
                                    </button>
                                    {kind.dismisses().then(|| view! {
                                        <button
                                            type="button"
                                            class="row-action"
                                            aria-label=dismiss_label
                                            on:click=move |_| press_on(Key::Dismiss, Some(dismiss_target.clone()))
                                        >
                                            "Dismiss"
                                        </button>
                                    })}
                                </span>
                                <SnoozeMenu
                                    open=open_menu
                                    on_pick=move |snooze_for| {
                                        press(match snooze_for {
                                            SnoozeFor::Day => Key::Day,
                                            SnoozeFor::Week => Key::Week,
                                        })
                                    }
                                />
                            </TriageRow>
                        }
                    }
                </For>
            </ul>
        }
    });
    TriageList { active, view }
}

pub fn open_tab(url: &GithubUrl) {
    let _ = window().open_with_url_and_target_and_features(
        url.as_str(),
        "_blank",
        "noopener,noreferrer",
    );
}
