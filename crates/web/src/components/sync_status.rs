use leptos::ev::{keydown, KeyboardEvent};
use leptos::html::Button;
use leptos::prelude::*;

use crate::store::{use_store, FreshLoad};
use crate::sync_label::{sync_label, SyncHealth};
use crate::views::freshness::FreshnessPanel;

const PANEL_ID: &str = "data-freshness";

/// The header's status line. It is a button that opens the freshness panel.
#[component]
pub fn SyncStatus() -> impl IntoView {
    let store = use_store();
    let now = store.now_secs();
    let open = RwSignal::new(false);
    let button = NodeRef::<Button>::new();

    let label = move || {
        store.freshness().with(|fresh| match fresh {
            FreshLoad::Loading => None,
            FreshLoad::Unavailable(_) => Some(sync_label(SyncHealth::Unknown)),
            FreshLoad::Ready(data) => Some(sync_label(SyncHealth::of(data, now.get()))),
        })
    };

    let close = window_event_listener(keydown, move |event| {
        if open.get_untracked() && event.key() == "Escape" {
            open.set(false);
            if let Some(button) = button.get_untracked() {
                let _ = button.focus();
            }
        }
    });
    on_cleanup(move || close.remove());

    // The Your turn view listens on `window` for Enter and swallows it, which
    // would stop the button's own click. Keep these keys on the button.
    let keep_key = move |event: KeyboardEvent| {
        if matches!(event.key().as_str(), "Enter" | " ") {
            event.stop_propagation();
        }
    };

    view! {
        <div class="sync-status">
            <Show when=move || label().is_some()>
                <button
                    type="button"
                    node_ref=button
                    class="sync-button"
                    data-sync
                    aria-expanded=move || open.get().to_string()
                    aria-controls=PANEL_ID
                    on:click=move |_| open.update(|o| *o = !*o)
                    on:keydown=keep_key
                >
                    {label}
                </button>
            </Show>
            <Show when=move || open.get()>
                <FreshnessPanel id=PANEL_ID/>
            </Show>
        </div>
    }
}
