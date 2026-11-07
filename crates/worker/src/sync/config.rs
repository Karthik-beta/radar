use super::work::KickReason;

/// Node ids per `Enrich` unit. Set from ADR 0002, Spike result, 2026-11-07: a cold
/// Syncer alarm used 9 ms of CPU and a warm one 1 to 2 ms, so the band is
/// `Syncer { batch: 10 }`.
pub const ENRICH_BATCH: usize = 10;

pub const RECONCILE_CRON: &str = "*/10 * * * *";
pub const NIGHTLY_CRON: &str = "0 3 * * *";

/// Seconds until the retry after the first and the second failure (N9). The
/// third failure marks the row dead.
pub const RETRY_DELAYS_SECS: [i64; 2] = [10, 60];
pub const MAX_ATTEMPTS: u32 = 3;

/// Below this share of the hourly limit, only polls run until the reset (P4).
pub const GATE_PERCENT: u32 = 20;
pub const BACKOFF_BASE_SECS: u64 = 60;
pub const BACKOFF_MAX_SECS: u64 = 3600;

/// Used when a response carries no `X-Poll-Interval` (P1).
pub const DEFAULT_POLL_INTERVAL_SECS: u32 = 60;

pub fn reason_for_cron(cron: &str) -> Option<KickReason> {
    match cron {
        RECONCILE_CRON => Some(KickReason::Reconcile),
        NIGHTLY_CRON => Some(KickReason::Rollup),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crons(table: &toml::Value) -> Vec<&str> {
        table["triggers"]["crons"]
            .as_array()
            .expect("crons is a list")
            .iter()
            .map(|v| v.as_str().expect("cron is a string"))
            .collect()
    }

    #[test]
    fn cron_strings_match_wrangler_toml() {
        let wrangler: toml::Value = toml::from_str(include_str!("../../../../wrangler.toml"))
            .expect("wrangler.toml parses");
        let expected = vec![RECONCILE_CRON, NIGHTLY_CRON];
        assert_eq!(crons(&wrangler), expected);
        assert_eq!(crons(&wrangler["env"]["preview"]), expected);
    }

    #[test]
    fn each_cron_maps_to_its_kick() {
        assert_eq!(reason_for_cron(RECONCILE_CRON), Some(KickReason::Reconcile));
        assert_eq!(reason_for_cron(NIGHTLY_CRON), Some(KickReason::Rollup));
        assert_eq!(reason_for_cron("* * * * *"), None);
    }
}
