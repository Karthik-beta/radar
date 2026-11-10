mod api;
pub mod auth;
pub mod db;
mod email;
pub mod github;
mod hooks;
#[cfg(feature = "probe")]
mod probe;
mod rollup;
mod security;
mod sync;

use worker::{
    console_error, console_log, event, Context, Env, Request, Response, Result, ScheduleContext,
    ScheduledEvent,
};

pub use sync::Syncer;

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let path = req.path();
    #[cfg(feature = "probe")]
    {
        if path.starts_with(probe::PREFIX) {
            return security::apply(probe::handle(&req, &env).await?);
        }
    }
    if let Some(route) = auth::Route::parse(&req.method(), &path) {
        return security::apply(auth::handle(route, &req, &env).await?);
    }
    if let Some(hook) = hooks::Hook::parse(&req.method(), &path) {
        return security::apply(hooks::handle(hook, req, &env).await?);
    }
    if let Some(endpoint) = api::Endpoint::parse(&req.method(), &path) {
        return security::apply(api::handle(endpoint, req, &env).await?);
    }
    security::apply(env.assets("ASSETS")?.fetch_request(req).await?)
}

#[event(scheduled)]
async fn scheduled(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    let cron = event.cron();
    let Some(reason) = sync::reason_for_cron(&cron) else {
        console_error!("scheduled cron={cron} matches no kick");
        return;
    };
    console_log!("scheduled {}", reason.as_str());
    if let Err(err) = sync::kick(&env, reason).await {
        console_error!("syncer kick failed: {err}");
    }
}
