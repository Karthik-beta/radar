use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::a11y::document_title;
use crate::auth::{self, AuthState};
use crate::components::banner::Banner;
use crate::components::header::Header;
use crate::components::tabs::Tab;
use crate::route::{provide_location, use_location, Route};
use crate::store::{provide_store, Load};
use crate::views::feed::Feed;
use crate::views::merged::Merged;
use crate::views::not_found::NotFound;
use crate::views::pipeline::Pipeline;
use crate::views::repos::Repos;
use crate::views::sign_in::SignIn;
use crate::views::their_turn::TheirTurn;
use crate::views::your_turn::YourTurn;

#[component]
pub fn App() -> impl IntoView {
    let state = RwSignal::new(AuthState::Loading);
    spawn_local(async move { state.set(auth::load().await) });
    let store = provide_store();
    let signed_out = Memo::new(move |_| store.load().with(|load| *load == Load::SignedOut));
    provide_location();
    move || match state.get() {
        AuthState::Loading => ().into_any(),
        AuthState::SignedOut => view! { <SignIn/> }.into_any(),
        AuthState::SignedIn(_) if signed_out.get() => view! { <SignIn/> }.into_any(),
        AuthState::SignedIn(_) => view! { <Shell/> }.into_any(),
    }
}

#[component]
fn Shell() -> impl IntoView {
    crate::store::use_store().start();
    let location = use_location();
    let path = location.path();
    let route = Memo::new(move |_| path.with(|path| Route::parse(path)));
    let store = crate::store::use_store();
    Effect::new(move |_| {
        let title = match route.get() {
            Route::Tab(tab) => {
                let count = store
                    .dashboard()
                    .with(|d| d.as_ref().map(|d| tab.count(&d.counters)));
                document_title(tab, count)
            }
            Route::SignIn | Route::NotFound => "Radar".to_owned(),
        };
        document().set_title(&title);
    });
    view! {
        <div class="sr-only" role="status" aria-live="polite" aria-atomic="true" data-live>
            {move || store.announcement().get()}
        </div>
        <Header/>
        <Banner/>
        <main class="px-(--gutter) py-6">
            {move || match route.get() {
                Route::Tab(Tab::YourTurn) => view! { <YourTurn/> }.into_any(),
                Route::Tab(Tab::TheirTurn) => view! { <TheirTurn/> }.into_any(),
                Route::Tab(Tab::Pipeline) => view! { <Pipeline/> }.into_any(),
                Route::Tab(Tab::Merged) => view! { <Merged/> }.into_any(),
                Route::Tab(Tab::Feed) => view! { <Feed/> }.into_any(),
                Route::Tab(Tab::Repos) => view! { <Repos/> }.into_any(),
                Route::SignIn => {
                    queue_microtask(move || location.replace("/"));
                    ().into_any()
                }
                Route::NotFound => view! { <NotFound/> }.into_any(),
            }}
        </main>
    }
}
