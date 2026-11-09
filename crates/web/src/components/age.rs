use domain::ids::Timestamp;
use leptos::prelude::*;

use crate::heat::{age_short, age_words, heat, Thresholds};

/// How long an item has waited, coloured by its heat. Only the age takes the
/// `warm` and `hot` colours (ADR 0003).
#[component]
pub fn Age(
    since: Timestamp,
    #[prop(into)] now: Signal<i64>,
    thresholds: Thresholds,
) -> impl IntoView {
    let age = move || now.get().saturating_sub(since.0);
    let level = move || heat(age(), thresholds);
    view! {
        <span
            class=move || format!("age {}", level().class())
            data-heat=move || level().as_str()
            title=move || format!("Waiting {}", age_words(age()))
        >
            {move || age_short(age())}
        </span>
    }
}
