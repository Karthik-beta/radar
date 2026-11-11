use domain::event::EventKind;
use domain::feed::{FeedCursor, FeedEvent, FeedKind, FeedRepo};
use domain::ids::RepoName;
use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use web_sys::{Element, ScrollIntoViewOptions, ScrollLogicalPosition};

use crate::api::{self, ApiError};
use crate::components::triage_list::open_tab;
use crate::heat::{age_short, age_words};
use crate::keys::{self, Key, Selection};
use crate::route::{use_location, FeedFilter};
use crate::store::use_store;
use crate::url::GithubUrl;

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

pub const NO_EVENTS: &str = "No events yet.";

/// The words after "Clear it" when a filter hides every event.
pub fn empty_tail(total: u32) -> String {
    match total {
        1 => " to see all 1 event.".to_owned(),
        n => format!(" to see all {n} events."),
    }
}

pub fn kind_word(kind: EventKind) -> String {
    kind.tag().replace('_', " ")
}

/// Only a failed check takes `red` (F2). Every other kind is a grey word.
pub fn kind_class(kind: EventKind) -> &'static str {
    match kind {
        EventKind::CiFailed => "text-red",
        _ => "text-muted",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Loading,
    Ready,
    Failed,
}

/// Every event Radar holds, newest first, 50 at a time.
#[component]
pub fn Feed() -> impl IntoView {
    let store = use_store();
    let location = use_location();
    let now = store.now_secs();
    let filter = RwSignal::new(FeedFilter::from_address());
    let events = RwSignal::new(Vec::<FeedEvent>::new());
    let repos = RwSignal::new(Vec::<FeedRepo>::new());
    let next = RwSignal::new(None::<FeedCursor>);
    let status = RwSignal::new(Status::Loading);
    let selection = RwSignal::new(Selection::None);
    let fetches = StoredValue::new(0u32);

    let fetch = move |before: Option<FeedCursor>| {
        let id = fetches.get_value() + 1;
        fetches.set_value(id);
        status.set(Status::Loading);
        let wanted = filter.get_untracked();
        spawn_local(async move {
            let result = api::feed(&wanted, before.as_ref()).await;
            if fetches.get_value() != id {
                return;
            }
            match result {
                Ok(page) => {
                    if before.is_some() {
                        events.update(|events| events.extend(page.events));
                    } else {
                        events.set(page.events);
                        repos.set(page.repos);
                        selection.set(Selection::None);
                    }
                    next.set(page.next);
                    status.set(Status::Ready);
                }
                Err(ApiError::Unauthorized) => store.refresh(),
                Err(_) => {
                    if before.is_none() {
                        events.set(Vec::new());
                    }
                    status.set(Status::Failed);
                }
            }
        });
    };
    Effect::new(move |_| {
        filter.track();
        fetch(None);
    });
    let address = location.path();
    Effect::new(move |_| {
        address.track();
        let current = FeedFilter::from_address();
        if filter.get_untracked() != current {
            filter.set(current);
        }
    });
    let choose = move |chosen: FeedFilter| {
        location.navigate(&chosen.href());
        filter.set(chosen);
    };

    let total = move || repos.with(|repos| repos.iter().map(|r| r.events).sum::<u32>());
    let repo_options = Memo::new(move |_| {
        let mut names: Vec<String> =
            repos.with(|repos| repos.iter().map(|r| r.repo.to_string()).collect());
        if let Some(repo) = filter.with(|f| f.repo.clone()) {
            let repo = repo.to_string();
            if !names.contains(&repo) {
                names.push(repo);
            }
        }
        names
    });
    let selected = Memo::new(move |_| {
        let len = events.with(Vec::len);
        selection.get().clamp(len).index()
    });

    let on_key = window_event_listener(ev::keydown, move |event| {
        let Some(key) = keys::from_event(&event) else {
            return;
        };
        let len = events.with_untracked(Vec::len);
        let current = selection.get_untracked().clamp(len);
        match (key, current) {
            (Key::Down, Selection::At(i)) => selection.set(Selection::At(i + 1).clamp(len)),
            (Key::Up, Selection::At(i)) => selection.set(Selection::At(i.saturating_sub(1))),
            (Key::Open, Selection::At(i)) if !on_control(&event) => {
                let url = events.with_untracked(|events| {
                    events.get(i).and_then(|fe| GithubUrl::parse(&fe.url))
                });
                if let Some(url) = url {
                    open_tab(&url);
                }
            }
            _ => return,
        }
        if !on_control(&event) {
            event.prevent_default();
        }
    });
    on_cleanup(move || on_key.remove());
    Effect::new(move |_| {
        if selected.get().is_some() {
            request_animation_frame(|| {
                let found = document().query_selector("[data-selected]").ok().flatten();
                if let Some(row) = found {
                    let options = ScrollIntoViewOptions::new();
                    options.set_block(ScrollLogicalPosition::Nearest);
                    row.scroll_into_view_with_scroll_into_view_options(&options);
                }
            });
        }
    });

    let select_class =
        "min-w-0 max-w-full rounded-md border border-line bg-surface px-2 py-1 text-text";
    view! {
        <section class="feed">
            <h1 class="mb-4 text-title font-semibold">"Activity"</h1>
            <div class="mb-4 flex flex-wrap gap-3">
                <select
                    aria-label="Repo"
                    class=select_class
                    on:change=move |event| {
                        let repo = RepoName::parse(&event_target_value(&event)).ok();
                        choose(FeedFilter { repo, ..filter.get_untracked() });
                    }
                >
                    <option value="" prop:selected=move || filter.with(|f| f.repo.is_none())>
                        "All repos"
                    </option>
                    {move || {
                        repo_options
                            .get()
                            .into_iter()
                            .map(|name| {
                                let picked = name.clone();
                                let label = name.clone();
                                view! {
                                    <option
                                        value=name.clone()
                                        prop:selected=move || {
                                            filter.with(|f| f.repo.as_ref().map(ToString::to_string).as_deref() == Some(picked.as_str()))
                                        }
                                    >
                                        {label}
                                    </option>
                                }
                            })
                            .collect_view()
                    }}
                </select>
                <select
                    aria-label="Kind"
                    class=select_class
                    on:change=move |event| {
                        let kind = FeedKind::parse(&event_target_value(&event));
                        choose(FeedFilter { kind, ..filter.get_untracked() });
                    }
                >
                    <option value="" prop:selected=move || filter.with(|f| f.kind.is_none())>
                        "All kinds"
                    </option>
                    {FeedKind::ALL
                        .into_iter()
                        .map(|kind| {
                            view! {
                                <option
                                    value=kind.tag()
                                    prop:selected=move || filter.with(|f| f.kind == Some(kind))
                                >
                                    {kind.tag().replace('_', " ")}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            </div>
            <Show when=move || events.with(Vec::is_empty) && status.get() == Status::Loading>
                <p class="text-muted">"Loading."</p>
            </Show>
            <Show when=move || status.get() == Status::Failed>
                <p class="text-muted" role="alert">"Could not load the feed. Reload to try again."</p>
            </Show>
            <Show when=move || events.with(Vec::is_empty) && status.get() == Status::Ready>
                <p class="empty-state" data-empty>
                    {move || {
                        if filter.with(FeedFilter::is_set) {
                            view! {
                                "Nothing matches this filter. "
                                <button
                                    type="button"
                                    class="text-text underline"
                                    on:click=move |_| choose(FeedFilter::default())
                                >
                                    "Clear it"
                                </button>
                                {empty_tail(total())}
                            }
                                .into_any()
                        } else {
                            view! { {NO_EVENTS} }.into_any()
                        }
                    }}
                </p>
            </Show>
            <ul class="rows" aria-label="Activity">
                <For
                    each=move || events.get().into_iter().enumerate()
                    key=|(index, fe)| (*index, fe.event.id.as_str().to_owned())
                    let:entry
                >
                    {
                        let (index, fe) = entry;
                        view! { <FeedRow fe=fe now=now selected=Signal::derive(move || selected.get() == Some(index))/> }
                    }
                </For>
            </ul>
            <Show when=move || next.with(Option::is_some)>
                <button
                    type="button"
                    class="mt-4 rounded-md border border-line px-3 py-1 text-text"
                    prop:disabled=move || status.get() == Status::Loading
                    on:click=move |_| fetch(next.get_untracked())
                >
                    "Load older"
                </button>
            </Show>
        </section>
    }
}

/// One event: kind, repo and number, title, actor, and age. A bot actor
/// carries a faint "bot" mark after the title.
#[component]
fn FeedRow(
    fe: FeedEvent,
    #[prop(into)] now: Signal<i64>,
    #[prop(into)] selected: Signal<bool>,
) -> impl IntoView {
    let kind = fe.event.kind;
    let at = fe.event.occurred_at;
    let age = move || now.get().saturating_sub(at.0);
    let bot = fe.event.actor.as_ref().is_some_and(|actor| actor.is_bot);
    let who = fe.event.actor.as_ref().map(|actor| actor.login.to_string());
    let place = if fe.number == 0 {
        fe.event.repo.to_string()
    } else {
        format!("{}#{}", fe.event.repo, fe.number)
    };
    let title = match GithubUrl::parse(&fe.url) {
        Some(url) => view! {
            <a class="row-link" href=url.as_str().to_owned() target="_blank" rel="noopener noreferrer">
                {fe.title}
            </a>
        }
        .into_any(),
        None => view! { <span class="row-link">{fe.title}</span> }.into_any(),
    };
    view! {
        <li
            class="row"
            data-row=fe.event.id.as_str().to_owned()
            data-kind=kind.tag()
            data-selected=move || selected.get().then_some("")
            aria-current=move || selected.get().then_some("true")
        >
            <span class=format!("row-reason {}", kind_class(kind))>{kind_word(kind)}</span>
            <span class="row-repo text-muted">{place}</span>
            <span class="row-title">
                {title}
                {bot.then(|| view! { <span class="row-agent text-faint">"bot"</span> })}
            </span>
            <span class="row-who text-muted">{who}</span>
            <span class="age text-muted" title=move || format!("{} ago", age_words(age()))>
                {move || age_short(age())}
            </span>
        </li>
    }
}

fn on_control(event: &web_sys::KeyboardEvent) -> bool {
    event
        .target()
        .and_then(|target| target.dyn_into::<Element>().ok())
        .is_some_and(|el| matches!(el.tag_name().as_str(), "A" | "BUTTON"))
}

#[cfg(test)]
mod tests {
    use domain::activity::ReviewState;

    use super::*;

    #[test]
    fn the_empty_sentence_names_every_event() {
        assert_eq!(empty_tail(12), " to see all 12 events.");
        assert_eq!(empty_tail(1), " to see all 1 event.");
    }

    #[test]
    fn only_a_failed_check_is_red() {
        let kinds = [
            EventKind::Opened,
            EventKind::Commented,
            EventKind::Reviewed(ReviewState::Approved),
            EventKind::Merged,
            EventKind::CiPassed,
            EventKind::CiFailed,
        ];
        for kind in kinds {
            let red = kind_class(kind) == "text-red";
            assert_eq!(red, kind == EventKind::CiFailed, "{}", kind.tag());
        }
    }

    #[test]
    fn the_kind_word_drops_underscores() {
        assert_eq!(kind_word(EventKind::ReviewRequested), "review requested");
        assert_eq!(kind_word(EventKind::CiFailed), "ci failed");
    }
}
