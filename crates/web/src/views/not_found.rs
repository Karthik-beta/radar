use leptos::prelude::*;

#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <section class="flex flex-col gap-2">
            <h1 class="text-title font-semibold">"Not found"</h1>
            <p class="text-muted">"Nothing lives at this address."</p>
            <a href="/" class="text-text underline">"Back to Your turn"</a>
        </section>
    }
}
