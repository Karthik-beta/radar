use leptos::prelude::*;

/// The slot above every view for a page-wide notice. It shows nothing until a
/// later PR gives it something to say.
#[component]
pub fn Banner() -> impl IntoView {
    view! { <div class="banner-slot" data-banner></div> }
}
