use std::fmt;

use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::url::form_urlencoded;
use worker::{Fetch, Headers, Method, Request, RequestInit, RequestRedirect};

use crate::github::{Client, Token};

const GITHUB_WEB: &str = "https://github.com";

/// Where the browser authorizes and the code is exchanged. Only github.com
/// over https, or a loopback stub over http for `wrangler dev` lanes, so the
/// code and client secret never go anywhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebBase(String);

impl WebBase {
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.strip_suffix('/').unwrap_or(raw);
        if trimmed == GITHUB_WEB {
            return Some(Self(trimmed.to_owned()));
        }
        // `Client::new` owns the loopback rule. Its only https base is the API
        // host, which the http prefix rules out here.
        let loopback = trimmed.starts_with("http://")
            && Token::new("unused")
                .and_then(|token| Client::new(trimmed, token))
                .is_ok();
        loopback.then(|| Self(trimmed.to_owned()))
    }

    pub fn authorize_url(&self, query: &[(&str, &str)]) -> String {
        format!("{}/login/oauth/authorize?{}", self.0, form(query))
    }

    fn token_url(&self) -> String {
        format!("{}/login/oauth/access_token", self.0)
    }
}

pub struct Grant<'a> {
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub code: &'a str,
    pub redirect_uri: &'a str,
}

/// The token GitHub issued, before it is sealed. It never prints.
pub struct AccessToken(pub(super) String);

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(<redacted>)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExchangeError {
    Transport,
    Rejected,
}

/// GitHub answers a refused code with 200 and an `error` field instead of
/// `access_token`.
#[derive(Deserialize)]
struct TokenReply {
    access_token: Option<String>,
}

/// Trades the login code for a token: the one POST to GitHub outside the
/// read-only client. It exchanges a code and never calls the GitHub API.
pub async fn exchange(base: &WebBase, grant: &Grant<'_>) -> Result<AccessToken, ExchangeError> {
    let body = form(&[
        ("client_id", grant.client_id),
        ("client_secret", grant.client_secret),
        ("code", grant.code),
        ("redirect_uri", grant.redirect_uri),
    ]);
    let headers = Headers::new();
    for (name, value) in [
        ("Accept", "application/json"),
        ("Content-Type", "application/x-www-form-urlencoded"),
        ("User-Agent", "radar"),
    ] {
        headers
            .set(name, value)
            .map_err(|_| ExchangeError::Transport)?;
    }
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&body)))
        .with_redirect(RequestRedirect::Manual);
    let request =
        Request::new_with_init(&base.token_url(), &init).map_err(|_| ExchangeError::Transport)?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|_| ExchangeError::Transport)?;
    if response.status_code() != 200 {
        return Err(ExchangeError::Rejected);
    }
    let text = response
        .text()
        .await
        .map_err(|_| ExchangeError::Transport)?;
    read_reply(&text)
}

fn read_reply(text: &str) -> Result<AccessToken, ExchangeError> {
    serde_json::from_str::<TokenReply>(text)
        .ok()
        .and_then(|reply| reply.access_token)
        .filter(|token| !token.is_empty())
        .map(AccessToken)
        .ok_or(ExchangeError::Rejected)
}

fn form(pairs: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (name, value) in pairs {
        serializer.append_pair(name, value);
    }
    serializer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_github_com_and_loopback_http() {
        for good in [
            "https://github.com",
            "https://github.com/",
            "http://127.0.0.1:8907",
            "http://localhost:8907/",
            "http://[::1]:8907",
        ] {
            assert!(WebBase::parse(good).is_some(), "{good:?}");
        }
        assert_eq!(
            WebBase::parse("https://github.com/").map(|b| b.authorize_url(&[("a", "b")])),
            Some("https://github.com/login/oauth/authorize?a=b".to_owned())
        );
    }

    #[test]
    fn refuses_any_other_base() {
        for bad in [
            "https://github.com.evil.example",
            "http://example.com",
            "http://github.com",
            "https://api.github.com",
            "https://github.com@evil.example",
            "https://github.com/evil",
            "https://gist.github.com",
            "http://127.0.0.1.evil.example",
            "http://localhost@evil.example",
            "https://127.0.0.1:8907",
            "ftp://localhost",
            "github.com",
            "",
        ] {
            assert_eq!(WebBase::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_token_reply_needs_a_non_empty_access_token() {
        assert_eq!(
            read_reply(r#"{"access_token":"gho_x","token_type":"bearer","scope":""}"#).map(|t| t.0),
            Ok("gho_x".to_owned())
        );
        for refused in [
            r#"{"error":"bad_verification_code","error_description":"x"}"#,
            r#"{"access_token":""}"#,
            "access_token=gho_x&scope=",
            "",
        ] {
            assert_eq!(
                read_reply(refused).map(|t| t.0),
                Err(ExchangeError::Rejected),
                "{refused:?}"
            );
        }
    }

    #[test]
    fn the_form_encodes_reserved_characters() {
        assert_eq!(
            form(&[
                ("code", "a&b=c"),
                ("redirect_uri", "http://x/auth/callback")
            ]),
            "code=a%26b%3Dc&redirect_uri=http%3A%2F%2Fx%2Fauth%2Fcallback"
        );
    }

    #[test]
    fn access_token_debug_is_redacted() {
        assert!(!format!("{:?}", AccessToken("gho_secret".into())).contains("gho_secret"));
    }
}
