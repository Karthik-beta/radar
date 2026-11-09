use leptos::prelude::*;

/// The big count that opens a view, its heading, and one line under it.
#[component]
pub fn CountBlock(
    #[prop(into)] count: Signal<u32>,
    heading: &'static str,
    #[prop(into)] line: Signal<String>,
) -> impl IntoView {
    view! {
        <section class="count-block" aria-label=heading>
            <p class="count-number" data-count>{move || count.get()}</p>
            <div class="count-text">
                <h1 class="count-heading">{heading}</h1>
                <p class="count-line">{move || line.get()}</p>
            </div>
        </section>
    }
}
