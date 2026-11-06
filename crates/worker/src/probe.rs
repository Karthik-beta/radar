use std::collections::HashMap;

use serde_json::{json, Map, Value};
use worker::{Env, Method, Request, Response, Result};

use crate::github::{query, ApiPath, Client, Conditional, Fetched, GithubError, Queried, Token};

pub const PREFIX: &str = "/__probe/";

const SECRET_HEADER: &str = "x-probe-secret";
const MIN_SECRET_LEN: usize = 32;

pub async fn handle(req: &Request, env: &Env) -> Result<Response> {
    if req.method() != Method::Get {
        return reply(405, json!({"error": "method_not_allowed"}));
    }
    let secret = setting(env, "PROBE_SECRET");
    let presented = req.headers().get(SECRET_HEADER)?;
    if !authorized(&api_base(env), secret.as_deref(), presented.as_deref()) {
        return reply(401, json!({"error": "unauthorized"}));
    }
    let url = req.url()?;
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    match url.path() {
        "/__probe/get" => get(env, &params).await,
        "/__probe/query" => run_query(env, &params).await,
        _ => reply(404, json!({"error": "not_found"})),
    }
}

async fn get(env: &Env, params: &HashMap<String, String>) -> Result<Response> {
    let cond = match (params.get("ims"), params.get("etag")) {
        (Some(_), Some(_)) => return reply(400, json!({"error": "ims_and_etag_are_exclusive"})),
        (Some(ims), None) => Conditional::IfModifiedSince(ims.clone()),
        (None, Some(etag)) => Conditional::IfNoneMatch(etag.clone()),
        (None, None) => Conditional::None,
    };
    let raw = params.get("path").map(String::as_str).unwrap_or_default();
    let path = match ApiPath::new(raw) {
        Ok(path) => path,
        Err(_) => return reply(200, error_json("bad_path", None)),
    };
    let client = match client(env) {
        Ok(client) => client,
        Err(err) => return reply(200, error_json(err.code(), Some(&err))),
    };
    let body = match client.get::<Value>(path, cond).await {
        Ok(Fetched::Modified {
            body,
            validators,
            meta,
        }) => json!({
            "outcome": "modified",
            "status": 200,
            "validators": validators,
            "meta": meta,
            "body": body,
        }),
        Ok(Fetched::NotModified { meta }) => json!({
            "outcome": "not_modified",
            "status": 304,
            "meta": meta,
        }),
        Err(err) => error_json(err.code(), Some(&err)),
    };
    reply(200, body)
}

async fn run_query(env: &Env, params: &HashMap<String, String>) -> Result<Response> {
    let name = params.get("doc").map(String::as_str).unwrap_or_default();
    let Some(doc) = query::find(name) else {
        return reply(400, json!({"error": "unknown_document"}));
    };
    let client = match client(env) {
        Ok(client) => client,
        Err(err) => return reply(200, error_json(err.code(), Some(&err))),
    };
    let body = match client.query::<Value>(doc, json!({})).await {
        Ok(Queried { data, meta }) => json!({
            "outcome": "modified",
            "status": 200,
            "meta": meta,
            "body": data,
        }),
        Err(err) => error_json(err.code(), Some(&err)),
    };
    reply(200, body)
}

fn client(env: &Env) -> std::result::Result<Client, GithubError> {
    let token = setting(env, "PROBE_TOKEN").ok_or(GithubError::BadConfig)?;
    Client::new(&api_base(env), Token::new(token)?)
}

fn api_base(env: &Env) -> String {
    env.var("GITHUB_API_BASE")
        .map(|v| v.to_string())
        .unwrap_or_default()
}

fn setting(env: &Env, name: &str) -> Option<String> {
    env.var(name)
        .map(|v| v.to_string())
        .or_else(|_| env.secret(name).map(|s| s.to_string()))
        .ok()
}

// `Client::new` accepts only api.github.com over https or a loopback stub over http,
// so an http base is a local stub lane and needs no secret.
fn authorized(base: &str, secret: Option<&str>, presented: Option<&str>) -> bool {
    if base.starts_with("http://") {
        return true;
    }
    match (secret, presented) {
        (Some(secret), Some(presented)) => {
            secret.len() >= MIN_SECRET_LEN && constant_time_eq(secret, presented)
        }
        _ => false,
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |diff, (x, y)| diff | (x ^ y))
            == 0
}

fn error_json(code: &str, err: Option<&GithubError>) -> Value {
    let mut out = Map::new();
    out.insert("outcome".into(), json!("error"));
    out.insert("error".into(), json!(code));
    match err {
        Some(GithubError::RateLimited { retry_after_secs }) => {
            out.insert("retry_after_secs".into(), json!(retry_after_secs));
        }
        Some(GithubError::Server(status)) => {
            out.insert("status".into(), json!(status));
        }
        Some(GithubError::Parse { path }) => {
            out.insert("path".into(), json!(path));
        }
        Some(GithubError::Graphql(messages)) => {
            out.insert("messages".into(), json!(messages));
        }
        _ => {}
    }
    Value::Object(out)
}

fn reply(status: u16, body: Value) -> Result<Response> {
    Ok(Response::from_json(&body)?.with_status(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    const API: &str = "https://api.github.com";
    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn loopback_stub_lanes_need_no_secret() {
        assert!(authorized("http://127.0.0.1:8906", None, None));
        assert!(authorized("http://localhost:8906", Some("short"), None));
    }

    #[test]
    fn deployed_probe_requires_the_matching_secret() {
        assert!(authorized(API, Some(SECRET), Some(SECRET)));
        assert!(!authorized(API, Some(SECRET), None));
        assert!(!authorized(API, Some(SECRET), Some("wrong")));
        assert!(!authorized(API, Some(SECRET), Some(&SECRET[..31])));
        assert!(!authorized(API, None, Some(SECRET)));
        assert!(!authorized(API, None, None));
    }

    #[test]
    fn a_secret_shorter_than_32_bytes_never_authorizes() {
        let short = &SECRET[..31];
        assert!(!authorized(API, Some(short), Some(short)));
        assert!(!authorized(API, Some(""), Some("")));
    }

    #[test]
    fn an_unset_base_is_not_a_stub_lane() {
        assert!(!authorized("", None, None));
    }
}
