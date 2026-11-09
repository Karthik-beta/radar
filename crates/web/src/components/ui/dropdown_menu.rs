//! rust-ui's dropdown menu, reduced to the controlled parts Radar uses. The
//! colour classes use Radar's tokens, and nothing sets a `style` attribute or
//! runs a script, so it works under the CSP.

use leptos::prelude::*;
use tw_merge::tw_merge;

#[component]
pub fn DropdownMenu(
    #[prop(into)] open: Signal<bool>,
    #[prop(into, optional)] class: String,
    children: ChildrenFn,
) -> impl IntoView {
    let class = tw_merge!("relative inline-block", class);
    view! {
        <div class=class data-state=move || if open.get() { "open" } else { "closed" }>
            <Show when=move || open.get()>{children()}</Show>
        </div>
    }
}

#[component]
pub fn DropdownMenuContent(
    #[prop(into)] label: String,
    #[prop(into, optional)] class: String,
    children: Children,
) -> impl IntoView {
    let class = tw_merge!(
        "absolute right-0 top-full z-10 mt-1 flex min-w-max flex-col gap-1 rounded-md border border-line bg-surface p-1 text-text",
        class
    );
    view! {
        <div role="menu" aria-label=label class=class>
            {children()}
        </div>
    }
}

#[component]
pub fn DropdownMenuItem(
    #[prop(into)] on_select: Callback<()>,
    #[prop(into, optional)] class: String,
    children: Children,
) -> impl IntoView {
    let class = tw_merge!(
        "flex items-center justify-between gap-4 rounded-sm px-2 py-1 text-left text-text hover:bg-bg focus-visible:bg-bg",
        class
    );
    view! {
        <button type="button" role="menuitem" class=class on:click=move |_| on_select.run(())>
            {children()}
        </button>
    }
}

#[component]
pub fn DropdownMenuShortcut(children: Children) -> impl IntoView {
    view! { <kbd class="text-caption text-faint">{children()}</kbd> }
}
