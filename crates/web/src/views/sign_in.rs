use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::auth::SignInError;

#[component]
pub fn SignIn() -> impl IntoView {
    let query = use_query_map();
    let error = move || {
        query
            .read()
            .get("error")
            .and_then(|code| SignInError::parse(&code))
    };
    view! {
        <main class="auth">
            <img class="auth-mark" src="/icon.svg" alt="" width="40" height="40"/>
            <h1 class="auth-title">"Radar"</h1>
            <p class="auth-lede">
                "Your open-source work in one list: what waits on you, and what waits on others."
            </p>
            {move || {
                error()
                    .map(|error| {
                        view! {
                            <p class="auth-error" role="alert">
                                {error.message()}
                            </p>
                        }
                    })
            }}
            <a class="auth-button" href="/auth/login" rel="external">
                "Sign in with GitHub"
            </a>
            <p class="auth-note">
                "Read-only. Radar asks for notifications and read:user, and never writes to GitHub."
            </p>
        </main>
    }
}
