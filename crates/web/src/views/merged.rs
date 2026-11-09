use leptos::prelude::*;

use crate::components::recent_list::RecentList;
use crate::store::{use_store, Load};

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

pub fn summary_line(merged_this_month: u32) -> String {
    format!("{merged_this_month} merged this month.")
}

/// Pull requests that ended in the last 14 days.
#[component]
pub fn Merged() -> impl IntoView {
    let store = use_store();
    let now = store.now_secs();
    let dashboard = store.dashboard();
    let recent = Signal::derive(move || {
        dashboard.with(|d| d.as_ref().map(|d| d.recent.clone()).unwrap_or_default())
    });
    let merged = move || dashboard.with(|d| d.as_ref().map_or(0, |d| d.counters.merged_this_month));

    view! {
        <section class="merged">
            {move || match store.load().get() {
                Load::Loading => view! { <p class="text-muted">"Loading."</p> }.into_any(),
                Load::Failed(_) => {
                    view! { <p class="text-muted" role="alert">"Could not load your pull requests. Radar will try again."</p> }
                        .into_any()
                }
                Load::SignedOut | Load::Ready { .. } | Load::Stale { .. } => ().into_any(),
            }}
            <Show when=move || dashboard.with(Option::is_some)>
                <h1 class="summary-line">{move || summary_line(merged())}</h1>
                <RecentList rows=recent now=now/>
            </Show>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_line_names_the_month_count() {
        assert_eq!(summary_line(0), "0 merged this month.");
        assert_eq!(summary_line(7), "7 merged this month.");
    }
}
