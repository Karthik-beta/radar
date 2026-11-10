use domain::agents::AgentView;
use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use web_sys::Element;

use crate::api::{self, ApiError};
use crate::components::triage_list::open_tab;
use crate::heat::{age_short, age_words};
use crate::keys::{self, Key, Selection};
use crate::store::use_store;
use crate::url::GithubUrl;

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

pub const NO_AGENTS: &str = "No agents are working. They appear here when they push to an agent branch on your forks or check in.";

pub fn taken_words(others: &[String]) -> Option<String> {
    (!others.is_empty()).then(|| format!("also taken by {}", others.join(", ")))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Loading,
    Ready,
    Failed,
}

/// Agent branches and check-ins that have no pull request yet.
#[component]
pub fn Agents() -> impl IntoView {
    let store = use_store();
    let now = store.now_secs();
    let agents = RwSignal::new(Vec::<AgentView>::new());
    let status = RwSignal::new(Status::Loading);
    let selection = RwSignal::new(Selection::None);

    Effect::new(move |_| {
        store.dashboard().track();
        spawn_local(async move {
            match api::agents().await {
                Ok(page) => {
                    agents.set(page.agents);
                    status.set(Status::Ready);
                }
                Err(ApiError::Unauthorized) => store.refresh(),
                Err(_) => status.set(Status::Failed),
            }
        });
    });
    let selected = Memo::new(move |_| selection.get().clamp(agents.with(Vec::len)).index());

    let on_key = window_event_listener(ev::keydown, move |event| {
        let Some(key) = keys::from_event(&event) else {
            return;
        };
        let len = agents.with_untracked(Vec::len);
        let on_control = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .is_some_and(|el| matches!(el.tag_name().as_str(), "A" | "BUTTON"));
        match (key, selection.get_untracked().clamp(len)) {
            (Key::Down, Selection::At(i)) => selection.set(Selection::At(i + 1).clamp(len)),
            (Key::Up, Selection::At(i)) => selection.set(Selection::At(i.saturating_sub(1))),
            (Key::Open, Selection::At(i)) if !on_control => {
                let url = agents.with_untracked(|rows| {
                    rows.get(i)
                        .and_then(AgentView::url)
                        .and_then(|u| GithubUrl::parse(&u))
                });
                if let Some(url) = url {
                    open_tab(&url);
                }
            }
            _ => return,
        }
        if !on_control {
            event.prevent_default();
        }
    });
    on_cleanup(move || on_key.remove());

    view! {
        <section class="agents">
            <h1 class="mb-4 text-title font-semibold">"Agents"</h1>
            <Show when=move || status.get() == Status::Loading>
                <p class="text-muted">"Loading."</p>
            </Show>
            <Show when=move || status.get() == Status::Failed>
                <p class="text-muted" role="alert">"Could not load agent work. Radar will try again."</p>
            </Show>
            <Show when=move || agents.with(Vec::is_empty) && status.get() == Status::Ready>
                <p class="empty-state" data-empty>{NO_AGENTS}</p>
            </Show>
            <ul class="rows" aria-label="Agents">
                <For
                    each=move || agents.get().into_iter().enumerate()
                    key=|(i, a)| (*i, a.key.clone(), a.state.word(), a.also_taken_by.join(","), a.last_signal_at.0)
                    let:entry
                >
                    {
                        let (index, agent) = entry;
                        view! { <AgentRow agent=agent now=now selected=Signal::derive(move || selected.get() == Some(index))/> }
                    }
                </For>
            </ul>
        </section>
    }
}

/// State word, repo, who holds the work with its title, issue and branch, and
/// the age of the last signal.
#[component]
fn AgentRow(
    agent: AgentView,
    #[prop(into)] now: Signal<i64>,
    #[prop(into)] selected: Signal<bool>,
) -> impl IntoView {
    let at = agent.last_signal_at.0;
    let age = move || now.get().saturating_sub(at);
    let who = agent.who();
    let lead = match agent.url().as_deref().and_then(GithubUrl::parse) {
        Some(url) => view! {
            <a class="row-link" href=url.as_str().to_owned() target="_blank" rel="noopener noreferrer">{who}</a>
        }
        .into_any(),
        None => view! { <span class="row-link">{who}</span> }.into_any(),
    };
    let issue = agent
        .issue_url()
        .as_deref()
        .and_then(GithubUrl::parse)
        .zip(agent.issue)
        .map(|(url, n)| {
            view! {
                <a class="row-link" href=url.as_str().to_owned() target="_blank" rel="noopener noreferrer">
                    {format!("#{}", n.get())}
                </a>
            }
        });
    let branch = agent
        .agent
        .is_some()
        .then(|| agent.branch.as_ref().map(|b| format!(" {b}")))
        .flatten();
    view! {
        <li
            class="row"
            data-row=agent.key.clone()
            data-state=agent.state.word()
            data-selected=move || selected.get().then_some("")
            aria-current=move || selected.get().then_some("true")
        >
            <span class="row-reason text-muted">{agent.state.word()}</span>
            <span class="row-repo text-muted">{agent.repo.to_string()}</span>
            <span class="row-title">
                {lead}
                {agent.title.map(|t| view! { <span class="row-agent text-muted">{t}</span> })}
                {taken_words(&agent.also_taken_by)
                    .map(|w| view! { <span class="row-agent text-muted" data-taken>{w}</span> })}
            </span>
            <span class="row-who text-muted">{issue}{branch}</span>
            <span class="age text-muted" title=move || format!("{} ago", age_words(age()))>
                {move || age_short(age())}
            </span>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taken_words_name_every_other_holder() {
        assert_eq!(taken_words(&[]), None);
        assert_eq!(
            taken_words(&["beta".into(), "agent/x".into()]).as_deref(),
            Some("also taken by beta, agent/x")
        );
    }
}
