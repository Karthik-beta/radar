//! The `/api/` endpoints. They read D1 and the Syncer's status, and never
//! call GitHub (N7).

mod dashboard;
mod feed;
mod items;
mod route;

use domain::error::{ErrorBody, ErrorCode};
use domain::health::{Health, HealthStatus};
use serde::Serialize;
use worker::{console_error, console_log, Env, Headers, Request, Response, Result};

use crate::auth::{self, now, SessionError};
use items::Written;
pub use route::{Endpoint, Protected};

/// A status and an optional JSON body, built without touching the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    status: u16,
    body: Option<String>,
}

impl Reply {
    fn json<T: Serialize>(status: u16, value: &T) -> Self {
        match serde_json::to_string(value) {
            Ok(body) => Self {
                status,
                body: Some(body),
            },
            Err(_) => Self::error(500, ErrorCode::Internal),
        }
    }

    fn error(status: u16, code: ErrorCode) -> Self {
        Self::json(status, &ErrorBody { error: code })
    }

    fn no_content() -> Self {
        Self {
            status: 204,
            body: None,
        }
    }

    fn into_response(self) -> Result<Response> {
        let headers = Headers::new();
        headers.set("cache-control", "no-store")?;
        let response = match self.body {
            Some(body) => {
                headers.set("content-type", "application/json")?;
                Response::ok(body)?
            }
            None => Response::empty()?,
        };
        Ok(response.with_status(self.status).with_headers(headers))
    }
}

pub async fn handle(endpoint: Endpoint, mut req: Request, env: &Env) -> Result<Response> {
    let reply = match respond(endpoint, &mut req, env).await {
        Ok(reply) => reply,
        Err(err) => {
            console_error!("api {} failed: {err}", req.path());
            Reply::error(500, ErrorCode::Internal)
        }
    };
    reply.into_response()
}

/// Settles everything that needs no D1 or Syncer call: public endpoints,
/// unknown paths, and a POST from a foreign origin.
pub fn precheck(
    endpoint: Endpoint,
    presented_origin: Option<&str>,
    own_origin: &str,
) -> std::result::Result<Protected, Reply> {
    match endpoint {
        Endpoint::Health => Err(Reply::json(
            200,
            &Health {
                status: HealthStatus::Ok,
                version: env!("CARGO_PKG_VERSION").into(),
            },
        )),
        Endpoint::WrongMethod => Err(Reply::error(405, ErrorCode::MethodNotAllowed)),
        Endpoint::NotFound => Err(Reply::error(404, ErrorCode::NotFound)),
        Endpoint::Protected(p)
            if p.is_post() && !auth::session::same_origin(presented_origin, own_origin) =>
        {
            Err(Reply::error(403, ErrorCode::Forbidden))
        }
        Endpoint::Protected(p) => Ok(p),
    }
}

async fn respond(endpoint: Endpoint, req: &mut Request, env: &Env) -> Result<Reply> {
    let presented = req.headers().get("origin")?;
    let protected = match precheck(endpoint, presented.as_deref(), &auth::origin(req)?) {
        Ok(protected) => protected,
        Err(reply) => {
            if reply.status == 403 {
                console_log!("api {} rejected reason=foreign_origin", req.path());
            }
            return Ok(reply);
        }
    };
    match auth::require_session(req, env).await {
        Ok(_) => {}
        Err(SessionError::Unavailable) => return Ok(Reply::error(500, ErrorCode::Internal)),
        Err(err) => {
            console_log!("api {} rejected reason={}", req.path(), err.name());
            return Ok(Reply::error(401, ErrorCode::Unauthorized));
        }
    }
    let now = now();
    Ok(match protected {
        Protected::Dashboard => {
            let raw = crate::db::dashboard::read(&env.d1("DB")?, now).await?;
            let (rows, errors) = raw.parse();
            for err in errors {
                console_error!("api dashboard skipped a row: {err}");
            }
            Reply::json(200, &dashboard::assemble(rows, now))
        }
        Protected::Feed => feed::respond(req, env).await?,
        Protected::Freshness => Reply {
            status: 200,
            body: Some(crate::sync::freshness_json(env).await?),
        },
        Protected::Dismiss(id) => written(items::dismiss(env, &id).await?),
        Protected::Snooze(id) => {
            let body = req.text().await?;
            match items::parse_snooze(&body) {
                Some(snooze_for) => {
                    written(items::snooze(env, &id, snooze_for.wake_at(now)).await?)
                }
                None => Reply::error(400, ErrorCode::BadRequest),
            }
        }
    })
}

fn written(outcome: Written) -> Reply {
    match outcome {
        Written::Done => Reply::no_content(),
        Written::UnknownItem => Reply::error(404, ErrorCode::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use domain::api::ItemId;

    use super::*;

    const OWN: &str = "http://127.0.0.1:8800";

    fn dismiss() -> Endpoint {
        Endpoint::Protected(Protected::Dismiss(ItemId::parse("PR_1").expect("id")))
    }

    fn body(reply: &Reply) -> &str {
        reply.body.as_deref().unwrap_or_default()
    }

    #[test]
    fn health_answers_without_a_session() {
        let reply = precheck(Endpoint::Health, None, OWN).expect_err("public");
        assert_eq!(reply.status, 200);
        assert_eq!(body(&reply), r#"{"status":"ok","version":"0.0.0"}"#);
    }

    #[test]
    fn unknown_paths_and_wrong_methods_answer_json_errors() {
        let not_found = precheck(Endpoint::NotFound, None, OWN).expect_err("404");
        assert_eq!(
            (not_found.status, body(&not_found)),
            (404, r#"{"error":"not_found"}"#)
        );
        let wrong = precheck(Endpoint::WrongMethod, None, OWN).expect_err("405");
        assert_eq!(
            (wrong.status, body(&wrong)),
            (405, r#"{"error":"method_not_allowed"}"#)
        );
    }

    #[test]
    fn a_post_from_a_foreign_or_missing_origin_is_refused_before_any_d1_call() {
        for presented in [
            Some("https://evil.example"),
            Some("http://127.0.0.1:8801"),
            None,
        ] {
            let reply = precheck(dismiss(), presented, OWN).expect_err("refused");
            assert_eq!(
                (reply.status, body(&reply)),
                (403, r#"{"error":"forbidden"}"#),
                "{presented:?}"
            );
        }
        assert!(precheck(dismiss(), Some(OWN), OWN).is_ok());
    }

    #[test]
    fn a_get_needs_no_origin() {
        let dashboard = Endpoint::Protected(Protected::Dashboard);
        assert_eq!(precheck(dashboard, None, OWN), Ok(Protected::Dashboard));
    }

    #[test]
    fn written_maps_an_unknown_item_to_not_found() {
        assert_eq!(written(Written::Done), Reply::no_content());
        assert_eq!(written(Written::UnknownItem).status, 404);
    }
}
