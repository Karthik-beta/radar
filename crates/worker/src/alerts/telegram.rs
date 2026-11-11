//! The one POST Radar sends to Telegram. It is separate from the GitHub client,
//! which has no write method, and it never names a GitHub host.

use domain::alert::TelegramConfig;
use serde_json::json;
use worker::wasm_bindgen::JsValue;
use worker::{Fetch, Headers, Method, Request, RequestInit, RequestRedirect};

/// What went wrong, without the URL, which holds the token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    Transport,
    Status(u16),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport => f.write_str("transport"),
            Self::Status(code) => write!(f, "status {code}"),
        }
    }
}

pub fn endpoint(base: &str, config: &TelegramConfig) -> String {
    format!(
        "{}/bot{}/sendMessage",
        base.trim_end_matches('/'),
        config.token
    )
}

pub fn body(config: &TelegramConfig, text: &str) -> String {
    json!({
        "chat_id": config.chat_id,
        "text": text,
        "link_preview_options": { "is_disabled": true },
    })
    .to_string()
}

/// A non-2xx status, `429` included, is one failed attempt and is not retried here.
pub async fn send(base: &str, config: &TelegramConfig, text: &str) -> Result<(), SendError> {
    let headers = Headers::new();
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| SendError::Transport)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&body(config, text))))
        .with_redirect(RequestRedirect::Manual);
    let request =
        Request::new_with_init(&endpoint(base, config), &init).map_err(|_| SendError::Transport)?;
    let response = Fetch::Request(request)
        .send()
        .await
        .map_err(|_| SendError::Transport)?;
    match response.status_code() {
        200..=299 => Ok(()),
        code => Err(SendError::Status(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> TelegramConfig {
        TelegramConfig {
            token: "tok".into(),
            chat_id: "42".into(),
        }
    }

    #[test]
    fn the_endpoint_joins_base_and_token() {
        assert_eq!(
            endpoint("http://127.0.0.1:9003/", &config()),
            "http://127.0.0.1:9003/bottok/sendMessage"
        );
    }

    #[test]
    fn the_body_is_plain_text_with_no_parse_mode_and_no_preview() {
        let value: serde_json::Value = serde_json::from_str(&body(&config(), "a *b* <c>")).unwrap();
        assert_eq!(value["chat_id"], "42");
        assert_eq!(value["text"], "a *b* <c>");
        assert_eq!(value["link_preview_options"]["is_disabled"], true);
        assert!(value.get("parse_mode").is_none());
        assert!(!value.to_string().contains("tok"));
    }

    #[test]
    fn a_failure_never_shows_the_token() {
        let shown = format!("{}", SendError::Status(429));
        assert!(!shown.contains("tok"));
    }
}
