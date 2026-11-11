use domain::api::{Budget, Freshness};
use domain::ids::Timestamp;
use domain::sync_status::{DeadWork, SyncKind};
use leptos::prelude::*;

use crate::components::email_banner::EmailBanner;
use crate::store::use_store;
use crate::sync_label::{
    age_compact, attempts_words, budget_used, clock, email_silent_words, email_when, last_ok,
    source_name, source_when,
};

/// The panel the status button opens: when each source last synced, the
/// GitHub budget, and the work Radar gave up on (F10, N9). It changes nothing.
#[component]
pub fn FreshnessPanel(#[prop(into)] id: &'static str) -> impl IntoView {
    let store = use_store();
    let now = store.now_secs();
    let data = Memo::new(move |_| store.freshness().with(|f| f.data().cloned()));
    view! {
        <section id=id class="fresh-panel" aria-label="Data freshness">
            {move || match data.get() {
                None => view! { <p class="fresh-note">"Sync status is unavailable."</p> }.into_any(),
                Some(freshness) => {
                    let now = now.get();
                    view! {
                        <ul class="fresh-list" aria-label="Sources">
                            {SyncKind::ALL
                                .into_iter()
                                .map(|kind| source_row(&freshness, kind, now))
                                .collect_view()}
                            <Row name="Notification email" value=email_when(&freshness.email, now)/>
                        </ul>
                        {email_silent_words(&freshness.email, now)
                            .map(|words| view! { <EmailBanner words=words/> })}
                        <ul class="fresh-list" aria-label="GitHub budget">
                            <Row name="API budget" value=budget_words(&freshness.budget)/>
                        </ul>
                        <h2 class="fresh-heading">"Failed syncs"</h2>
                        {dead_rows(&freshness.dead, now)}
                    }
                        .into_any()
                }
            }}
        </section>
    }
}

#[component]
fn Row(name: &'static str, #[prop(into)] value: String) -> impl IntoView {
    view! {
        <li class="fresh-row">
            <span class="fresh-name">{name}</span>
            <span class="fresh-value">{value}</span>
        </li>
    }
}

fn source_row(freshness: &Freshness, kind: SyncKind, now: i64) -> impl IntoView {
    let pending = freshness
        .sources
        .iter()
        .find(|s| s.source == kind)
        .map_or(0, |s| s.pending);
    let when = source_when(last_ok(freshness, kind), now);
    let value = if pending > 0 {
        format!("{when}, {pending} waiting")
    } else {
        when
    };
    view! { <Row name=source_name(kind) value=value/> }
}

fn budget_words(budget: &Budget) -> String {
    let Some(used) = budget_used(budget) else {
        return "Not reported yet".to_owned();
    };
    match budget.reset_at {
        Some(at) => format!("{used}, resets at {}", local_clock(at)),
        None => used,
    }
}

fn local_clock(at: Timestamp) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(at.0 as f64 * 1000.0));
    clock(date.get_hours(), date.get_minutes())
}

fn dead_rows(dead: &[DeadWork], now: i64) -> impl IntoView {
    if dead.is_empty() {
        return view! { <p class="fresh-note">"No failed syncs"</p> }.into_any();
    }
    view! {
        <ul class="fresh-list" aria-label="Failed syncs">
            {dead
                .iter()
                .map(|work| {
                    view! {
                        <li class="fresh-dead" data-dead>
                            <span class="fresh-name">{source_name(work.kind)}</span>
                            <span class="fresh-key">{work.key.clone()}</span>
                            <span class="fresh-value">
                                {format!(
                                    "{}, since {} ago",
                                    attempts_words(work.attempts),
                                    age_compact(now - work.since.0),
                                )}
                            </span>
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
    .into_any()
}
