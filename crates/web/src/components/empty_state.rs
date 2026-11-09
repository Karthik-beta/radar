use leptos::prelude::*;

/// An empty list says what is still true (ADR 0003).
#[component]
pub fn EmptyState(#[prop(into)] message: Signal<String>) -> impl IntoView {
    view! {
        <p class="empty-state" data-empty>
            {move || message.get()}
        </p>
    }
}

/// "1 thread is" or "N threads are", for empty-state sentences.
pub fn threads_are(n: u32) -> String {
    if n == 1 {
        "1 thread is".to_owned()
    } else {
        format!("{n} threads are")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threads_are_agrees_with_the_count() {
        assert_eq!(threads_are(0), "0 threads are");
        assert_eq!(threads_are(1), "1 thread is");
        assert_eq!(threads_are(7), "7 threads are");
    }
}
