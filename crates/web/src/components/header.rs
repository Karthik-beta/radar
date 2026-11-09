use leptos::prelude::*;
use leptos_router::hooks::use_location;

use super::sync_status::SyncStatus;
use super::tabs::Tab;
use crate::store::use_store;

#[component]
pub fn Header() -> impl IntoView {
    let store = use_store();
    let pathname = use_location().pathname;
    let current = move || Tab::from_path(&pathname.get());
    let counters = move || store.dashboard().with(|d| d.as_ref().map(|d| d.counters));
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
            <div class="ml-auto"><SyncStatus/></div>
        </header>
    }
}
