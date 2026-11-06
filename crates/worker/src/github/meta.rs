use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn secs_until(self, now: Timestamp) -> u64 {
        self.0.saturating_sub(now.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RateMeta {
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset: Option<Timestamp>,
    pub poll_interval: Option<u32>,
    pub scopes: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Validators {
    pub last_modified: Option<String>,
    pub etag: Option<String>,
}

impl RateMeta {
    pub fn parse(header: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            limit: number(&header, "x-ratelimit-limit"),
            remaining: number(&header, "x-ratelimit-remaining"),
            reset: number::<u64>(&header, "x-ratelimit-reset").map(Timestamp),
            poll_interval: number(&header, "x-poll-interval"),
            scopes: header("x-oauth-scopes").map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            }),
        }
    }
}

impl Validators {
    pub fn parse(header: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            last_modified: header("last-modified"),
            etag: header("etag"),
        }
    }
}

pub(crate) fn number<N: std::str::FromStr>(
    header: &impl Fn(&str) -> Option<String>,
    name: &str,
) -> Option<N> {
    header(name)?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
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

    #[test]
    fn parses_every_header() {
        let meta = RateMeta::parse(lookup(&[
            ("x-ratelimit-limit", "5000"),
            ("x-ratelimit-remaining", "4990"),
            ("x-ratelimit-reset", "1793900000"),
            ("x-poll-interval", "60"),
            ("x-oauth-scopes", "notifications, read:user"),
        ]));
        assert_eq!(
            meta,
            RateMeta {
                limit: Some(5000),
                remaining: Some(4990),
                reset: Some(Timestamp(1_793_900_000)),
                poll_interval: Some(60),
                scopes: Some(vec!["notifications".into(), "read:user".into()]),
            }
        );
    }

    #[test]
    fn missing_headers_are_none() {
        assert_eq!(RateMeta::parse(lookup(&[])), RateMeta::default());
    }

    #[test]
    fn empty_scopes_header_is_an_empty_list_not_none() {
        let meta = RateMeta::parse(lookup(&[("x-oauth-scopes", "")]));
        assert_eq!(meta.scopes, Some(vec![]));
    }

    #[test]
    fn garbage_numbers_are_none() {
        let meta = RateMeta::parse(lookup(&[
            ("x-ratelimit-limit", "lots"),
            ("x-ratelimit-remaining", "-1"),
            ("x-ratelimit-reset", "tomorrow"),
            ("x-poll-interval", "60s"),
        ]));
        assert_eq!(meta, RateMeta::default());
    }

    #[test]
    fn validators_come_from_last_modified_and_etag() {
        let v = Validators::parse(lookup(&[
            ("last-modified", "Thu, 05 Nov 2026 10:00:00 GMT"),
            ("etag", "\"abc\""),
        ]));
        assert_eq!(
            v.last_modified.as_deref(),
            Some("Thu, 05 Nov 2026 10:00:00 GMT")
        );
        assert_eq!(v.etag.as_deref(), Some("\"abc\""));
    }
}
