use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::auth::{self, AuthState};
use crate::components::banner::Banner;
use crate::components::header::Header;
use crate::components::tabs::Tab;
use crate::route::{provide_location, use_location, Route};
use crate::store::{provide_store, Load};
use crate::views::merged::Merged;
use crate::views::not_found::NotFound;
use crate::views::pipeline::Pipeline;
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
    view! {
        <Header/>
        <Banner/>
        <main class="px-(--gutter) py-6">
            {move || match route.get() {
                Route::Tab(Tab::YourTurn) => view! { <YourTurn/> }.into_any(),
                Route::Tab(Tab::TheirTurn) => view! { <TheirTurn/> }.into_any(),
                Route::Tab(Tab::Pipeline) => view! { <Pipeline/> }.into_any(),
                Route::Tab(Tab::Merged) => view! { <Merged/> }.into_any(),
                Route::SignIn => {
                    queue_microtask(move || location.replace("/"));
                    ().into_any()
                }
                Route::NotFound => view! { <NotFound/> }.into_any(),
            }}
        </main>
    }
}
