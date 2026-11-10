//! Hand-written SVG charts for the Trends view. They use no chart library and
//! no colour beyond the neutral tokens (ADR 0003).

pub mod bars;
pub mod heatmap;

use domain::ids::Timestamp;
use domain::trends::{Trends, HEATMAP_WEEKS};

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `YYYY-MM-DD`.
pub fn iso_date(at: Timestamp) -> String {
    at.to_string().chars().take(10).collect()
}

/// `Oct 14`, for an axis label.
pub fn short_date(at: Timestamp) -> String {
    let iso = iso_date(at);
    let month = iso
        .get(5..7)
        .and_then(|m| m.parse::<usize>().ok())
        .and_then(|m| MONTHS.get(m.wrapping_sub(1)))
        .copied()
        .unwrap_or_default();
    let day = iso.get(8..10).unwrap_or_default().trim_start_matches('0');
    format!("{month} {day}")
}

pub fn events_title(count: u32, at: Timestamp) -> String {
    let noun = if count == 1 { "event" } else { "events" };
    format!("{count} {noun} on {}", iso_date(at))
}

/// The line that opens the view.
pub fn summary(trends: &Trends) -> String {
    if trends.merged_total == 0 {
        return "Nothing merged yet.".to_owned();
    }
    let prs = if trends.merged_total == 1 {
        "pull request"
    } else {
        "pull requests"
    };
    let repos = if trends.repos_total == 1 {
        "repo"
    } else {
        "repos"
    };
    format!(
        "{} {prs} merged in {HEATMAP_WEEKS} weeks across {} {repos}",
        trends.merged_total, trends.repos_total
    )
}

#[cfg(test)]
mod tests {
    use domain::trends::build_trends;

    use super::*;

    fn at(s: &str) -> Timestamp {
        Timestamp::parse_rfc3339(s).expect("timestamp")
    }

    #[test]
    fn dates_read_as_iso_and_short() {
        assert_eq!(iso_date(at("2026-10-14T23:59:59Z")), "2026-10-14");
        assert_eq!(short_date(at("2026-10-14T00:00:00Z")), "Oct 14");
        assert_eq!(short_date(at("2026-01-05T00:00:00Z")), "Jan 5");
    }

    #[test]
    fn a_cell_title_names_its_count_and_date() {
        let day = at("2026-10-14T00:00:00Z");
        assert_eq!(events_title(3, day), "3 events on 2026-10-14");
        assert_eq!(events_title(1, day), "1 event on 2026-10-14");
        assert_eq!(events_title(0, day), "0 events on 2026-10-14");
    }

    #[test]
    fn the_summary_follows_the_totals() {
        let mut trends = build_trends(&[], &[], &[], at("2026-10-14T12:00:00Z"));
        assert_eq!(summary(&trends), "Nothing merged yet.");
        trends.merged_total = 14;
        trends.repos_total = 6;
        assert_eq!(
            summary(&trends),
            "14 pull requests merged in 26 weeks across 6 repos"
        );
        trends.merged_total = 1;
        trends.repos_total = 1;
        assert_eq!(
            summary(&trends),
            "1 pull request merged in 26 weeks across 1 repo"
        );
    }
}
