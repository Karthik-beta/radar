use domain::ids::{RepoName, Timestamp};
use serde::{Deserialize, Serialize};

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

/// Stores what GitHub says about each repo. A repo the import meets first gets
/// zero counts, which only the repo cards' own upsert ever sets.
const UPSERT_META: &str = "INSERT INTO repos (full_name, open_prs, merged_prs, closed_prs, \
     updated_at, node_id, owned_by_me, stars, primary_language, meta_refreshed_at) \
     SELECT json_extract(value, '$.name'), 0, 0, 0, 0, json_extract(value, '$.id'), \
     json_extract(value, '$.owned'), json_extract(value, '$.stars'), \
     json_extract(value, '$.language'), json_extract(value, '$.at') \
     FROM json_each(?1) WHERE true \
     ON CONFLICT (full_name) DO UPDATE SET node_id = excluded.node_id, \
     owned_by_me = excluded.owned_by_me, stars = excluded.stars, \
     primary_language = excluded.primary_language, \
     meta_refreshed_at = excluded.meta_refreshed_at";

const PAGE: &str = "SELECT full_name, node_id FROM repos \
     WHERE node_id IS NOT NULL AND full_name > ?1 ORDER BY full_name LIMIT ?2";

/// What the import or the weekly refresh learned about one repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoMeta {
    pub full_name: RepoName,
    pub node_id: String,
    pub owned_by_me: bool,
    pub stars: u32,
    pub primary_language: Option<String>,
    pub refreshed_at: Timestamp,
}

#[derive(Serialize)]
struct FlatMeta<'a> {
    name: String,
    id: &'a str,
    owned: u8,
    stars: u32,
    language: Option<&'a str>,
    at: i64,
}

pub fn upsert_meta(rows: &[RepoMeta]) -> Statement {
    let flat: Vec<FlatMeta<'_>> = rows
        .iter()
        .map(|r| FlatMeta {
            name: r.full_name.to_string(),
            id: &r.node_id,
            owned: u8::from(r.owned_by_me),
            stars: r.stars,
            language: r.primary_language.as_deref(),
            at: r.refreshed_at.0,
        })
        .collect();
    Statement {
        sql: UPSERT_META,
        params: vec![Param::text(
            serde_json::to_string(&flat).unwrap_or_else(|_| "[]".to_owned()),
        )],
    }
}

/// Up to `limit` repos with a node ID, in name order, after `after`.
pub fn page(after: Option<&RepoName>, limit: usize) -> Statement {
    Statement {
        sql: PAGE,
        params: vec![
            Param::text(after.map(RepoName::to_string).unwrap_or_default()),
            Param::Int(i64::try_from(limit).unwrap_or(i64::MAX)),
        ],
    }
}

#[derive(Debug, Deserialize)]
pub struct RepoIdRecord {
    pub full_name: String,
    pub node_id: String,
}

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

    fn meta(name: &str, stars: u32, at: i64) -> RepoMeta {
        RepoMeta {
            full_name: RepoName::parse(name).expect("repo"),
            node_id: format!("R_{}", name.replace('/', "_")),
            owned_by_me: name.starts_with("karthik/"),
            stars,
            primary_language: Some("Rust".to_owned()),
            refreshed_at: Timestamp(at),
        }
    }

    #[test]
    fn meta_keeps_the_repo_cards_counts_and_refreshes_the_rest() {
        let conn = memory_db();
        exec(&conn, &upsert(&repo(2, 2000)).expect("stmt")).expect("card");
        exec(
            &conn,
            &upsert_meta(&[meta("karthik-beta/radar", 5, 10), meta("karthik/x", 1, 10)]),
        )
        .expect("import");
        exec(
            &conn,
            &upsert_meta(&[RepoMeta {
                primary_language: None,
                ..meta("karthik-beta/radar", 9, 20)
            }]),
        )
        .expect("refresh");
        let rows = query_json(
            &conn,
            "SELECT full_name, open_prs, stars, primary_language, owned_by_me, \
             meta_refreshed_at FROM repos ORDER BY full_name",
            &[],
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["open_prs"], 2);
        assert_eq!(rows[0]["stars"], 9);
        assert!(rows[0]["primary_language"].is_null());
        assert_eq!(rows[0]["meta_refreshed_at"], 20);
        assert_eq!(rows[1]["owned_by_me"], 1);
        assert_eq!(rows[1]["open_prs"], 0);
    }

    #[test]
    fn a_page_holds_repos_with_a_node_id_after_the_last_name() {
        let conn = memory_db();
        exec(&conn, &upsert(&repo(2, 2000)).expect("stmt")).expect("no node id");
        let names = ["a/one", "b/two", "c/three"];
        let all: Vec<RepoMeta> = names.iter().map(|n| meta(n, 1, 1)).collect();
        exec(&conn, &upsert_meta(&all)).expect("import");
        let first = page(None, 2);
        let got = query_json(&conn, first.sql, &first.params);
        assert_eq!(got.len(), 2);
        assert_eq!(got[1]["full_name"], "b/two");
        let after = RepoName::parse("b/two").expect("repo");
        let next = page(Some(&after), 2);
        let got = query_json(&conn, next.sql, &next.params);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["full_name"], "c/three");
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
