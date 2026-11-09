use domain::api::PipelineColumn;
use domain::settings::NudgeThresholds;
use leptos::prelude::*;

use crate::components::pipeline_card::PipelineCard;
use crate::components::recent_list::RecentList;
use crate::stage_label::{stage_key, stage_label};
use crate::store::{use_store, Load};

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

pub fn summary_line(open: usize) -> String {
    match open {
        1 => "1 open pull request.".to_owned(),
        n => format!("{n} open pull requests."),
    }
}

/// My open pull requests in a column per stage, with the recent list under the
/// board (F4, F12).
#[component]
pub fn Pipeline() -> impl IntoView {
    let store = use_store();
    let now = store.now_secs();
    let dashboard = store.dashboard();
    let columns = Memo::new(move |_| {
        dashboard.with(|d| {
            d.as_ref()
                .map(|d| d.pipeline.columns.to_vec())
                .unwrap_or_default()
        })
    });
    let recent = Signal::derive(move || {
        dashboard.with(|d| d.as_ref().map(|d| d.recent.clone()).unwrap_or_default())
    });
    let open = move || columns.with(|columns| columns.iter().map(|c| c.cards.len()).sum());

    view! {
        <section class="pipeline">
            {move || match store.load().get() {
                Load::Loading => view! { <p class="text-muted">"Loading."</p> }.into_any(),
                Load::Failed(_) => {
                    view! { <p class="text-muted" role="alert">"Could not load your pull requests. Radar will try again."</p> }
                        .into_any()
                }
                Load::SignedOut | Load::Ready { .. } | Load::Stale { .. } => ().into_any(),
            }}
            <Show when=move || dashboard.with(Option::is_some)>
                <h1 class="summary-line">{move || summary_line(open())}</h1>
                <div class="board" role="region" aria-label="Pull request pipeline" tabindex="-1">
                    <div class="board-grid">
                        {move || {
                            let nudge = dashboard
                                .with(|d| d.as_ref().map(|d| d.nudge).unwrap_or_default());
                            columns
                                .get()
                                .into_iter()
                                .map(|column| view! { <Column column=column nudge=nudge now=now/> })
                                .collect_view()
                        }}
                    </div>
                </div>
                <RecentList rows=recent now=now/>
            </Show>
        </section>
    }
}

#[component]
fn Column(
    column: PipelineColumn,
    nudge: NudgeThresholds,
    #[prop(into)] now: Signal<i64>,
) -> impl IntoView {
    let stage = column.stage;
    let label = stage_label(stage);
    let count = column.cards.len();
    let body = if column.cards.is_empty() {
        view! { <p class="column-none text-muted">"None"</p> }.into_any()
    } else {
        view! {
            <ul class="cards">
                {column
                    .cards
                    .into_iter()
                    .map(|card| view! { <PipelineCard card=card stage=stage nudge=nudge now=now/> })
                    .collect_view()}
            </ul>
        }
        .into_any()
    };
    view! {
        <section class="column" data-stage=stage_key(stage) aria-label=label>
            <h2 class="column-heading">
                <span class="column-name">{label}</span>
                <span class="column-count" data-column-count>{count}</span>
            </h2>
            {body}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_line_agrees_with_the_count() {
        assert_eq!(summary_line(0), "0 open pull requests.");
        assert_eq!(summary_line(1), "1 open pull request.");
        assert_eq!(summary_line(12), "12 open pull requests.");
    }
}
