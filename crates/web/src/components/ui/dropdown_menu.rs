//! rust-ui's dropdown menu, reduced to the controlled parts Radar uses. The
//! colour classes use Radar's tokens, and nothing sets a `style` attribute or
//! runs a script, so it works under the CSP. rust-ui merges classes at run
//! time with `tw_merge`, which doubled the UI bundle, so each part takes one
//! extra class that is appended instead.

use leptos::prelude::*;

#[component]
pub fn DropdownMenu(
    #[prop(into)] open: Signal<bool>,
    #[prop(optional)] class: &'static str,
    children: ChildrenFn,
) -> impl IntoView {
    view! {
        <div
            class=format!("relative inline-block {class}")
            data-state=move || if open.get() { "open" } else { "closed" }
        >
            <Show when=move || open.get()>{children()}</Show>
        </div>
    }
}

#[component]
pub fn DropdownMenuContent(label: &'static str, children: Children) -> impl IntoView {
    view! {
        <div
            role="menu"
            aria-label=label
            class="absolute right-0 top-full z-10 mt-1 flex min-w-max flex-col gap-1 rounded-md border border-line bg-surface p-1 text-text"
        >
            {children()}
        </div>
    }
}

#[component]
pub fn DropdownMenuItem(
    #[prop(into)] on_select: Callback<()>,
    children: Children,
) -> impl IntoView {
    view! {
        <button
            type="button"
            role="menuitem"
            class="flex items-center justify-between gap-4 rounded-sm px-2 py-1 text-left text-text hover:bg-bg focus-visible:bg-bg"
            on:click=move |_| on_select.run(())
        >
            {children()}
        </button>
    }
}

#[component]
pub fn DropdownMenuShortcut(children: Children) -> impl IntoView {
    view! { <kbd class="text-caption text-faint">{children()}</kbd> }
}
