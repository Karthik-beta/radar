use domain::api::{ItemId, SnoozeFor, SnoozeRequest};
use domain::ids::Timestamp;
use worker::{Env, Result};

use crate::db::{local_state, run, Statement};

/// Longer bodies are refused unread. A valid one is under 20 bytes.
pub const MAX_BODY_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    Done,
    UnknownItem,
}

pub fn parse_snooze(body: &str) -> Option<SnoozeFor> {
    if body.len() > MAX_BODY_BYTES {
        return None;
    }
    serde_json::from_str::<SnoozeRequest>(body)
        .ok()
        .map(|r| r.snooze_for)
}

pub async fn dismiss(env: &Env, id: &ItemId) -> Result<Written> {
    write(env, local_state::dismiss(id.as_str())).await
}

pub async fn snooze(env: &Env, id: &ItemId, wake_at: Timestamp) -> Result<Written> {
    write(env, local_state::snooze(id.as_str(), wake_at)).await
}

/// The statement inserts from `items`, so no change means no such item.
async fn write(env: &Env, stmt: Statement) -> Result<Written> {
    let results = run(&env.d1("DB")?, vec![stmt]).await?;
    let meta = match results.first() {
        Some(result) => result.meta()?,
        None => None,
    };
    let changes = meta.and_then(|m| m.changes).unwrap_or(0);
    Ok(if changes > 0 {
        Written::Done
    } else {
        Written::UnknownItem
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snooze_body_must_name_one_day_or_one_week() {
        assert_eq!(parse_snooze(r#"{"for":"1d"}"#), Some(SnoozeFor::Day));
        assert_eq!(parse_snooze(r#"{ "for": "1w" }"#), Some(SnoozeFor::Week));
        for bad in [
            r#"{"for":"1y"}"#,
            r#"{"for":"7d"}"#,
            r#"{"until":"1d"}"#,
            r#"{"for":"1d","extra":true}"#,
            "for=1d",
            "",
        ] {
            assert_eq!(parse_snooze(bad), None, "{bad}");
        }
        let padded = format!("{{\"for\":\"1d\"}}{}", " ".repeat(MAX_BODY_BYTES));
        assert_eq!(parse_snooze(&padded), None);
    }
}
