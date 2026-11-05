use leptos::prelude::*;
use wasm_bindgen::prelude::wasm_bindgen;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Mine,
    Theirs,
    Pipeline,
}

#[derive(Clone)]
struct Item {
    id: u32,
    reason: &'static str,
    repo: &'static str,
    title: &'static str,
    who: &'static str,
    hours: u32,
}

fn items() -> Vec<Item> {
    vec![
        Item { id: 1, reason: "Changes requested", repo: "drizzle-team/drizzle-orm #4412", title: "Support the D1 batch API in the migrator", who: "tessa-r requested changes", hours: 76 },
        Item { id: 2, reason: "CI failing", repo: "biomejs/biome #6120", title: "Fix noUnusedImports false positive", who: "CI 2 checks failing", hours: 49 },
        Item { id: 3, reason: "Reply", repo: "honojs/hono #3981", title: "Keep the query string on relative redirects", who: "mkaplan asked a question", hours: 26 },
        Item { id: 4, reason: "Review", repo: "cloudflare/workers-sdk #8870", title: "Print the D1 migration plan", who: "dlin requested your review", hours: 9 },
        Item { id: 5, reason: "Mention", repo: "vitest-dev/vitest #7311", title: "pool-workers hangs on Durable Object import", who: "ana-v mentioned you", hours: 2 },
    ]
}

fn age(h: u32) -> String {
    if h < 24 { format!("{h}h") } else { format!("{}d", h / 24) }
}

#[component]
fn App() -> impl IntoView {
    let (tab, set_tab) = signal(Tab::Mine);
    let (list, set_list) = signal(items());
    let (sel, set_sel) = signal(0usize);

    let on_key = move |ev: leptos::ev::KeyboardEvent| match ev.key().as_str() {
        "j" => set_sel.update(|s| *s = (*s + 1).min(list.get().len().saturating_sub(1))),
        "k" => set_sel.update(|s| *s = s.saturating_sub(1)),
        "e" => set_list.update(|l| {
            if sel.get() < l.len() {
                l.remove(sel.get());
            }
        }),
        _ => {}
    };

    view! {
        <div class="app" tabindex="0" on:keydown=on_key>
            <header>
                <span class="mark">"Radar"</span>
                <nav>
                    <a aria-current=move || (tab.get() == Tab::Mine).then_some("page") on:click=move |_| set_tab.set(Tab::Mine)>"Your turn " {move || list.get().len()}</a>
                    <a on:click=move |_| set_tab.set(Tab::Theirs)>"Their turn 7"</a>
                    <a on:click=move |_| set_tab.set(Tab::Pipeline)>"Pipeline 9"</a>
                </nav>
            </header>
            <section class="answer">
                <span class="count">{move || list.get().len()}</span>
                <h1>"waiting on you"</h1>
            </section>
            <ul>
                <For each=move || list.get().into_iter().enumerate() key=|(_, it)| it.id let:entry>
                    <li class:sel=move || sel.get() == entry.0 class:ci={entry.1.reason == "CI failing"}>
                        <span class="why">{entry.1.reason}</span>
                        <span class="repo">{entry.1.repo}</span>
                        <span class="title">{entry.1.title}</span>
                        <span class="who">{entry.1.who}</span>
                        <span class="age" class:hot={entry.1.hours >= 72} class:warm={entry.1.hours >= 24}>{age(entry.1.hours)}</span>
                    </li>
                </For>
            </ul>
        </div>
    }
}

#[wasm_bindgen(start)]
pub fn main() {
    leptos::mount::mount_to_body(App);
}
