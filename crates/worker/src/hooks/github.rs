//! `POST /hooks/github`: GitHub push deliveries for the operator's forks.

use domain::agents::{commit_title, AgentKey, BranchName, Reported};
use domain::ids::{RepoName, Timestamp};
use domain::me::GithubUserId;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use worker::{console_error, console_log, Env, Request, Result};

use super::secret::HookSecret;
use super::{HookOutcome, HookReply};
use crate::auth::keys::read_setting;
use crate::db::agents::{self, Signal};

pub const MAX_BODY: usize = 256 * 1024;

/// Checks `X-Hub-Signature-256` over the raw body. `verify_slice` compares in
/// constant time. Only the lowercase `sha256=<64 hex>` form GitHub sends passes.
pub fn signature_matches(key: &[u8], body: &[u8], header: Option<&str>) -> bool {
    let Some(tag) = header
        .and_then(|h| h.strip_prefix("sha256="))
        .and_then(decode_hex)
    else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&tag).is_ok()
}

fn decode_hex(hex: &str) -> Option<[u8; 32]> {
    let digit = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let bytes = hex.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (slot, [high, low]) in out.iter_mut().zip(bytes.as_chunks::<2>().0) {
        *slot = digit(*high)? << 4 | digit(*low)?;
    }
    Some(out)
}

/// The parts of a push delivery Radar reads. GitHub sends many more fields.
#[derive(Debug, Deserialize)]
pub struct Push {
    #[serde(rename = "ref")]
    git_ref: String,
    #[serde(default)]
    deleted: bool,
    repository: PushRepo,
    head_commit: Option<HeadCommit>,
}

#[derive(Debug, Deserialize)]
struct PushRepo {
    full_name: RepoName,
    owner: PushOwner,
}

#[derive(Debug, Deserialize)]
struct PushOwner {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct HeadCommit {
    message: String,
}

/// What a verified push does: an outcome, and the row to write when it counts.
pub fn decide(push: Push, allowed: GithubUserId, now: Timestamp) -> (HookOutcome, Option<Signal>) {
    if push.repository.owner.id != allowed.0 {
        return (HookOutcome::IgnoredOwner, None);
    }
    let Some(branch) = BranchName::from_agent_ref(&push.git_ref) else {
        return (HookOutcome::IgnoredRef, None);
    };
    let (outcome, reported) = if push.deleted {
        (HookOutcome::Removed, Reported::Finished)
    } else {
        (HookOutcome::Recorded, Reported::Working)
    };
    let title = match (push.deleted, push.head_commit) {
        (false, Some(commit)) => commit_title(&commit.message),
        _ => None,
    };
    let signal = Signal {
        key: AgentKey::Branch {
            repo: push.repository.full_name,
            branch,
        },
        issue: None,
        agent: None,
        reported,
        title,
        at: now,
    };
    (outcome, Some(signal))
}

pub async fn handle(req: &mut Request, env: &Env, now: Timestamp) -> Result<HookReply> {
    let Some(secret) = HookSecret::from_env(env, "WEBHOOK_SECRET") else {
        return Ok(HookReply::empty(500));
    };
    let Some(allowed) = read_setting(env, "ALLOWED_USER_ID")
        .as_deref()
        .and_then(GithubUserId::parse)
    else {
        console_error!("hook secret missing name=ALLOWED_USER_ID");
        return Ok(HookReply::empty(500));
    };
    if super::declared_too_large(req, MAX_BODY)? {
        return Ok(HookReply::empty(413));
    }
    let body = req.bytes().await?;
    if body.len() > MAX_BODY {
        return Ok(HookReply::empty(413));
    }
    let signature = req.headers().get("x-hub-signature-256")?;
    if !signature_matches(secret.as_bytes(), &body, signature.as_deref()) {
        console_log!("hook github rejected reason=signature");
        return Ok(HookReply::empty(401));
    }
    let event = req.headers().get("x-github-event")?;
    let outcome = match event.as_deref() {
        Some("ping") => HookOutcome::Pong,
        Some("push") => {
            let Ok(push) = serde_json::from_slice::<Push>(&body) else {
                console_log!("hook push rejected reason=malformed");
                return Ok(HookReply::empty(400));
            };
            let (outcome, signal) = decide(push, allowed, now);
            match &signal {
                Some(signal) => {
                    agents::write(&env.d1("DB")?, signal).await?;
                    if let AgentKey::Branch { repo, branch } = &signal.key {
                        console_log!(
                            "hook push outcome={} repo={repo} branch={branch}",
                            outcome.tag()
                        );
                    }
                }
                None => console_log!("hook push outcome={}", outcome.tag()),
            }
            outcome
        }
        _ => HookOutcome::IgnoredEvent,
    };
    Ok(HookReply::outcome(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCS_KEY: &[u8] = b"It's a Secret to Everybody";
    const DOCS_BODY: &[u8] = b"Hello, World!";
    const DOCS_SIG: &str =
        "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";

    #[test]
    fn hooks_signature_accepts_the_github_docs_vector() {
        assert!(signature_matches(DOCS_KEY, DOCS_BODY, Some(DOCS_SIG)));
    }

    #[test]
    fn hooks_signature_rejects_every_malformed_or_wrong_header() {
        let flipped = DOCS_SIG.replace("e17", "e16");
        let wrong_upper = flipped.to_uppercase().replace("SHA256=", "sha256=");
        let sha1 = "sha1=2ef7bde608ce5404e97d5f042f95f89f1c232871";
        let bare = DOCS_SIG.trim_start_matches("sha256=");
        for header in [
            Some(flipped.as_str()),
            Some(""),
            Some(sha1),
            Some(wrong_upper.as_str()),
            Some(bare),
            None,
        ] {
            assert!(
                !signature_matches(DOCS_KEY, DOCS_BODY, header),
                "{header:?}"
            );
        }
        let mut body = DOCS_BODY.to_vec();
        body[0] ^= 1;
        assert!(!signature_matches(DOCS_KEY, &body, Some(DOCS_SIG)));
    }

    fn push(owner: u64, git_ref: &str, deleted: bool) -> Push {
        let json = serde_json::json!({
            "ref": git_ref,
            "deleted": deleted,
            "repository": {"full_name": "karthik/radar", "owner": {"id": owner}},
            "head_commit": if deleted { serde_json::Value::Null } else {
                serde_json::json!({"message": "Fix the parser\n\nLong body"})
            },
        });
        serde_json::from_value(json).expect("push")
    }

    #[test]
    fn hooks_push_decides_by_owner_then_ref() {
        let me = GithubUserId(1001);
        let now = Timestamp(100);
        let (outcome, signal) = decide(push(1001, "refs/heads/agent/fix-123", false), me, now);
        assert_eq!(outcome, HookOutcome::Recorded);
        let signal = signal.expect("signal");
        assert_eq!(signal.key.to_string(), "branch:karthik/radar:agent/fix-123");
        assert_eq!(signal.title.as_deref(), Some("Fix the parser"));
        assert_eq!(signal.reported, Reported::Working);

        let (outcome, signal) = decide(push(1001, "refs/heads/agent/fix-123", true), me, now);
        assert_eq!(outcome, HookOutcome::Removed);
        assert_eq!(signal.map(|s| s.reported), Some(Reported::Finished));

        let (outcome, signal) = decide(push(1001, "refs/heads/main", false), me, now);
        assert_eq!((outcome, signal.is_none()), (HookOutcome::IgnoredRef, true));
        let (outcome, signal) = decide(push(42, "refs/heads/agent/fix-123", false), me, now);
        assert_eq!(
            (outcome, signal.is_none()),
            (HookOutcome::IgnoredOwner, true)
        );
    }
}
