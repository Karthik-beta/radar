use leptos::prelude::*;
use leptos_router::hooks::use_location;

use super::tabs::Tab;

#[component]
pub fn Header() -> impl IntoView {
    let pathname = use_location().pathname;
    let current = move || Tab::from_path(&pathname.get());
    view! {
        <header class="flex flex-wrap items-center gap-x-5 gap-y-2 border-b border-line px-(--gutter) py-3">
            <span class="font-semibold">"Radar"</span>
            <nav aria-label="Primary" class="-m-1 flex gap-1 p-1 phone-down:order-3 phone-down:w-full phone-down:overflow-x-auto">
                {Tab::ALL
                    .into_iter()
                    .map(|tab| {
                        let active = move || current() == Some(tab);
                        view! {
                            <a
                                href=tab.href()
                                aria-current=move || active().then_some("page")
                                class=move || {
                                    if active() {
                                        "whitespace-nowrap rounded-md bg-surface px-3 py-1 text-text"
                                    } else {
                                        "whitespace-nowrap rounded-md px-3 py-1 text-muted"
                                    }
                                }
                            >
                                {tab.label()}
                            </a>
                        }
                    })
                    .collect_view()}
            </nav>
        </header>
    }
}
