use leptos::prelude::*;

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = false;

#[component]
pub fn Pipeline() -> impl IntoView {
    view! { <p class="text-muted">"Pipeline is not built yet."</p> }
}
