use std::collections::HashMap;

use serde_json::{json, Map, Value};
use worker::{Env, Method, Request, Response, Result};

use crate::github::{query, ApiPath, Client, Conditional, Fetched, GithubError, Queried, Token};

pub const PREFIX: &str = "/__probe/";

pub async fn handle(req: &Request, env: &Env) -> Result<Response> {
    if req.method() != Method::Get {
        return reply(405, json!({"error": "method_not_allowed"}));
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
    let base = env
        .var("GITHUB_API_BASE")
        .map_err(|_| GithubError::BadConfig)?
        .to_string();
    let token = env
        .var("PROBE_TOKEN")
        .map(|v| v.to_string())
        .or_else(|_| env.secret("PROBE_TOKEN").map(|s| s.to_string()))
        .map_err(|_| GithubError::BadConfig)?;
    Client::new(&base, Token::new(token)?)
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
