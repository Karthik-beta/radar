use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::components::header::Header;
use crate::views::not_found::NotFound;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <Header/>
            <main class="px-(--gutter) py-6">
                <Routes fallback=NotFound>
                    <Route path=path!("") view=|| ()/>
                    <Route path=path!("theirs") view=|| ()/>
                    <Route path=path!("pipeline") view=|| ()/>
                </Routes>
            </main>
        </Router>
    }
}
