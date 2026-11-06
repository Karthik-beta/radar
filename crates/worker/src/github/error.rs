use std::fmt;

use super::meta::{number, Timestamp};

const DEFAULT_RETRY_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubError {
    Unauthorized,
    Forbidden,
    NotFound,
    RateLimited { retry_after_secs: u64 },
    Server(u16),
    Transport,
    Parse { path: String },
    Graphql(Vec<String>),
    BadConfig,
}

impl GithubError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::RateLimited { .. } => "rate_limited",
            Self::Server(_) => "server",
            Self::Transport => "transport",
            Self::Parse { .. } => "parse",
            Self::Graphql(_) => "graphql",
            Self::BadConfig => "bad_config",
        }
    }
}

impl fmt::Display for GithubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimited { retry_after_secs } => {
                write!(f, "rate_limited retry_after_secs={retry_after_secs}")
            }
            Self::Server(status) => write!(f, "server status={status}"),
            Self::Parse { path } => write!(f, "parse path={path}"),
            Self::Graphql(messages) => write!(f, "graphql {}", messages.join("; ")),
            other => f.write_str(other.code()),
        }
    }
}

impl std::error::Error for GithubError {}

pub(crate) fn from_status(
    status: u16,
    header: &impl Fn(&str) -> Option<String>,
    now: Timestamp,
) -> GithubError {
    match status {
        401 => GithubError::Unauthorized,
        404 => GithubError::NotFound,
        403 | 429 => rate_limit_or_forbidden(status, header, now),
        other => GithubError::Server(other),
    }
}

fn rate_limit_or_forbidden(
    status: u16,
    header: &impl Fn(&str) -> Option<String>,
    now: Timestamp,
) -> GithubError {
    if let Some(secs) = number::<u64>(header, "retry-after") {
        return GithubError::RateLimited {
            retry_after_secs: secs,
        };
    }
    let exhausted = number::<u32>(header, "x-ratelimit-remaining") == Some(0);
    if status == 403 && !exhausted {
        return GithubError::Forbidden;
    }
    let secs = number::<u64>(header, "x-ratelimit-reset")
        .map(|reset| Timestamp(reset).secs_until(now))
        .unwrap_or(DEFAULT_RETRY_SECS);
    GithubError::RateLimited {
        retry_after_secs: secs,
    }
}
