//! Hand-written SVG charts for the Stats view. They use no chart library and
//! no colour beyond the neutral tokens (ADR 0003).

pub mod bars;
pub mod grid;
pub mod hbars;
pub mod heatmap;
pub mod lines;

use domain::ids::Timestamp;

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

/// The slots that carry a label on an axis: first, last, and three between.
pub fn axis_slots(n: usize) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }
    let mut slots: Vec<usize> = (0..5).map(|i| i * (n - 1) / 4).collect();
    slots.dedup();
    slots
}

/// `Oct`, for an axis label.
pub fn month_label(at: Timestamp) -> String {
    month_name(at.year_month().1).to_owned()
}

pub fn month_name(month: i64) -> &'static str {
    usize::try_from(month - 1)
        .ok()
        .and_then(|m| MONTHS.get(m))
        .copied()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
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
    fn axis_labels_cover_empty_one_and_many() {
        assert!(axis_slots(0).is_empty());
        assert_eq!(axis_slots(1), [0]);
        assert_eq!(axis_slots(13), [0, 3, 6, 9, 12]);
        assert_eq!(axis_slots(52), [0, 12, 25, 38, 51]);
    }

    #[test]
    fn a_month_reads_as_its_short_name() {
        assert_eq!(month_label(at("2026-10-14T00:00:00Z")), "Oct");
        assert_eq!(month_name(0), "");
        assert_eq!(month_name(13), "");
    }
}
