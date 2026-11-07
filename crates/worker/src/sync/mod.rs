//! The Syncer Durable Object: the only code that calls GitHub. Each alarm runs
//! one unit from the `work` table, and `scheduler` decides which.

mod config;
mod gate;
mod handlers;
mod scheduler;
mod state;
mod store;
mod work;

use domain::ids::Timestamp;
use domain::sync_status::SyncStatus;
use serde_json::json;
use worker::js_sys::{Date as JsDate, Number};
use worker::{
    console_error, console_log, durable_object, Date, DurableObject, Env, Request, Response,
    Result, ScheduledTime, SqlStorageValue, State, Url,
};

pub use config::reason_for_cron;
pub use work::KickReason;

use crate::auth::now;
use crate::db::{Param, Statement};
use scheduler::{apply, plan, Applied, Decision, RowEffect, UnitOutcome};
use state::SyncState;
#[cfg(feature = "probe")]
use work::WorkKind;
use work::WorkRow;

const BINDING: &str = "SYNCER";
const INSTANCE: &str = "syncer";
const ORIGIN: &str = "https://syncer";

#[durable_object]
pub struct Syncer {
    state: State,
    env: Env,
}

impl DurableObject for Syncer {
    fn new(state: State, env: Env) -> Self {
        let syncer = Self { state, env };
        if let Err(err) = syncer.exec_sql(store::SCHEMA, Vec::new()) {
            console_error!("syncer schema failed: {err}");
        }
        syncer
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let url = req.url()?;
        let param = |name: &str| {
            url.query_pairs()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default()
        };
        match url.path() {
            "/status" => Response::from_json(&self.status().await?),
            "/kick" => match KickReason::parse(&param("reason")) {
                Some(reason) => {
                    self.kick(reason).await?;
                    Response::from_json(&json!({ "kicked": reason.as_str() }))
                }
                None => error(400, "unknown_reason"),
            },
            #[cfg(feature = "probe")]
            "/enqueue" => match WorkKind::from_request(&param("kind"), &param("key")) {
                Ok(kind) => {
                    let queued = self.enqueue(kind).await?;
                    Response::from_json(&json!({ "queued": queued }))
                }
                Err(err) => error(400, &err.to_string()),
            },
            _ => error(404, "not_found"),
        }
    }

    async fn alarm(&self) -> Result<Response> {
        if let Err(err) = self.run_alarm().await {
            console_error!("syncer alarm failed: {err}");
        }
        Response::ok("")
    }
}

impl Syncer {
    async fn run_alarm(&self) -> Result<()> {
        let (rows, sync) = self.load()?;
        if let Decision::Run(row) = plan(&rows, &sync, now()) {
            self.run_unit(row, sync).await?;
        }
        let (rows, sync) = self.load()?;
        let at = now();
        self.set_alarm(plan(&rows, &sync, at).alarm(at)).await
    }

    async fn run_unit(&self, row: WorkRow, mut observed: SyncState) -> Result<()> {
        let kind = row.kind.clone();
        let (name, key) = (kind.kind().as_str(), kind.key());
        console_log!("unit start kind={name} key={key}");
        let started = Date::now().as_millis();
        let outcome = handlers::run(&kind, &self.env, &mut observed, now()).await;
        let done_at = now();
        // Kicks run while the unit awaits GitHub, so write onto a fresh read.
        let (rows, mut sync) = self.load()?;
        sync.observe(&observed);
        let row = rows.iter().find(|r| r.kind == kind).cloned().unwrap_or(row);
        let Applied {
            row: effect,
            throttle,
        } = apply(row, &outcome, sync.throttle, done_at);
        sync.throttle = throttle;
        let mut writes = vec![match &effect {
            RowEffect::Delete => store::delete(&kind),
            RowEffect::Update(row) | RowEffect::MarkDead(row) => store::upsert(row),
        }];
        if let UnitOutcome::Done { follow_ups } = &outcome {
            sync.last_ok.insert(kind.kind(), done_at);
            let others: Vec<WorkRow> = rows.iter().filter(|r| r.kind != kind).cloned().collect();
            writes.extend(
                follow_ups
                    .iter()
                    .filter_map(|next| scheduler::enqueue(&others, next.clone(), done_at))
                    .map(|row| store::upsert(&row)),
            );
        }
        writes.push(store::save_state(&sync));
        for write in &writes {
            self.exec(write)?;
        }
        match (&effect, &outcome) {
            (RowEffect::MarkDead(row), _) => console_log!(
                "sync.dead kind={name} key={key} attempts={} error={}",
                row.attempts,
                row.last_error.as_deref().unwrap_or_default()
            ),
            (RowEffect::Update(row), UnitOutcome::Retry { error }) => console_log!(
                "unit retry kind={name} key={key} attempts={} due_in={} error={error}",
                row.attempts,
                row.due_at.0 - done_at.0
            ),
            (RowEffect::Update(row), UnitOutcome::RateLimited { .. }) => console_log!(
                "unit deferred kind={name} key={key} cause=rate_limited due_in={} strikes={}",
                row.due_at.0 - done_at.0,
                sync.throttle.strikes
            ),
            (_, UnitOutcome::NoToken) => {
                console_log!("unit deferred kind={name} key={key} cause=no_token")
            }
            _ => {}
        }
        let ms = Date::now().as_millis().saturating_sub(started);
        console_log!("unit done kind={name} ms={ms} outcome={}", outcome.name());
        Ok(())
    }

    async fn kick(&self, reason: KickReason) -> Result<()> {
        let at = now();
        let (rows, mut sync) = self.load()?;
        let upserts = scheduler::kick(reason, &rows, &mut sync, at);
        for row in &upserts {
            self.exec(&store::upsert(row))?;
        }
        self.exec(&store::save_state(&sync))?;
        console_log!(
            "syncer kick reason={} queued={}",
            reason.as_str(),
            upserts.len()
        );
        self.arm_earlier(at).await
    }

    #[cfg(feature = "probe")]
    async fn enqueue(&self, kind: WorkKind) -> Result<bool> {
        let at = now();
        let (rows, _) = self.load()?;
        let Some(row) = scheduler::enqueue(&rows, kind, at) else {
            return Ok(false);
        };
        self.exec(&store::upsert(&row))?;
        self.arm_earlier(at).await?;
        Ok(true)
    }

    async fn status(&self) -> Result<SyncStatus> {
        let (rows, sync) = self.load()?;
        let alarm = self.state.storage().get_alarm().await?;
        let next_alarm = alarm.map(|ms| Timestamp(ms.div_euclid(1000)));
        Ok(state::status(&rows, &sync, next_alarm, now()))
    }

    /// Many kicks make one alarm: a kick only ever moves the alarm earlier.
    async fn arm_earlier(&self, at: Timestamp) -> Result<()> {
        let (rows, sync) = self.load()?;
        let Some(target) = plan(&rows, &sync, at).alarm(at) else {
            return Ok(());
        };
        let current = self.state.storage().get_alarm().await?;
        if current.is_none_or(|ms| millis(target) < ms) {
            self.set_alarm(Some(target)).await?;
        }
        Ok(())
    }

    async fn set_alarm(&self, at: Option<Timestamp>) -> Result<()> {
        let storage = self.state.storage();
        match at {
            Some(at) => {
                let date = JsDate::new(&Number::from(millis(at) as f64));
                storage.set_alarm(ScheduledTime::new(date)).await
            }
            None => storage.delete_alarm().await,
        }
    }

    fn load(&self) -> Result<(Vec<WorkRow>, SyncState)> {
        let sql = self.state.storage().sql();
        let raw = sql
            .exec(store::SELECT_ROWS, None)?
            .to_array::<store::RawRow>()?;
        let (rows, errors) = store::parse_rows(raw);
        for err in errors {
            console_error!("syncer skipped a work row: {err}");
        }
        let sync = sql
            .exec(store::SELECT_STATE, None)?
            .to_array::<store::RawState>()?
            .into_iter()
            .next()
            .map(store::RawState::parse)
            .unwrap_or_default();
        Ok((rows, sync))
    }

    fn exec(&self, stmt: &Statement) -> Result<()> {
        let values = stmt.params.iter().map(sql_value).collect();
        self.exec_sql(stmt.sql, values)
    }

    fn exec_sql(&self, sql: &str, values: Vec<SqlStorageValue>) -> Result<()> {
        let bindings = (!values.is_empty()).then_some(values);
        let cursor = self.state.storage().sql().exec(sql, bindings)?;
        for row in cursor.raw() {
            row?;
        }
        Ok(())
    }
}

fn sql_value(param: &Param) -> SqlStorageValue {
    match param {
        Param::Text(s) => SqlStorageValue::String(s.clone()),
        Param::Int(i) => SqlStorageValue::Integer(*i),
        Param::Real(f) => SqlStorageValue::Float(*f),
        Param::Null => SqlStorageValue::Null,
        Param::Blob(b) => SqlStorageValue::Blob(b.clone()),
    }
}

fn millis(at: Timestamp) -> i64 {
    at.0.saturating_mul(1000)
}

fn error(status: u16, code: &str) -> Result<Response> {
    Ok(Response::from_json(&json!({ "error": code }))?.with_status(status))
}

async fn call(env: &Env, path: &str, query: &[(&str, &str)]) -> Result<Response> {
    let url = Url::parse_with_params(&format!("{ORIGIN}{path}"), query)
        .map_err(|err| worker::Error::RustError(err.to_string()))?;
    let stub = env
        .durable_object(BINDING)?
        .id_from_name(INSTANCE)?
        .get_stub()?;
    stub.fetch_with_str(url.as_str()).await
}

pub async fn kick(env: &Env, reason: KickReason) -> Result<()> {
    let response = call(env, "/kick", &[("reason", reason.as_str())]).await?;
    match response.status_code() {
        200 => Ok(()),
        status => Err(worker::Error::RustError(format!(
            "syncer kick answered {status}"
        ))),
    }
}

/// The Syncer's status, reachable only through the `SYNCER` binding.
#[cfg(feature = "probe")]
pub async fn status(env: &Env) -> Result<Response> {
    call(env, "/status", &[]).await
}

#[cfg(feature = "probe")]
pub async fn enqueue(env: &Env, kind: &str, key: &str) -> Result<Response> {
    call(env, "/enqueue", &[("kind", kind), ("key", key)]).await
}
