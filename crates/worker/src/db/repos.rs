use domain::ids::{RepoName, Timestamp};

use super::rows::RowError;
use super::sql::{Param, Statement};

const UPSERT: &str = "INSERT INTO repos (full_name, open_prs, merged_prs, closed_prs, \
     median_first_response_hours, last_activity_at, updated_at) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
     ON CONFLICT (full_name) DO UPDATE SET \
     open_prs = excluded.open_prs, merged_prs = excluded.merged_prs, \
     closed_prs = excluded.closed_prs, \
     median_first_response_hours = excluded.median_first_response_hours, \
     last_activity_at = excluded.last_activity_at, updated_at = excluded.updated_at \
     WHERE excluded.updated_at >= repos.updated_at";

#[derive(Debug, Clone, PartialEq)]
pub struct RepoRow {
    pub full_name: RepoName,
    pub open_prs: u32,
    pub merged_prs: u32,
    pub closed_prs: u32,
    pub median_first_response_hours: Option<f64>,
    pub last_activity_at: Option<Timestamp>,
    pub updated_at: Timestamp,
}

pub fn upsert(row: &RepoRow) -> Result<Statement, RowError> {
    let median = match row.median_first_response_hours {
        None => Param::Null,
        Some(h) if h.is_finite() && h >= 0.0 => Param::Real(h),
        Some(_) => return Err(RowError::NotFinite("median_first_response_hours")),
    };
    Ok(Statement {
        sql: UPSERT,
        params: vec![
            Param::text(row.full_name.to_string()),
            Param::Int(i64::from(row.open_prs)),
            Param::Int(i64::from(row.merged_prs)),
            Param::Int(i64::from(row.closed_prs)),
            median,
            Param::opt_ts(row.last_activity_at),
            Param::ts(row.updated_at),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};

    fn repo(open_prs: u32, updated_at: i64) -> RepoRow {
        RepoRow {
            full_name: RepoName::parse("karthik-beta/radar").expect("repo"),
            open_prs,
            merged_prs: 3,
            closed_prs: 1,
            median_first_response_hours: Some(5.5),
            last_activity_at: Some(Timestamp(1000)),
            updated_at: Timestamp(updated_at),
        }
    }

    #[test]
    fn repo_upsert_keeps_one_row_and_the_newest_counts() {
        let conn = memory_db();
        exec(&conn, &upsert(&repo(2, 2000)).expect("stmt")).expect("first");
        exec(&conn, &upsert(&repo(5, 3000)).expect("stmt")).expect("newer");
        exec(&conn, &upsert(&repo(1, 2500)).expect("stmt")).expect("older");
        assert_eq!(count_rows(&conn, "repos"), 1);
        let rows = query_json(
            &conn,
            "SELECT open_prs, median_first_response_hours FROM repos",
            &[],
        );
        assert_eq!(rows[0]["open_prs"], 5);
        assert_eq!(rows[0]["median_first_response_hours"], 5.5);
    }

    #[test]
    fn non_finite_median_is_refused_before_sql() {
        for bad in [f64::NAN, f64::INFINITY, -1.0] {
            let row = RepoRow {
                median_first_response_hours: Some(bad),
                ..repo(1, 1)
            };
            assert!(upsert(&row).is_err());
        }
    }
}
