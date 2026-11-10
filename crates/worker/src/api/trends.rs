use domain::ids::Timestamp;
use domain::trends::{build_trends, Trends};

use crate::db::trends::RawTrends;
use crate::db::RowError;

/// Validates every row and drops the ones that do not parse, so one bad row
/// cannot blank the charts.
pub fn assemble(raw: RawTrends, now: Timestamp) -> (Trends, Vec<RowError>) {
    let mut errors = Vec::new();
    let prs: Vec<_> = raw.prs.into_iter().map(|row| row.parse()).collect();
    let events: Vec<_> = raw
        .events
        .into_iter()
        .filter_map(|row| row.parse().map_err(|e| errors.push(e)).ok())
        .collect();
    let rollups: Vec<_> = raw
        .rollups
        .into_iter()
        .filter_map(|row| row.parse().map_err(|e| errors.push(e)).ok())
        .collect();
    (build_trends(&prs, &events, &rollups, now), errors)
}
