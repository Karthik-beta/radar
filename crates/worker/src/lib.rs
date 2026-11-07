mod api;
pub mod auth;
pub mod db;
pub mod github;
#[cfg(feature = "probe")]
mod probe;
mod security;
mod sync;

use api::Route;
use worker::{
    console_error, console_log, event, Context, Env, Headers, Request, Response, Result,
    ScheduleContext, ScheduledEvent,
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
    let response = match api::route(req.method().as_ref(), &path) {
        Route::Json { status, body } => json_response(status, body)?,
        Route::Assets => env.assets("ASSETS")?.fetch_request(req).await?,
    };
    security::apply(response)
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

fn json_response(status: u16, body: String) -> Result<Response> {
    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    Ok(Response::ok(body)?
        .with_status(status)
        .with_headers(headers))
}
