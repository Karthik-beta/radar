use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{Redirect, Route, Router, Routes};
use leptos_router::path;

use crate::auth::{self, AuthState};
use crate::components::banner::Banner;
use crate::components::header::Header;
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
    view! {
        <Router>
            {move || match state.get() {
                AuthState::Loading => ().into_any(),
                AuthState::SignedOut => view! { <SignIn/> }.into_any(),
                AuthState::SignedIn(_) if signed_out.get() => view! { <SignIn/> }.into_any(),
                AuthState::SignedIn(_) => view! { <Shell/> }.into_any(),
            }}
        </Router>
    }
}

#[component]
fn Shell() -> impl IntoView {
    crate::store::use_store().start();
    view! {
        <Header/>
        <Banner/>
        <main class="px-(--gutter) py-6">
            <Routes fallback=NotFound>
                <Route path=path!("") view=YourTurn/>
                <Route path=path!("theirs") view=TheirTurn/>
                <Route path=path!("pipeline") view=Pipeline/>
                <Route path=path!("merged") view=Merged/>
                <Route path=path!("signin") view=|| view! { <Redirect path="/"/> }/>
            </Routes>
        </main>
    }
}
