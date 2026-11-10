//! `POST /hooks/checkin`: an agent reports what it is working on.

use domain::agents::{AgentCheckin, Reported};
use domain::ids::Timestamp;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use worker::{console_log, Env, Request, Result};

use super::secret::HookSecret;
use super::{HookOutcome, HookReply};
use crate::db::agents::{self, Signal};

pub const MAX_BODY: usize = 4 * 1024;

/// Compares SHA-256 digests in constant time, so neither the token's length
/// nor its prefix leaks. Only the `Authorization` header is read: no cookie,
/// no query string.
pub fn bearer_matches(token: &HookSecret, header: Option<&str>) -> bool {
    let Some(presented) = header.and_then(|h| h.strip_prefix("Bearer ")) else {
        return false;
    };
    let presented = Sha256::digest(presented.as_bytes());
    let expected = Sha256::digest(token.as_bytes());
    presented.as_slice().ct_eq(expected.as_slice()).into()
}

pub fn parse(body: &[u8]) -> Option<(AgentCheckin, domain::agents::AgentKey)> {
    let checkin: AgentCheckin = serde_json::from_slice(body).ok()?;
    let key = checkin.key()?;
    Some((checkin, key))
}

pub async fn handle(req: &mut Request, env: &Env, now: Timestamp) -> Result<HookReply> {
    let Some(token) = HookSecret::from_env(env, "AGENT_TOKEN") else {
        return Ok(HookReply::empty(500));
    };
    let authorization = req.headers().get("authorization")?;
    if !bearer_matches(&token, authorization.as_deref()) {
        console_log!("hook checkin rejected reason=token");
        return Ok(HookReply::empty(401));
    }
    if super::declared_too_large(req, MAX_BODY)? {
        return Ok(HookReply::empty(413));
    }
    let body = req.bytes().await?;
    if body.len() > MAX_BODY {
        return Ok(HookReply::empty(413));
    }
    let Some((checkin, key)) = parse(&body) else {
        console_log!("hook checkin rejected reason=malformed");
        return Ok(HookReply::empty(400));
    };
    let outcome = match checkin.state {
        Reported::Finished => HookOutcome::Removed,
        Reported::Working | Reported::Blocked => HookOutcome::Recorded,
    };
    let signal = Signal {
        key,
        issue: checkin.issue,
        agent: Some(checkin.agent.clone()),
        reported: checkin.state,
        title: checkin.note.map(String::from),
        at: now,
    };
    agents::write(&env.d1("DB")?, &signal).await?;
    console_log!(
        "hook checkin outcome={} repo={} agent={}",
        outcome.tag(),
        checkin.repo,
        checkin.agent
    );
    Ok(HookReply::outcome(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "lane-agent-token-0123456789abcdef";

    fn token() -> HookSecret {
        HookSecret::parse(Some(TOKEN)).expect("token")
    }

    #[test]
    fn hooks_checkin_needs_the_exact_bearer_token() {
        assert!(bearer_matches(&token(), Some(&format!("Bearer {TOKEN}"))));
        for bad in [
            None,
            Some(String::new()),
            Some(TOKEN.to_owned()),
            Some(format!("bearer {TOKEN}")),
            Some(format!("Bearer {TOKEN}x")),
            Some(format!("Bearer {}", &TOKEN[1..])),
            Some("Bearer ".to_owned()),
        ] {
            assert!(!bearer_matches(&token(), bad.as_deref()), "{bad:?}");
        }
    }

    #[test]
    fn hooks_checkin_body_needs_a_branch_or_an_issue() {
        assert!(parse(br#"{"agent":"a","repo":"o/r","issue":3,"state":"working"}"#).is_some());
        assert!(parse(br#"{"agent":"a","repo":"o/r","state":"working"}"#).is_none());
        assert!(parse(b"not json").is_none());
    }
}
