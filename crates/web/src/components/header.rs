use leptos::prelude::*;
use leptos_router::hooks::use_location;

use super::tabs::Tab;
use crate::heat::age_words;
use crate::store::{use_store, Load};

/// The header's sync line: when the last fetch succeeded, or that the last
/// refetch failed.
pub fn sync_words(load: &Load, now_ms: f64) -> Option<String> {
    if matches!(load, Load::Stale { .. }) {
        return Some("Refresh failed".to_owned());
    }
    let secs = ((now_ms - load.fetched_at()?) / 1000.0) as i64;
    Some(format!("Synced {} ago", age_words(secs)))
}

#[component]
pub fn Header() -> impl IntoView {
    let store = use_store();
    let pathname = use_location().pathname;
    let current = move || Tab::from_path(&pathname.get());
    let counters = move || store.dashboard().with(|d| d.as_ref().map(|d| d.counters));
    let now = store.now_secs();
    let sync = move || {
        store
            .load()
            .with(|load| sync_words(load, now.get() as f64 * 1000.0))
    };
    view! {
        <header class="flex flex-wrap items-center gap-x-5 gap-y-2 border-b border-line px-(--gutter) py-3">
            <span class="font-semibold">"Radar"</span>
            <nav aria-label="Primary" class="-m-1 flex gap-1 p-1 phone-down:order-3 phone-down:w-full phone-down:overflow-x-auto">
                {Tab::ALL
                    .into_iter()
                    .map(|tab| {
                        let active = move || current() == Some(tab);
                        let count = move || counters().map(|c| tab.count(&c));
                        let class = move || {
                            if active() {
                                "tab whitespace-nowrap rounded-md bg-surface px-3 py-1 text-text"
                            } else {
                                "tab whitespace-nowrap rounded-md px-3 py-1 text-muted"
                            }
                        };
                        let body = move || {
                            view! {
                                {tab.label()}
                                {move || count().map(|n| view! { <span class="tab-count" data-tab-count>{n}</span> })}
                            }
                        };
                        if tab.ready() {
                            view! {
                                <a
                                    href=tab.href()
                                    data-tab=tab.href()
                                    aria-current=move || active().then_some("page")
                                    class=class
                                >
                                    {body}
                                </a>
                            }
                            .into_any()
                        } else {
                            view! {
                                <a data-tab=tab.href() aria-disabled="true" class=class>
                                    {body}
                                </a>
                            }
                            .into_any()
                        }
                    })
                    .collect_view()}
            </nav>
            <span class="ml-auto text-caption text-faint" data-sync>{sync}</span>
        </header>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiError;

    #[test]
    fn sync_words_read_the_last_fetch() {
        assert_eq!(sync_words(&Load::Loading, 0.0), None);
        assert_eq!(sync_words(&Load::SignedOut, 0.0), None);
        assert_eq!(sync_words(&Load::Failed(ApiError::Network), 0.0), None);
    }
}
