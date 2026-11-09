use leptos::prelude::*;

/// A row of keys and what each does, as `(key, action)` pairs.
#[component]
pub fn KeyHints(hints: &'static [(&'static str, &'static str)]) -> impl IntoView {
    view! {
        <ul class="key-hints" aria-label="Keyboard shortcuts">
            {hints
                .iter()
                .map(|(key, action)| {
                    view! {
                        <li>
                            <kbd>{*key}</kbd>
                            " "
                            {*action}
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
}
