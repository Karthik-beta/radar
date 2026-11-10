//! The unauthenticated `/hooks/` routes. Each checks its own secret before it
//! reads the body, writes only `agent_work` in D1, and never calls GitHub.

mod checkin;
mod github;
mod route;
mod secret;

use worker::{console_error, Env, Headers, Request, Response, Result};

pub use route::Hook;

/// Serialised as its snake case tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookOutcome {
    Recorded,
    Removed,
    Pong,
    IgnoredEvent,
    IgnoredRef,
    IgnoredOwner,
}

impl HookOutcome {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::Removed => "removed",
            Self::Pong => "pong",
            Self::IgnoredEvent => "ignored_event",
            Self::IgnoredRef => "ignored_ref",
            Self::IgnoredOwner => "ignored_owner",
        }
    }
}

/// A status and, for a verified delivery, `{"outcome":"<tag>"}`. Every
/// rejection has an empty body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookReply {
    status: u16,
    outcome: Option<HookOutcome>,
}

impl HookReply {
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            outcome: None,
        }
    }

    pub fn outcome(outcome: HookOutcome) -> Self {
        Self {
            status: 200,
            outcome: Some(outcome),
        }
    }

    fn into_response(self) -> Result<Response> {
        let headers = Headers::new();
        headers.set("cache-control", "no-store")?;
        let response = match self.outcome {
            Some(outcome) => {
                headers.set("content-type", "application/json")?;
                Response::ok(format!(r#"{{"outcome":"{}"}}"#, outcome.tag()))?
            }
            None => Response::empty()?,
        };
        Ok(response.with_status(self.status).with_headers(headers))
    }
}

/// A `Content-Length` above `max` is refused before the body is read.
fn declared_too_large(req: &Request, max: usize) -> Result<bool> {
    Ok(req
        .headers()
        .get("content-length")?
        .is_some_and(|len| len.trim().parse::<usize>().map_or(true, |len| len > max)))
}

pub async fn handle(hook: Hook, mut req: Request, env: &Env) -> Result<Response> {
    let now = crate::auth::now();
    let reply = match hook {
        Hook::Github => github::handle(&mut req, env, now).await,
        Hook::Checkin => checkin::handle(&mut req, env, now).await,
        Hook::WrongMethod => Ok(HookReply::empty(405)),
        Hook::NotFound => Ok(HookReply::empty(404)),
    };
    reply
        .unwrap_or_else(|err| {
            console_error!("hook {hook:?} failed: {err}");
            HookReply::empty(500)
        })
        .into_response()
}
