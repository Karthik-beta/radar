use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use worker::{Date, Fetch, Headers, Method, Request, RequestInit, RequestRedirect, Response};

use super::error::{from_status, GithubError};
use super::meta::{RateMeta, Timestamp, Validators};
use super::path::ApiPath;
use super::query::{validate_document, QueryDoc};

const GITHUB_API: &str = "https://api.github.com";
const GRAPHQL_PATH: &str = "/graphql";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conditional {
    None,
    IfModifiedSince(String),
    IfNoneMatch(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Fetched<T> {
    Modified {
        body: T,
        validators: Validators,
        /// The raw `Link` header, which carries the `next` page of a list.
        link: Option<String>,
        meta: RateMeta,
    },
    NotModified {
        meta: RateMeta,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Queried<T> {
    pub data: T,
    pub meta: RateMeta,
}

pub struct Token(String);

impl Token {
    pub fn new(raw: impl Into<String>) -> Result<Self, GithubError> {
        let raw = raw.into();
        if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(GithubError::BadConfig);
        }
        Ok(Self(raw))
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

#[derive(Debug)]
pub struct Client {
    base: String,
    token: Token,
}

impl Client {
    pub fn new(base: &str, token: Token) -> Result<Self, GithubError> {
        Ok(Self {
            base: parse_base(base)?,
            token,
        })
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        path: ApiPath,
        cond: Conditional,
    ) -> Result<Fetched<T>, GithubError> {
        let headers = self.headers()?;
        match &cond {
            Conditional::None => {}
            Conditional::IfModifiedSince(value) => set(&headers, "If-Modified-Since", value)?,
            Conditional::IfNoneMatch(value) => set(&headers, "If-None-Match", value)?,
        }
        let mut init = RequestInit::new();
        init.with_method(Method::Get)
            .with_headers(headers)
            .with_redirect(RequestRedirect::Manual);
        let url = format!("{}{}", self.base, path.as_str());
        let (status, response) = send(&url, &init).await?;
        let header = |name: &str| response.header(name);
        interpret_get(path.as_str(), status, &header, &response.body, now())
    }

    pub async fn query<T: DeserializeOwned>(
        &self,
        doc: &QueryDoc,
        variables: impl Serialize,
    ) -> Result<Queried<T>, GithubError> {
        validate_document(doc.text())?;
        let payload = serde_json::to_string(&Payload {
            query: doc.text(),
            variables,
        })
        .map_err(|_| GithubError::BadConfig)?;
        let headers = self.headers()?;
        set(&headers, "Content-Type", "application/json")?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(JsValue::from_str(&payload)))
            .with_redirect(RequestRedirect::Manual);
        let url = format!("{}{GRAPHQL_PATH}", self.base);
        let (status, response) = send(&url, &init).await?;
        let header = |name: &str| response.header(name);
        interpret_query(status, &header, &response.body, now())
    }

    fn headers(&self) -> Result<Headers, GithubError> {
        let headers = Headers::new();
        set(&headers, "Accept", "application/vnd.github+json")?;
        set(&headers, "X-GitHub-Api-Version", "2022-11-28")?;
        set(&headers, "User-Agent", "radar")?;
        set(
            &headers,
            "Authorization",
            &format!("Bearer {}", self.token.0),
        )?;
        Ok(headers)
    }
}

#[derive(Serialize)]
struct Payload<'a, V: Serialize> {
    query: &'a str,
    variables: V,
}

struct Received {
    headers: Headers,
    body: String,
}

impl Received {
    fn header(&self, name: &str) -> Option<String> {
        self.headers.get(name).ok().flatten()
    }
}

async fn send(url: &str, init: &RequestInit) -> Result<(u16, Received), GithubError> {
    let request = Request::new_with_init(url, init).map_err(|_| GithubError::BadConfig)?;
    let mut response: Response = Fetch::Request(request)
        .send()
        .await
        .map_err(|_| GithubError::Transport)?;
    let status = response.status_code();
    let body = response.text().await.map_err(|_| GithubError::Transport)?;
    let headers = response.headers().clone();
    Ok((status, Received { headers, body }))
}

fn set(headers: &Headers, name: &str, value: &str) -> Result<(), GithubError> {
    headers.set(name, value).map_err(|_| GithubError::BadConfig)
}

fn now() -> Timestamp {
    Timestamp(Date::now().as_millis() / 1000)
}

fn parse_base(base: &str) -> Result<String, GithubError> {
    let trimmed = base.strip_suffix('/').unwrap_or(base);
    if trimmed == GITHUB_API {
        return Ok(trimmed.to_owned());
    }
    let authority = trimmed
        .strip_prefix("http://")
        .ok_or(GithubError::BadConfig)?;
    let after_host = ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .find_map(|host| authority.strip_prefix(host))
        .ok_or(GithubError::BadConfig)?;
    let port_ok = match after_host.strip_prefix(':') {
        None => after_host.is_empty(),
        Some(port) => {
            !port.is_empty() && port.len() <= 5 && port.bytes().all(|b| b.is_ascii_digit())
        }
    };
    if !port_ok {
        return Err(GithubError::BadConfig);
    }
    Ok(trimmed.to_owned())
}

pub(crate) fn interpret_get<T: DeserializeOwned>(
    path: &str,
    status: u16,
    header: &impl Fn(&str) -> Option<String>,
    body: &str,
    now: Timestamp,
) -> Result<Fetched<T>, GithubError> {
    let meta = RateMeta::parse(header);
    match status {
        304 => Ok(Fetched::NotModified { meta }),
        200..=299 => {
            let body = serde_json::from_str(body).map_err(|_| GithubError::Parse {
                path: path.to_owned(),
            })?;
            Ok(Fetched::Modified {
                body,
                validators: Validators::parse(header),
                link: header("link"),
                meta,
            })
        }
        _ => Err(from_status(status, header, now)),
    }
}

#[derive(Deserialize)]
struct GraphqlErrors {
    errors: Option<Vec<GraphqlError>>,
}

#[derive(Deserialize)]
struct GraphqlError {
    message: String,
}

#[derive(Deserialize)]
struct GraphqlData<T> {
    data: Option<T>,
}

pub(crate) fn interpret_query<T: DeserializeOwned>(
    status: u16,
    header: &impl Fn(&str) -> Option<String>,
    body: &str,
    now: Timestamp,
) -> Result<Queried<T>, GithubError> {
    if !(200..=299).contains(&status) {
        return Err(from_status(status, header, now));
    }
    let parse_error = || GithubError::Parse {
        path: GRAPHQL_PATH.to_owned(),
    };
    let envelope: GraphqlErrors = serde_json::from_str(body).map_err(|_| parse_error())?;
    if let Some(errors) = envelope.errors.filter(|errors| !errors.is_empty()) {
        return Err(GithubError::Graphql(
            errors.into_iter().map(|e| e.message).collect(),
        ));
    }
    let GraphqlData { data } = serde_json::from_str(body).map_err(|_| parse_error())?;
    Ok(Queried {
        data: data.ok_or_else(parse_error)?,
        meta: RateMeta::parse(header),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    const NOW: Timestamp = Timestamp(1_793_900_000);

    fn headers(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| {
            owned
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }
    }

    fn get(status: u16, pairs: &[(&str, &str)], body: &str) -> Result<Fetched<Value>, GithubError> {
        interpret_get("/x", status, &headers(pairs), body, NOW)
    }

    #[test]
    fn status_304_is_not_modified() {
        let got = get(304, &[("x-ratelimit-remaining", "4999")], "");
        assert_eq!(
            got,
            Ok(Fetched::NotModified {
                meta: RateMeta {
                    remaining: Some(4999),
                    ..RateMeta::default()
                }
            })
        );
    }

    #[test]
    fn status_200_carries_body_validators_and_meta() {
        let got = get(
            200,
            &[
                ("etag", "\"abc\""),
                ("last-modified", "Thu, 05 Nov 2026 10:00:00 GMT"),
                ("x-poll-interval", "60"),
                ("link", "<https://api.github.com/x?page=2>; rel=\"next\""),
            ],
            r#"{"login":"karthik"}"#,
        );
        match got {
            Ok(Fetched::Modified {
                body,
                validators,
                link,
                meta,
            }) => {
                assert_eq!(body, json!({"login": "karthik"}));
                assert_eq!(
                    link.as_deref(),
                    Some("<https://api.github.com/x?page=2>; rel=\"next\"")
                );
                assert_eq!(validators.etag.as_deref(), Some("\"abc\""));
                assert_eq!(meta.poll_interval, Some(60));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn non_json_success_body_is_a_parse_error_naming_the_path() {
        assert_eq!(
            get(200, &[], "<html>"),
            Err(GithubError::Parse { path: "/x".into() })
        );
    }

    #[test]
    fn retry_after_on_403_is_rate_limited() {
        assert_eq!(
            get(403, &[("retry-after", "17")], "{}"),
            Err(GithubError::RateLimited {
                retry_after_secs: 17
            })
        );
    }

    #[test]
    fn retry_after_on_429_is_rate_limited() {
        assert_eq!(
            get(429, &[("retry-after", "23")], "{}"),
            Err(GithubError::RateLimited {
                retry_after_secs: 23
            })
        );
    }

    #[test]
    fn exhausted_budget_on_403_uses_reset() {
        let reset = (NOW.0 + 45).to_string();
        assert_eq!(
            get(
                403,
                &[
                    ("x-ratelimit-remaining", "0"),
                    ("x-ratelimit-reset", &reset)
                ],
                "{}"
            ),
            Err(GithubError::RateLimited {
                retry_after_secs: 45
            })
        );
        let past = (NOW.0 - 5).to_string();
        assert_eq!(
            get(
                403,
                &[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", &past)],
                "{}"
            ),
            Err(GithubError::RateLimited {
                retry_after_secs: 0
            })
        );
    }

    #[test]
    fn rate_limited_without_any_hint_waits_a_default_minute() {
        assert_eq!(
            get(429, &[], "{}"),
            Err(GithubError::RateLimited {
                retry_after_secs: 60
            })
        );
    }

    #[test]
    fn plain_403_is_forbidden() {
        assert_eq!(get(403, &[], "{}"), Err(GithubError::Forbidden));
        assert_eq!(
            get(403, &[("x-ratelimit-remaining", "4000")], "{}"),
            Err(GithubError::Forbidden)
        );
    }

    #[test]
    fn unauthorized_and_not_found_have_their_own_variants() {
        assert_eq!(get(401, &[], "{}"), Err(GithubError::Unauthorized));
        assert_eq!(get(404, &[], "{}"), Err(GithubError::NotFound));
    }

    #[test]
    fn server_error_carries_status() {
        for status in [500, 502, 503, 301, 422] {
            assert_eq!(get(status, &[], "{}"), Err(GithubError::Server(status)));
        }
    }

    fn query(status: u16, body: &str) -> Result<Queried<Value>, GithubError> {
        interpret_query(
            status,
            &headers(&[("x-ratelimit-remaining", "10")]),
            body,
            NOW,
        )
    }

    #[test]
    fn graphql_data_is_returned_with_meta() {
        let got = query(200, r#"{"data":{"viewer":{"login":"karthik"}}}"#);
        match got {
            Ok(Queried { data, meta }) => {
                assert_eq!(data, json!({"viewer": {"login": "karthik"}}));
                assert_eq!(meta.remaining, Some(10));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn graphql_errors_in_a_200_are_errors() {
        let got = query(
            200,
            r#"{"data":{"viewer":null},"errors":[{"message":"boom"},{"message":"again"}]}"#,
        );
        assert_eq!(
            got.map(|q| q.data),
            Err(GithubError::Graphql(vec!["boom".into(), "again".into()]))
        );
    }

    #[test]
    fn graphql_without_data_or_errors_is_a_parse_error() {
        assert_eq!(
            query(200, "{}").map(|q| q.data),
            Err(GithubError::Parse {
                path: "/graphql".into()
            })
        );
        assert_eq!(
            query(200, "nope").map(|q| q.data),
            Err(GithubError::Parse {
                path: "/graphql".into()
            })
        );
    }

    #[test]
    fn graphql_http_failures_use_the_status_mapping() {
        assert_eq!(
            query(502, "{}").map(|q| q.data),
            Err(GithubError::Server(502))
        );
        assert_eq!(
            query(401, "{}").map(|q| q.data),
            Err(GithubError::Unauthorized)
        );
    }

    fn token() -> Token {
        Token::new("gho_secretvalue").expect("valid token")
    }

    #[test]
    fn refuses_non_loopback_http_base() {
        for bad in [
            "http://example.com",
            "http://api.github.com",
            "http://127.0.0.1.evil.com",
            "http://127.0.0.1:80.evil.com",
            "http://127.0.0.1@evil.com",
            "http://localhost@evil.com:80",
            "http://localhost:",
            "http://localhost:8906/path",
            "http://0.0.0.0:8906",
            "https://api.github.com.evil.com",
            "https://example.com",
            "https://127.0.0.1:8906",
            "ftp://localhost",
            "api.github.com",
            "",
        ] {
            assert_eq!(
                Client::new(bad, token()).map(|_| ()),
                Err(GithubError::BadConfig),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn accepts_https_api_github_com() {
        for good in [
            "https://api.github.com",
            "https://api.github.com/",
            "http://127.0.0.1:8906",
            "http://localhost:8906/",
            "http://[::1]:8906",
            "http://localhost",
        ] {
            assert!(Client::new(good, token()).is_ok(), "{good:?}");
        }
    }

    #[test]
    fn token_debug_is_redacted() {
        let client = Client::new("https://api.github.com", token()).expect("client");
        assert!(!format!("{:?}", token()).contains("secretvalue"));
        assert!(!format!("{client:?}").contains("secretvalue"));
    }

    #[test]
    fn token_rejects_empty_and_header_breaking_values() {
        for bad in ["", "a b", "a\r\nX: y", "tok\u{e9}"] {
            assert!(Token::new(bad).is_err(), "{bad:?}");
        }
    }
}
