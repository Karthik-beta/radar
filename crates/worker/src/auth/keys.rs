use std::fmt;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use domain::me::GithubUserId;
use hmac::{Hmac, KeyInit};
use sha2::Sha256;
use worker::Env;

use super::exchange::WebBase;

const MIN_SESSION_KEY_LEN: usize = 32;
const TOKEN_KEY_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    Missing(&'static str),
    Invalid(&'static str),
    SameKeys,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(name) => write!(f, "{name} is not set"),
            Self::Invalid(name) => write!(f, "{name} is not valid"),
            Self::SameKeys => f.write_str("TOKEN_KEY and SESSION_KEY must differ"),
        }
    }
}

/// The AES-256-GCM key that seals the GitHub token at rest.
pub struct TokenKey(pub(super) [u8; TOKEN_KEY_LEN]);

/// The HMAC-SHA256 key that signs the state and session cookies, keyed once
/// at parse time so signing has no failure path.
#[derive(Clone)]
pub struct SessionKey(pub(super) Hmac<Sha256>);

impl SessionKey {
    pub(super) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < MIN_SESSION_KEY_LEN {
            return None;
        }
        Hmac::<Sha256>::new_from_slice(bytes).ok().map(Self)
    }
}

impl fmt::Debug for TokenKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenKey(<redacted>)")
    }
}

impl fmt::Debug for SessionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionKey(<redacted>)")
    }
}

pub struct ClientSecret(pub(super) String);

impl fmt::Debug for ClientSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClientSecret(<redacted>)")
    }
}

/// Every setting sign-in needs, parsed once. A missing or malformed one fails
/// the whole config, so auth fails closed.
#[derive(Debug)]
pub struct AuthConfig {
    pub client_id: String,
    pub client_secret: ClientSecret,
    pub allowed: GithubUserId,
    pub token_key: TokenKey,
    pub session_key: SessionKey,
    pub web_base: WebBase,
    pub api_base: String,
}

impl AuthConfig {
    pub fn from_env(env: &Env) -> Result<Self, ConfigError> {
        Self::parse(|name| read_setting(env, name))
    }

    pub fn parse(setting: impl Fn(&'static str) -> Option<String>) -> Result<Self, ConfigError> {
        let required = |name: &'static str| {
            setting(name)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .ok_or(ConfigError::Missing(name))
        };
        let client_id = required("OAUTH_CLIENT_ID")?;
        let client_secret = ClientSecret(required("OAUTH_CLIENT_SECRET")?);
        let allowed = GithubUserId::parse(&required("ALLOWED_USER_ID")?)
            .ok_or(ConfigError::Invalid("ALLOWED_USER_ID"))?;
        let (token_key, session_key) =
            parse_keys(&required("TOKEN_KEY")?, &required("SESSION_KEY")?)?;
        let web_base = WebBase::parse(&required("GITHUB_WEB_BASE")?)
            .ok_or(ConfigError::Invalid("GITHUB_WEB_BASE"))?;
        let api_base = required("GITHUB_API_BASE")?;
        Ok(Self {
            client_id,
            client_secret,
            allowed,
            token_key,
            session_key,
            web_base,
            api_base,
        })
    }
}

pub fn parse_keys(token: &str, session: &str) -> Result<(TokenKey, SessionKey), ConfigError> {
    let token = STANDARD
        .decode(token)
        .map_err(|_| ConfigError::Invalid("TOKEN_KEY"))?;
    let session = STANDARD
        .decode(session)
        .map_err(|_| ConfigError::Invalid("SESSION_KEY"))?;
    let token: [u8; TOKEN_KEY_LEN] = token
        .try_into()
        .map_err(|_| ConfigError::Invalid("TOKEN_KEY"))?;
    let session_key =
        SessionKey::from_bytes(&session).ok_or(ConfigError::Invalid("SESSION_KEY"))?;
    if session == token {
        return Err(ConfigError::SameKeys);
    }
    Ok((TokenKey(token), session_key))
}

/// A Wrangler secret, else a variable, so `.dev.vars` and production secrets
/// both work.
pub fn read_setting(env: &Env, name: &str) -> Option<String> {
    env.secret(name)
        .map(|s| s.to_string())
        .or_else(|_| env.var(name).map(|v| v.to_string()))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    const KEY_B: &str = "ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8=";

    #[test]
    fn short_keys_are_rejected() {
        let short = STANDARD.encode([7u8; 31]);
        assert_eq!(
            parse_keys(&short, KEY_B).map(|_| ()),
            Err(ConfigError::Invalid("TOKEN_KEY"))
        );
        assert_eq!(
            parse_keys(KEY_A, &short).map(|_| ()),
            Err(ConfigError::Invalid("SESSION_KEY"))
        );
    }

    #[test]
    fn equal_keys_are_rejected() {
        assert_eq!(
            parse_keys(KEY_A, KEY_A).map(|_| ()),
            Err(ConfigError::SameKeys)
        );
    }

    #[test]
    fn bad_base64_is_rejected() {
        assert_eq!(
            parse_keys("not base64!", KEY_B).map(|_| ()),
            Err(ConfigError::Invalid("TOKEN_KEY"))
        );
        assert_eq!(
            parse_keys(KEY_A, "%%%%").map(|_| ()),
            Err(ConfigError::Invalid("SESSION_KEY"))
        );
    }

    #[test]
    fn distinct_32_byte_keys_parse_and_a_longer_session_key_is_fine() {
        assert!(parse_keys(KEY_A, KEY_B).is_ok());
        let long = STANDARD.encode([9u8; 64]);
        assert!(parse_keys(KEY_A, &long).is_ok());
    }

    #[test]
    fn a_token_key_longer_than_32_bytes_is_rejected() {
        let long = STANDARD.encode([9u8; 48]);
        assert_eq!(
            parse_keys(&long, KEY_B).map(|_| ()),
            Err(ConfigError::Invalid("TOKEN_KEY"))
        );
    }

    fn full(name: &str) -> Option<String> {
        let value = match name {
            "OAUTH_CLIENT_ID" => "stub-client",
            "OAUTH_CLIENT_SECRET" => "stub-secret",
            "ALLOWED_USER_ID" => "1001",
            "TOKEN_KEY" => KEY_A,
            "SESSION_KEY" => KEY_B,
            "GITHUB_WEB_BASE" => "http://127.0.0.1:8907",
            "GITHUB_API_BASE" => "http://127.0.0.1:8907",
            _ => return None,
        };
        Some(value.to_owned())
    }

    #[test]
    fn every_setting_is_required() {
        assert!(AuthConfig::parse(full).is_ok());
        for missing in [
            "OAUTH_CLIENT_ID",
            "OAUTH_CLIENT_SECRET",
            "ALLOWED_USER_ID",
            "TOKEN_KEY",
            "SESSION_KEY",
            "GITHUB_WEB_BASE",
            "GITHUB_API_BASE",
        ] {
            let without = |name: &'static str| (name != missing).then(|| full(name)).flatten();
            assert_eq!(
                AuthConfig::parse(without).map(|_| ()),
                Err(ConfigError::Missing(missing))
            );
            let blank = |name: &'static str| {
                if name == missing {
                    Some("  ".to_owned())
                } else {
                    full(name)
                }
            };
            assert_eq!(
                AuthConfig::parse(blank).map(|_| ()),
                Err(ConfigError::Missing(missing))
            );
        }
    }

    #[test]
    fn a_non_numeric_allowed_user_id_is_invalid() {
        let named = |name: &'static str| {
            if name == "ALLOWED_USER_ID" {
                Some("karthik".to_owned())
            } else {
                full(name)
            }
        };
        assert_eq!(
            AuthConfig::parse(named).map(|_| ()),
            Err(ConfigError::Invalid("ALLOWED_USER_ID"))
        );
    }

    #[test]
    fn config_debug_never_prints_a_secret() {
        let config = AuthConfig::parse(full).expect("config");
        let printed = format!("{config:?}");
        assert!(!printed.contains("stub-secret"), "{printed}");
        assert!(!printed.contains("[0, 1, 2"), "{printed}");
    }
}
