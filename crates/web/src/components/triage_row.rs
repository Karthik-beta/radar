use domain::api::{Action, ItemId, TurnRow};
use domain::ids::{Login, RepoName, Timestamp};
use domain::turn::Reason;
use leptos::html::Li;
use leptos::prelude::*;
use web_sys::{ScrollIntoViewOptions, ScrollLogicalPosition};

use super::age::Age;
use crate::heat::{heat, Thresholds};
use crate::url::GithubUrl;

/// One list row as every triage view draws it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RowData {
    pub id: ItemId,
    pub url: Option<GithubUrl>,
    pub reason: Option<Reason>,
    pub repo: RepoName,
    pub number: u32,
    pub title: String,
    pub agent: bool,
    pub who: Option<String>,
    pub usual_reply: Option<i64>,
    pub since: Timestamp,
}

impl From<&TurnRow> for RowData {
    fn from(row: &TurnRow) -> Self {
        Self {
            id: row.id.clone(),
            url: GithubUrl::parse(&row.url),
            reason: Some(row.reason),
            repo: row.repo.clone(),
            number: row.number,
            title: row.title.clone(),
            agent: row.agent,
            who: who_did_what(row.reason, row.actor.as_ref(), row.what),
            usual_reply: None,
            since: row.since,
        }
    }
}

pub fn reason_label(reason: Reason) -> &'static str {
    match reason {
        Reason::ReviewRequested => "review requested",
        Reason::ChangesRequested => "changes requested",
        Reason::CiFailed => "CI failed",
        Reason::Mentioned => "mentioned",
        Reason::Assigned => "assigned",
        Reason::Replied => "replied",
    }
}

/// The `data-reason` value.
pub fn reason_key(reason: Reason) -> &'static str {
    match reason {
        Reason::ReviewRequested => "review_requested",
        Reason::ChangesRequested => "changes_requested",
        Reason::CiFailed => "ci_failed",
        Reason::Mentioned => "mentioned",
        Reason::Assigned => "assigned",
        Reason::Replied => "replied",
    }
}

pub fn action_words(what: Action) -> &'static str {
    match what {
        Action::Comment => "commented",
        Action::Review => "reviewed",
        Action::ReviewRequested => "requested your review",
        Action::Assigned => "assigned you",
        Action::Pushed => "pushed",
    }
}

/// "alice commented", or "Checks failed" when CI failed with no human.
pub fn who_did_what(reason: Reason, actor: Option<&Login>, what: Option<Action>) -> Option<String> {
    match (actor, what) {
        (Some(actor), Some(what)) => Some(format!("{actor} {}", action_words(what))),
        _ if reason == Reason::CiFailed => Some("Checks failed".to_owned()),
        _ => None,
    }
}

/// One row: reason, repo and number, title, who did what, and age on one line
/// on a laptop, three lines on a phone. Only a `CiFailed` row's reason and
/// who-line are `red` (F2). `children` renders at the row's end, for menus.
#[component]
pub fn TriageRow(
    row: RowData,
    thresholds: Thresholds,
    #[prop(into)] now: Signal<i64>,
    #[prop(into)] selected: Signal<bool>,
    #[prop(into)] leaving: Signal<bool>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let node = NodeRef::<Li>::new();
    Effect::new(move |_| {
        if selected.get() {
            if let Some(el) = node.get() {
                let options = ScrollIntoViewOptions::new();
                options.set_block(ScrollLogicalPosition::Nearest);
                el.scroll_into_view_with_scroll_into_view_options(&options);
            }
        }
    });
    let since = row.since;
    let heat_value = move || heat(now.get().saturating_sub(since.0), thresholds).as_str();
    let alarm = row.reason == Some(Reason::CiFailed);
    let tone = if alarm { "text-red" } else { "text-muted" };
    let title = match row.url {
        Some(url) => view! {
            <a class="row-link" href=url.as_str().to_owned() target="_blank" rel="noopener noreferrer">
                {row.title}
            </a>
        }
        .into_any(),
        None => view! { <span class="row-link">{row.title}</span> }.into_any(),
    };
    view! {
        <li
            node_ref=node
            class="row"
            data-row=row.id.as_str().to_owned()
            data-reason=row.reason.map(reason_key)
            data-heat=heat_value
            data-selected=move || selected.get().then_some("")
            data-leaving=move || leaving.get().then_some("")
            aria-current=move || selected.get().then_some("true")
        >
            {row.reason.map(|reason| view! { <span class=format!("row-reason {tone}")>{reason_label(reason)}</span> })}
            <span class="row-repo text-muted">{format!("{}#{}", row.repo, row.number)}</span>
            <span class="row-title">
                {title}
                {row.agent.then(|| view! { <span class="row-agent text-faint">"agent"</span> })}
            </span>
            <span class=format!("row-who {tone}")>{row.who}</span>
            <Age since=since now=now thresholds=thresholds/>
            {children.map(|children| children())}
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_did_what_names_the_actor_or_failing_checks() {
        let alice = Login::parse("alice").expect("login");
        assert_eq!(
            who_did_what(Reason::Mentioned, Some(&alice), Some(Action::Comment)).as_deref(),
            Some("alice commented")
        );
        assert_eq!(
            who_did_what(Reason::CiFailed, None, None).as_deref(),
            Some("Checks failed")
        );
        assert_eq!(who_did_what(Reason::Assigned, None, None), None);
    }
}
