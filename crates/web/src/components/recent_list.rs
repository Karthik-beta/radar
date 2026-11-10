use domain::api::{Ended, RecentRow};
use leptos::prelude::*;

use crate::heat::{age_short, age_words};
use crate::url::GithubUrl;

pub const HEADING: &str = "Merged or closed in the last 14 days";
pub const NONE: &str = "Nothing merged or closed in the last 14 days.";

pub fn state_word(state: Ended) -> &'static str {
    match state {
        Ended::Merged => "Merged",
        Ended::Closed => "Closed",
    }
}

/// Closed rows take `muted`. Merged rows keep the primary text colour.
pub fn state_class(state: Ended) -> &'static str {
    match state {
        Ended::Merged => "text-text",
        Ended::Closed => "text-muted",
    }
}

/// Pull requests merged or closed in the last 14 days, newest first as the
/// API sends them.
#[component]
pub fn RecentList(
    #[prop(into)] rows: Signal<Vec<RecentRow>>,
    #[prop(into)] now: Signal<i64>,
) -> impl IntoView {
    view! {
        <section class="recent" id="recent" aria-labelledby="recent-heading">
            <h2 class="recent-heading" id="recent-heading">{HEADING}</h2>
            <Show
                when=move || rows.with(|rows| !rows.is_empty())
                fallback=|| view! { <p class="empty-state recent-none" data-empty>{NONE}</p> }
            >
                <ul class="recent-rows">
                    <For each=move || rows.get() key=|row| (row.id.clone(), row.title.clone(), row.url.clone(), state_word(row.state), row.ended_at)
                        let:row
                    >
                        {
                            let ended = row.ended_at;
                            let age = move || now.get().saturating_sub(ended.0);
                            let tone = state_class(row.state);
                            let title = match GithubUrl::parse(&row.url) {
                                Some(url) => view! {
                                    <a class="row-link" href=url.as_str().to_owned() target="_blank" rel="noopener noreferrer">
                                        {row.title.clone()}
                                    </a>
                                }
                                .into_any(),
                                None => view! { <span class="row-link">{row.title.clone()}</span> }.into_any(),
                            };
                            view! {
                                <li class=format!("recent-row {tone}") data-state=state_word(row.state)>
                                    <span class="recent-state">{state_word(row.state)}</span>
                                    <span class="recent-repo text-muted">{format!("{} #{}", row.repo, row.number)}</span>
                                    <span class="recent-title">{title}</span>
                                    <span class="recent-age text-muted" title=move || format!("{} ago", age_words(age()))>
                                        {move || age_short(age())}
                                    </span>
                                </li>
                            }
                        }
                    </For>
                </ul>
            </Show>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_none_sentence_names_the_window() {
        assert_eq!(NONE, "Nothing merged or closed in the last 14 days.");
    }

    #[test]
    fn recent_closed_rows_are_muted_and_merged_rows_are_not() {
        assert_eq!(state_word(Ended::Closed), "Closed");
        assert_eq!(state_class(Ended::Closed), "text-muted");
        assert_eq!(state_word(Ended::Merged), "Merged");
        assert_eq!(state_class(Ended::Merged), "text-text");
    }
}
