use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{Redirect, Route, Router, Routes};
use leptos_router::path;

use crate::auth::{self, AuthState};
use crate::components::header::Header;
use crate::views::not_found::NotFound;
use crate::views::sign_in::SignIn;

#[component]
pub fn App() -> impl IntoView {
    let state = RwSignal::new(AuthState::Loading);
    spawn_local(async move { state.set(auth::load().await) });
    view! {
        <Router>
            {move || match state.get() {
                AuthState::Loading => ().into_any(),
                AuthState::SignedOut => view! { <SignIn/> }.into_any(),
                AuthState::SignedIn(_) => view! { <Shell/> }.into_any(),
            }}
        </Router>
    }
}

#[component]
fn Shell() -> impl IntoView {
    view! {
        <Header/>
        <main class="px-(--gutter) py-6">
            <Routes fallback=NotFound>
                <Route path=path!("") view=|| ()/>
                <Route path=path!("theirs") view=|| ()/>
                <Route path=path!("pipeline") view=|| ()/>
                <Route path=path!("signin") view=|| view! { <Redirect path="/"/> }/>
            </Routes>
        </main>
    }
}
