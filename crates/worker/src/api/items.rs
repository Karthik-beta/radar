use domain::api::{ItemId, SnoozeFor};
use domain::ids::Timestamp;
use worker::{Env, Result};

use crate::db::dashboard::RawCount;
use crate::db::{local_state, run, Statement};

/// Longer bodies are refused unread. A valid one is under 20 bytes.
pub const MAX_BODY_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    Done,
    UnknownItem,
}

/// Reads the `SnoozeRequest` JSON, `{"for":"1d"}` or `{"for":"1w"}` with any
/// whitespace between tokens. It matches the two bodies by hand because a
/// serde_json deserializer for one struct costs the Worker about 1 KB gzipped.
pub fn parse_snooze(body: &str) -> Option<SnoozeFor> {
    if body.len() > MAX_BODY_BYTES {
        return None;
    }
    let mut tokens = String::with_capacity(body.len());
    let mut in_string = false;
    for c in body.chars() {
        match c {
            '\\' => return None,
            '"' => {
                in_string = !in_string;
                tokens.push(c);
            }
            ' ' | '\t' | '\n' | '\r' if !in_string => {}
            _ => tokens.push(c),
        }
    }
    match tokens.as_str() {
        r#"{"for":"1d"}"# => Some(SnoozeFor::Day),
        r#"{"for":"1w"}"# => Some(SnoozeFor::Week),
        _ => None,
    }
}

pub async fn dismiss(env: &Env, id: &ItemId) -> Result<Written> {
    write(env, local_state::dismiss(id.as_str())).await
}

pub async fn snooze(env: &Env, id: &ItemId, wake_at: Timestamp) -> Result<Written> {
    write(env, local_state::snooze(id.as_str(), wake_at)).await
}

/// The statement inserts from `items` and returns the row it wrote, so no row
/// means no such item.
async fn write(env: &Env, stmt: Statement) -> Result<Written> {
    let results = run(&env.d1("DB")?, vec![stmt]).await?;
    let rows = match results.first() {
        Some(result) => result.results::<RawCount>()?.len(),
        None => 0,
    };
    Ok(if rows > 0 {
        Written::Done
    } else {
        Written::UnknownItem
    })
}

#[cfg(test)]
mod tests {
    use domain::api::SnoozeRequest;

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
        for bad in [r#"{"for":"1 d"}"#, r#"{"f\u006fr":"1d"}"#, r#"{"for":"1d""#] {
            assert_eq!(parse_snooze(bad), None, "{bad}");
        }
        let padded = format!("{{\"for\":\"1d\"}}{}", " ".repeat(MAX_BODY_BYTES));
        assert_eq!(parse_snooze(&padded), None);
    }

    #[test]
    fn reads_what_the_domain_type_writes() {
        for snooze_for in [SnoozeFor::Day, SnoozeFor::Week] {
            let body = serde_json::to_string(&SnoozeRequest { snooze_for }).expect("json");
            assert_eq!(parse_snooze(&body), Some(snooze_for));
            let spaced = serde_json::to_string_pretty(&SnoozeRequest { snooze_for }).expect("json");
            assert_eq!(parse_snooze(&spaced), Some(snooze_for));
        }
    }
}
