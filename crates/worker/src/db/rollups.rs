use domain::ids::Timestamp;

use super::rows::RowError;
use super::sql::{Param, Statement};

const SECONDS_PER_WEEK: i64 = 604_800;
/// 1970-01-05, the first Monday after the epoch.
const FIRST_MONDAY: i64 = 345_600;

const UPSERT: &str = "INSERT INTO rollups_weekly (week_start, prs_opened, prs_merged, \
     repos_touched, events) VALUES (?1, ?2, ?3, ?4, ?5) \
     ON CONFLICT (week_start) DO UPDATE SET \
     prs_opened = excluded.prs_opened, prs_merged = excluded.prs_merged, \
     repos_touched = excluded.repos_touched, events = excluded.events";

/// The Monday 00:00 UTC that starts a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeekStart(i64);

impl WeekStart {
    pub fn new(start: Timestamp) -> Result<Self, RowError> {
        if start.0.rem_euclid(SECONDS_PER_WEEK) == FIRST_MONDAY {
            Ok(Self(start.0))
        } else {
            Err(RowError::NotMonday(start.0))
        }
    }

    /// The week that contains `at`.
    pub fn containing(at: Timestamp) -> Self {
        let since_monday = (at.0 - FIRST_MONDAY).rem_euclid(SECONDS_PER_WEEK);
        Self(at.0 - since_monday)
    }

    pub fn timestamp(self) -> Timestamp {
        Timestamp(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollupRow {
    pub week_start: WeekStart,
    pub prs_opened: u32,
    pub prs_merged: u32,
    pub repos_touched: u32,
    pub events: u32,
}

pub fn upsert(row: &RollupRow) -> Statement {
    Statement {
        sql: UPSERT,
        params: vec![
            Param::ts(row.week_start.timestamp()),
            Param::Int(i64::from(row.prs_opened)),
            Param::Int(i64::from(row.prs_merged)),
            Param::Int(i64::from(row.repos_touched)),
            Param::Int(i64::from(row.events)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};

    #[test]
    fn week_start_must_be_a_monday_midnight_utc() {
        let monday = Timestamp::parse_rfc3339("2026-11-02T00:00:00Z").expect("ts");
        assert!(WeekStart::new(monday).is_ok());
        for bad in [
            "2026-11-02T00:00:01Z",
            "2026-11-03T00:00:00Z",
            "2026-11-01T00:00:00Z",
        ] {
            let ts = Timestamp::parse_rfc3339(bad).expect("ts");
            assert_eq!(WeekStart::new(ts), Err(RowError::NotMonday(ts.0)));
        }
    }

    #[test]
    fn containing_snaps_back_to_monday() {
        let monday = Timestamp::parse_rfc3339("2026-11-02T00:00:00Z").expect("ts");
        let sunday_night = Timestamp::parse_rfc3339("2026-11-08T23:59:59Z").expect("ts");
        let next_monday = Timestamp::parse_rfc3339("2026-11-09T00:00:00Z").expect("ts");
        assert_eq!(WeekStart::containing(sunday_night).timestamp(), monday);
        assert_eq!(WeekStart::containing(next_monday).timestamp(), next_monday);
        assert_eq!(
            WeekStart::containing(Timestamp(0)).timestamp(),
            Timestamp(-259_200)
        );
    }

    #[test]
    fn rollup_upsert_replaces_the_week() {
        let conn = memory_db();
        let week = WeekStart::new(Timestamp(FIRST_MONDAY)).expect("monday");
        let row = RollupRow {
            week_start: week,
            prs_opened: 3,
            prs_merged: 2,
            repos_touched: 1,
            events: 10,
        };
        exec(&conn, &upsert(&row)).expect("first");
        exec(&conn, &upsert(&RollupRow { events: 12, ..row })).expect("second");
        assert_eq!(count_rows(&conn, "rollups_weekly"), 1);
        let rows = query_json(&conn, "SELECT events FROM rollups_weekly", &[]);
        assert_eq!(rows[0]["events"], 12);
    }

    #[test]
    fn the_table_refuses_a_week_that_starts_midweek() {
        let conn = memory_db();
        let mut stmt = upsert(&RollupRow {
            week_start: WeekStart(FIRST_MONDAY),
            prs_opened: 0,
            prs_merged: 0,
            repos_touched: 0,
            events: 0,
        });
        stmt.params[0] = Param::Int(FIRST_MONDAY + 1);
        assert!(exec(&conn, &stmt).is_err());
    }
}
