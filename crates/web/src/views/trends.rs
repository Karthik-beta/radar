use domain::trends::{Trends as TrendsData, HEATMAP_WEEKS};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, ApiError};
use crate::charts::bars::WeeklyBars;
use crate::charts::heatmap::HeatMap;
use crate::charts::summary;
use crate::store::use_store;

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

#[derive(Clone, PartialEq)]
enum Status {
    Loading,
    Ready(TrendsData),
    Failed,
}

/// Pull requests opened and merged per week for a year, and a 26 week heatmap.
#[component]
pub fn Trends() -> impl IntoView {
    let store = use_store();
    let status = RwSignal::new(Status::Loading);
    spawn_local(async move {
        match api::trends().await {
            Ok(trends) => status.set(Status::Ready(trends)),
            Err(ApiError::Unauthorized) => store.refresh(),
            Err(_) => status.set(Status::Failed),
        }
    });
    view! {
        <section class="trends">
            {move || match status.get() {
                Status::Loading => view! { <p class="text-muted">"Loading."</p> }.into_any(),
                Status::Failed => {
                    view! {
                        <p class="text-muted" role="alert">
                            "Could not load trends. Reload to try again."
                        </p>
                    }
                        .into_any()
                }
                Status::Ready(trends) => {
                    view! {
                        <h1 class="mb-4 text-title font-semibold">{summary(&trends)}</h1>
                        <h2 class="mb-2 text-caption text-muted">"Pull requests per week, last 52 weeks"</h2>
                        <WeeklyBars weeks=trends.weeks.clone()/>
                        <h2 class="mb-2 mt-6 text-caption text-muted">
                            {format!("Activity per day, last {HEATMAP_WEEKS} weeks")}
                        </h2>
                        <HeatMap cells=trends.heatmap.clone()/>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}
