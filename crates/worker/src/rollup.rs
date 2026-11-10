//! The handler for `WorkKind::Rollup`. One unit folds the oldest week of events
//! past the cutoff into `rollups_weekly` and deletes that week's rows, in one D1
//! batch, so a retry never counts a week twice. It makes no GitHub call.

use domain::ids::Timestamp;
use domain::rollup::{rolls_up, rollup_cutoff, WeekStart};
use serde::Deserialize;
use worker::{console_log, D1Database, Env};

use crate::db::rollups::{self, RollupRow, RollupTotals};
use crate::db::{run, Statement};
use crate::sync::scheduler::UnitOutcome;
use crate::sync::work::WorkKind;

const NIGHTLY_KEY: &str = "nightly";
const WEEK_PREFIX: &str = "week:";

/// Which week a rollup unit names. The nightly unit finds the oldest week
/// itself. A follow-up names the next one, under its own key so it cannot
/// merge with the nightly row that is finishing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollupKey {
    Nightly,
    Week(WeekStart),
}

impl RollupKey {
    pub fn key(self) -> String {
        match self {
            Self::Nightly => NIGHTLY_KEY.to_owned(),
            Self::Week(week) => format!("{WEEK_PREFIX}{}", week.date()),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        if key == NIGHTLY_KEY {
            return Some(Self::Nightly);
        }
        WeekStart::parse_date(key.strip_prefix(WEEK_PREFIX)?).map(Self::Week)
    }
}

#[derive(Deserialize)]
struct Oldest {
    at: Option<i64>,
}

/// The statements that fold one week in, in the order D1 runs them.
pub fn fold(week: WeekStart, totals: RollupTotals) -> Vec<Statement> {
    vec![
        rollups::upsert(&RollupRow {
            week_start: week,
            totals,
        }),
        rollups::delete_week(week),
    ]
}

pub async fn run_unit(key: RollupKey, env: &Env, now: Timestamp) -> UnitOutcome {
    match unit(key, env, now).await {
        Ok(follow_ups) => UnitOutcome::Done { follow_ups },
        Err(error) => UnitOutcome::Retry { error },
    }
}

async fn unit(key: RollupKey, env: &Env, now: Timestamp) -> Result<Vec<WorkKind>, String> {
    let db = env.d1("DB").map_err(|err| format!("d1 binding: {err}"))?;
    let target = match key {
        RollupKey::Nightly => oldest_week(&db, now).await?,
        RollupKey::Week(week) => rolls_up(week, now).then_some(week),
    };
    let Some(week) = target else {
        console_log!("rollup nothing to roll");
        return Ok(Vec::new());
    };
    let totals = week_totals(&db, week).await?;
    if totals.events == 0 {
        console_log!("rollup week={} events=0 skipped", week.date());
    } else {
        run(&db, fold(week, totals))
            .await
            .map_err(|err| format!("d1 rollup batch: {err}"))?;
        console_log!(
            "rollup week={} events={} opened={} merged={} repos={}",
            week.date(),
            totals.events,
            totals.prs_opened,
            totals.prs_merged,
            totals.repos_touched
        );
    }
    let next = oldest_week(&db, now).await?;
    Ok(next
        .filter(|next| *next != week)
        .map(|next| WorkKind::Rollup(RollupKey::Week(next)))
        .into_iter()
        .collect())
}

async fn oldest_week(db: &D1Database, now: Timestamp) -> Result<Option<WeekStart>, String> {
    let cutoff = rollup_cutoff(now).timestamp();
    let rows = rollups::oldest_before(cutoff)
        .prepare(db)
        .map_err(|err| err.to_string())?
        .all()
        .await
        .and_then(|result| result.results::<Oldest>())
        .map_err(|err| format!("d1 read: {err}"))?;
    Ok(rows
        .first()
        .and_then(|row| row.at)
        .map(|at| WeekStart::containing(Timestamp(at))))
}

async fn week_totals(db: &D1Database, week: WeekStart) -> Result<RollupTotals, String> {
    let rows = rollups::week_totals(week)
        .prepare(db)
        .map_err(|err| err.to_string())?
        .all()
        .await
        .and_then(|result| result.results::<RollupTotals>())
        .map_err(|err| format!("d1 read: {err}"))?;
    rows.into_iter()
        .next()
        .ok_or_else(|| "week totals returned no row".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::{count_rows, exec, memory_db, query_json};

    fn week(date: &str) -> WeekStart {
        WeekStart::parse_date(date).expect("monday")
    }

    #[test]
    fn keys_round_trip_and_never_collide() {
        for key in [RollupKey::Nightly, RollupKey::Week(week("2026-05-04"))] {
            assert_eq!(RollupKey::parse(&key.key()), Some(key));
        }
        assert_eq!(RollupKey::Week(week("2026-05-04")).key(), "week:2026-05-04");
        assert_ne!(
            RollupKey::Nightly.key(),
            RollupKey::Week(week("2026-05-04")).key()
        );
        for bad in [
            "",
            "main",
            "week:",
            "week:2026-05-05",
            "week:nightly",
            "Nightly",
        ] {
            assert_eq!(RollupKey::parse(bad), None, "{bad}");
        }
    }

    fn seed(conn: &rusqlite::Connection, id: &str, repo: &str, at: i64) {
        conn.execute(
            "INSERT INTO events (id, item_node_id, repo, kind, actor_login, actor_is_me, \
             actor_is_bot, occurred_at, source, ingested_at) \
             VALUES (?1, 'PR_x', ?2, 'commented', 'alice', 0, 0, ?3, 'poll', 0)",
            rusqlite::params![id, repo, at],
        )
        .expect("event");
    }

    #[test]
    fn folding_a_week_moves_its_events_into_one_row() {
        let conn = memory_db();
        let w = week("2026-05-04");
        let start = w.timestamp().0;
        seed(&conn, "a", "o/one", start + 10);
        seed(&conn, "b", "o/two", start + 20);
        seed(&conn, "c", "o/one", w.end().0);
        let totals = RollupTotals {
            events: 2,
            prs_opened: 0,
            prs_merged: 0,
            repos_touched: 2,
        };
        for stmt in fold(w, totals) {
            exec(&conn, &stmt).expect("statement");
        }
        assert_eq!(count_rows(&conn, "events"), 1);
        assert_eq!(count_rows(&conn, "rollups_weekly"), 1);
        let rows = query_json(
            &conn,
            "SELECT events, repos_touched FROM rollups_weekly",
            &[],
        );
        assert_eq!(rows[0]["events"].as_i64(), Some(2));
        assert_eq!(rows[0]["repos_touched"].as_i64(), Some(2));
    }
}
