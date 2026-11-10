use leptos::html::Button;
use leptos::prelude::*;

use super::tabs::{NavGroup, Tab};
use crate::route::use_location;

const MENU_ID: &str = "more-nav";

/// The quiet second nav group, with no counts. At or below 600 px it folds into
/// one "More" button that opens the same links as a menu.
#[component]
pub fn MoreNav() -> impl IntoView {
    let pathname = use_location().path();
    let current = move || Tab::from_path(&pathname.get());
    let open = RwSignal::new(false);
    let button = NodeRef::<Button>::new();
    Effect::new(move |_| {
        pathname.track();
        open.set(false);
    });
    let escape = window_event_listener(leptos::ev::keydown, move |event| {
        if event.key() == "Escape" && open.get_untracked() {
            open.set(false);
            if let Some(button) = button.get_untracked() {
                let _ = button.focus();
            }
        }
    });
    on_cleanup(move || escape.remove());
    let menu_class = move || {
        if open.get() {
            "flex gap-1 phone-down:absolute phone-down:left-0 phone-down:top-full phone-down:z-10 phone-down:mt-1 phone-down:flex-col phone-down:rounded-md phone-down:border phone-down:border-line phone-down:bg-surface phone-down:p-1"
        } else {
            "flex gap-1 phone-down:hidden"
        }
    };
    view! {
        <div class="relative">
            <button
                type="button"
                node_ref=button
                class="tab hidden whitespace-nowrap rounded-md px-3 py-1 text-muted phone-down:block"
                aria-expanded=move || open.get().to_string()
                aria-controls=MENU_ID
                on:click=move |_| open.update(|open| *open = !*open)
            >
                "More"
            </button>
            <nav id=MENU_ID aria-label="More" class=menu_class>
                {Tab::in_group(NavGroup::More)
                    .map(|tab| {
                        let active = move || current() == Some(tab);
                        let class = move || {
                            if active() {
                                "tab whitespace-nowrap rounded-md bg-surface px-3 py-1 text-text"
                            } else {
                                "tab whitespace-nowrap rounded-md px-3 py-1 text-muted"
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
                                    {tab.label()}
                                </a>
                            }
                            .into_any()
                        } else {
                            view! { <a data-tab=tab.href() aria-disabled="true" class=class>{tab.label()}</a> }
                                .into_any()
                        }
                    })
                    .collect_view()}
            </nav>
        </div>
    }
}
