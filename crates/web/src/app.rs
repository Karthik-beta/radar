use domain::health::{Health, HealthStatus};
use gloo_net::http::Request;
use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Api {
    Ok,
    Unreachable,
}

#[component]
pub fn App() -> impl IntoView {
    let api = LocalResource::new(check_api);
    let label = move || match api.get() {
        None => "Checking API",
        Some(Api::Ok) => "API ok",
        Some(Api::Unreachable) => "API unreachable",
    };
    view! {
        <main>
            <h1>"Radar"</h1>
            <p>{label}</p>
        </main>
    }
}

async fn check_api() -> Api {
    let Ok(response) = Request::get("/api/health").send().await else {
        return Api::Unreachable;
    };
    match response.json::<Health>().await {
        Ok(Health {
            status: HealthStatus::Ok,
            ..
        }) if response.ok() => Api::Ok,
        _ => Api::Unreachable,
    }
}
