//! `GET /api/stats?period=12m&grain=week|month` or `?period=yoy`.

use domain::error::ErrorCode;
use domain::ids::Timestamp;
use domain::stats::{build_stats, Period, Stats, StatsInput};
use serde_json::Value;
use worker::{console_error, console_log, Env, Request, Result};

use super::Reply;
use crate::db::stats::{self, parse_merge, parse_pr, parse_review, parse_transition, RawStats};
use crate::db::RowError;

/// The period the query names, or `None` for a missing or unknown value.
pub fn period_of(pairs: &[(String, String)]) -> Option<Period> {
    let value = |key| {
        pairs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    Period::parse(value("period")?, value("grain"))
}

pub async fn respond(req: &Request, env: &Env, now: Timestamp) -> Result<Reply> {
    let Some(period) = period_of(&super::feed::query_pairs(req)) else {
        console_log!("api stats rejected reason=bad_period");
        return Ok(Reply::error(400, ErrorCode::BadRequest));
    };
    let raw = stats::read(&env.d1("DB")?, period, now).await?;
    let (stats, errors) = assemble(raw, period, now);
    for err in errors {
        console_error!("api stats skipped a row: {err}");
    }
    Ok(Reply::json(200, &stats))
}

fn parsed<T>(
    rows: &[Value],
    errors: &mut Vec<RowError>,
    parse: fn(&Value) -> std::result::Result<T, RowError>,
) -> Vec<T> {
    rows.iter()
        .filter_map(|row| parse(row).map_err(|e| errors.push(e)).ok())
        .collect()
}

/// Validates every row and drops the ones that do not parse, so one bad row
/// cannot blank a section.
pub fn assemble(mut raw: RawStats, period: Period, now: Timestamp) -> (Stats, Vec<RowError>) {
    let (trends, mut errors) = super::trends::assemble(std::mem::take(&mut raw.trends), now);
    let input = StatsInput {
        now,
        utc_offset_minutes: i32::try_from(raw.utc_offset_minutes()).unwrap_or(0),
        goal_merged_per_month: raw
            .goal_merged_per_month()
            .and_then(|n| u32::try_from(n).ok()),
        import: raw.import(),
        prs: parsed(&raw.prs, &mut errors, parse_pr),
        merges: parsed(&raw.merges, &mut errors, parse_merge),
        reviews: parsed(&raw.reviews, &mut errors, parse_review),
        transitions: parsed(&raw.transitions, &mut errors, parse_transition),
        closed_issues: u32::try_from(raw.closed_issues()).unwrap_or(0),
        events: raw.event_times(),
        trends,
    };
    (build_stats(&input, period), errors)
}

#[cfg(test)]
mod tests {
    use domain::stats::Grain;

    use super::*;
    use crate::api::{precheck, Endpoint, Protected};

    const NOW: Timestamp = Timestamp(1_794_830_400);

    fn pairs(query: &str) -> Vec<(String, String)> {
        query
            .split('&')
            .filter_map(|p| p.split_once('='))
            .map(|(n, v)| (n.to_owned(), v.to_owned()))
            .collect()
    }

    #[test]
    fn only_the_documented_periods_are_accepted() {
        let month = Some(Period::Last12Months(Grain::Month));
        assert_eq!(period_of(&pairs("period=12m&grain=month")), month);
        assert_eq!(
            period_of(&pairs("period=12m&grain=week")),
            Some(Period::Last12Months(Grain::Week))
        );
        assert_eq!(period_of(&pairs("period=yoy")), Some(Period::YearOverYear));
        for bad in [
            "period=all",
            "period=12m&grain=day",
            "grain=week",
            "",
            "period=",
        ] {
            assert_eq!(period_of(&pairs(bad)), None, "{bad}");
        }
    }

    #[test]
    fn stats_is_protected_so_a_missing_session_is_a_401_and_a_get_needs_no_origin() {
        let endpoint = Endpoint::Protected(Protected::Stats);
        assert_eq!(
            precheck(endpoint, None, "http://127.0.0.1:8800"),
            Ok(Protected::Stats)
        );
    }

    #[test]
    fn an_empty_database_assembles_without_errors_and_serializes_nulls() {
        for period in [
            Period::Last12Months(Grain::Week),
            Period::Last12Months(Grain::Month),
            Period::YearOverYear,
        ] {
            let (stats, errors) = assemble(RawStats::default(), period, NOW);
            assert!(errors.is_empty());
            let json = serde_json::to_value(&stats).expect("json");
            assert!(json["lands"]["summary"]["merge_rate"].is_null());
            assert_eq!(json["lands"]["summary"]["merged"], 0);
            assert_eq!(json["yoy_ready"], false);
            assert!(!json.to_string().contains("NaN"));
        }
    }

    #[test]
    fn a_bad_row_is_dropped_with_an_error_and_the_rest_still_count() {
        let raw = RawStats {
            merges: vec![
                serde_json::json!({"repo": "bad", "external": 1, "merged_at": 1, "stars": null}),
                serde_json::json!({"repo": "o/ok", "external": 1, "merged_at": 1_794_000_000, "stars": 5}),
            ],
            ..RawStats::default()
        };
        let (stats, errors) = assemble(raw, Period::YearOverYear, NOW);
        assert_eq!(errors.len(), 1);
        assert_eq!(stats.habits.milestones.len(), 1);
    }
}
