use domain::repo_stats::{Percent, RepoCard};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, ApiError};
use crate::components::empty_state::EmptyState;
use crate::heat::age_short;
use crate::store::use_store;

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

pub const NO_REPOS: &str = "No repositories yet. They appear after the first sync.";

pub fn counts_line(card: &RepoCard) -> String {
    format!(
        "{} open, {} merged, {} closed",
        card.open, card.merged, card.closed
    )
}

pub fn merge_line(rate: Option<Percent>) -> String {
    rate.map_or_else(
        || "No merged or closed yet".to_owned(),
        |Percent(percent)| format!("{percent}% merged"),
    )
}

pub fn reply_line(median_secs: Option<i64>) -> String {
    median_secs.map_or_else(
        || "No reply yet".to_owned(),
        |secs| format!("Maintainers reply in {}", age_short(secs)),
    )
}

pub fn active_line(last_activity: i64, now: i64) -> String {
    format!(
        "Active {} ago",
        age_short(now.saturating_sub(last_activity))
    )
}

#[derive(Clone, PartialEq)]
enum Status {
    Loading,
    Ready(Vec<RepoCard>),
    Failed,
}

/// One card per repo, most recently active first (F7).
#[component]
pub fn Repos() -> impl IntoView {
    let store = use_store();
    let now = store.now_secs();
    let status = RwSignal::new(Status::Loading);
    spawn_local(async move {
        match api::repos().await {
            Ok(cards) => status.set(Status::Ready(cards)),
            Err(ApiError::Unauthorized) => store.refresh(),
            Err(_) => status.set(Status::Failed),
        }
    });
    view! {
        <section class="repos">
            <h1 class="mb-4 text-title font-semibold">"Repositories"</h1>
            {move || match status.get() {
                Status::Loading => view! { <p class="text-muted">"Loading."</p> }.into_any(),
                Status::Failed => {
                    view! { <p class="text-muted" role="alert">"Could not load the repositories. Radar will try again."</p> }
                        .into_any()
                }
                Status::Ready(cards) if cards.is_empty() => {
                    view! { <EmptyState message=NO_REPOS/> }.into_any()
                }
                Status::Ready(cards) => {
                    view! {
                        <ul class="grid grid-cols-3 gap-4 phone-down:grid-cols-1" aria-label="Repositories">
                            {cards
                                .into_iter()
                                .map(|card| view! { <RepoCardView card=card now=now/> })
                                .collect_view()}
                        </ul>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}

#[component]
fn RepoCardView(card: RepoCard, #[prop(into)] now: Signal<i64>) -> impl IntoView {
    let href = format!("https://github.com/{}", card.repo);
    let counts = counts_line(&card);
    let merge = merge_line(card.merge_rate);
    let reply = reply_line(card.median_first_response_secs);
    let last = card.last_activity.0;
    view! {
        <li class="flex flex-col gap-1 rounded-md bg-surface p-4" data-repo=card.repo.to_string()>
            <a class="row-link font-semibold" href=href target="_blank" rel="noopener noreferrer">
                {card.repo.to_string()}
            </a>
            <span class="text-muted">{counts}</span>
            <span class="text-muted">{merge}</span>
            <span class="text-muted">{reply}</span>
            <span class="text-faint">{move || active_line(last, now.get())}</span>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    #[test]
    fn a_card_reads_in_the_words_of_the_plan() {
        assert_eq!(merge_line(Some(Percent(86))), "86% merged");
        assert_eq!(reply_line(Some(2 * DAY)), "Maintainers reply in 2d");
        assert_eq!(active_line(100, 100 + 3 * DAY), "Active 3d ago");
    }

    #[test]
    fn nothing_measured_reads_as_words_never_a_false_zero() {
        assert_eq!(merge_line(None), "No merged or closed yet");
        assert_eq!(reply_line(None), "No reply yet");
    }
}
