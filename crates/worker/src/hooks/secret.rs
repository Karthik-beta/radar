use std::fmt;

use worker::{console_error, Env};

use crate::auth::keys::read_setting;

pub const MIN_SECRET_LEN: usize = 32;

/// `WEBHOOK_SECRET` or `AGENT_TOKEN`. It exists only for a value of at least
/// 32 bytes, so a route that holds one cannot run with a weak secret.
pub struct HookSecret(Vec<u8>);

impl HookSecret {
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        raw.filter(|value| value.len() >= MIN_SECRET_LEN)
            .map(|value| Self(value.as_bytes().to_vec()))
    }

    /// Logs the setting's name, never its value, when it is missing or short.
    pub fn from_env(env: &Env, name: &'static str) -> Option<Self> {
        let secret = Self::parse(read_setting(env, name).as_deref());
        if secret.is_none() {
            console_error!("hook secret missing name={name}");
        }
        secret
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for HookSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HookSecret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_secret_fails_closed() {
        assert!(HookSecret::parse(None).is_none());
    }

    #[test]
    fn empty_secret_fails_closed() {
        assert!(HookSecret::parse(Some("")).is_none());
    }

    #[test]
    fn thirty_one_byte_secret_fails_closed() {
        assert!(HookSecret::parse(Some("short-secret-0123456789abcdefgh")).is_none());
    }

    #[test]
    fn thirty_two_byte_secret_is_accepted() {
        let secret = HookSecret::parse(Some(&"s".repeat(32))).expect("accepted");
        assert_eq!(secret.as_bytes().len(), 32);
        assert_eq!(format!("{secret:?}"), "HookSecret(<redacted>)");
    }
}
