mod fixture;

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use worker::{
    console_error, console_log, durable_object, event, js_sys, Context, Date, DurableObject, Env,
    Error, Method, Request, Response, Result, ScheduledTime, SqlStorage, SqlStorageValue, State,
    Url,
};

static ALARM_SEEN: AtomicBool = AtomicBool::new(false);

const MAX_CHAIN: u32 = 100;
const MAX_GAP_MS: u32 = 3_600_000;
const DEFAULT_GAP_MS: u32 = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Fixture {
    Ok,
    Throw,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Cold,
    Warm,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Outcome {
    Ok,
    Error,
}

#[derive(Serialize, Deserialize)]
struct AlarmRun {
    seq: i64,
    scheduled_for: i64,
    started_at: i64,
    kind: Kind,
    attempt: i64,
    parsed: i64,
    failing: i64,
    drafts: i64,
    outcome: Outcome,
}

#[derive(Deserialize)]
struct Chain {
    seq: i64,
    last_seq: i64,
    gap_ms: i64,
    fixture: Fixture,
    scheduled_for: i64,
    attempt: i64,
}

#[derive(Debug, PartialEq)]
struct Arm {
    chain: u32,
    gap_ms: u32,
    fixture: Fixture,
}

#[derive(Serialize)]
struct Armed {
    first_seq: i64,
    last_seq: i64,
    scheduled_for: i64,
}

#[derive(Serialize)]
struct Log {
    pending_alarm: Option<i64>,
    runs: Vec<AlarmRun>,
}

#[derive(Deserialize)]
struct MaxSeq {
    seq: i64,
}

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let key = env
        .secret("SPIKE_KEY")
        .map(|k| k.to_string())
        .unwrap_or_default();
    if key.is_empty() {
        return Response::error("SPIKE_KEY is not set", 500);
    }
    let header = req.headers().get("authorization")?;
    if !authorized(header.as_deref(), &key) {
        return Response::error("unauthorized", 401);
    }
    env.durable_object("PROBE")?
        .id_from_name("probe")?
        .get_stub()?
        .fetch_with_request(req)
        .await
}

fn authorized(header: Option<&str>, key: &str) -> bool {
    header
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|given| constant_time_eq(given.as_bytes(), key.as_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn parse_arm(url: &Url) -> std::result::Result<Arm, String> {
    let mut arm = Arm {
        chain: 1,
        gap_ms: DEFAULT_GAP_MS,
        fixture: Fixture::Ok,
    };
    for (name, value) in url.query_pairs() {
        match &*name {
            "chain" => {
                arm.chain = value
                    .parse()
                    .ok()
                    .filter(|n| (1..=MAX_CHAIN).contains(n))
                    .ok_or(format!("chain must be 1 to {MAX_CHAIN}"))?
            }
            "gap_ms" => {
                arm.gap_ms = value
                    .parse()
                    .ok()
                    .filter(|n| *n <= MAX_GAP_MS)
                    .ok_or(format!("gap_ms must be 0 to {MAX_GAP_MS}"))?
            }
            "fixture" => {
                arm.fixture = match &*value {
                    "ok" => Fixture::Ok,
                    "throw" => Fixture::Throw,
                    _ => return Err("fixture must be ok or throw".into()),
                }
            }
            other => return Err(format!("unknown parameter {other}")),
        }
    }
    Ok(arm)
}

fn now_ms() -> i64 {
    Date::now().as_millis() as i64
}

fn at(ms: i64) -> ScheduledTime {
    ScheduledTime::new(js_sys::Date::new(&(ms as f64).into()))
}

#[durable_object(alarm)]
pub struct Probe {
    state: State,
}

impl Probe {
    fn sql(&self) -> SqlStorage {
        self.state.storage().sql()
    }

    async fn arm(&self, arm: Arm) -> Result<Response> {
        let sql = self.sql();
        let last_run = sql
            .exec("SELECT COALESCE(MAX(seq), 0) AS seq FROM runs", None)?
            .one::<MaxSeq>()?
            .seq;
        let armed = Armed {
            first_seq: last_run + 1,
            last_seq: last_run + i64::from(arm.chain),
            scheduled_for: now_ms() + i64::from(arm.gap_ms),
        };
        let fixture = match arm.fixture {
            Fixture::Ok => "ok",
            Fixture::Throw => "throw",
        };
        sql.exec(
            "INSERT OR REPLACE INTO chain (id, seq, last_seq, gap_ms, fixture, scheduled_for, attempt) \
             VALUES (1, ?, ?, ?, ?, ?, 0)",
            vec![
                armed.first_seq.into(),
                armed.last_seq.into(),
                i64::from(arm.gap_ms).into(),
                fixture.into(),
                armed.scheduled_for.into(),
            ],
        )?;
        self.state
            .storage()
            .set_alarm(at(armed.scheduled_for))
            .await?;
        Response::from_json(&armed)
    }

    async fn log(&self) -> Result<Response> {
        let runs = self
            .sql()
            .exec(
                "SELECT seq, scheduled_for, started_at, kind, attempt, parsed, failing, drafts, outcome \
                 FROM runs ORDER BY seq, attempt",
                None,
            )?
            .to_array::<AlarmRun>()?;
        let pending_alarm = self.state.storage().get_alarm().await?;
        Response::from_json(&Log {
            pending_alarm,
            runs,
        })
    }

    async fn reset(&self) -> Result<Response> {
        self.state.storage().delete_alarm().await?;
        let sql = self.sql();
        sql.exec("DELETE FROM runs", None)?;
        sql.exec("DELETE FROM chain", None)?;
        Ok(Response::empty()?.with_status(204))
    }

    fn record(&self, run: &AlarmRun) -> Result<()> {
        let kind = match run.kind {
            Kind::Cold => "cold",
            Kind::Warm => "warm",
        };
        let outcome = match run.outcome {
            Outcome::Ok => "ok",
            Outcome::Error => "error",
        };
        let values: Vec<SqlStorageValue> = vec![
            run.seq.into(),
            run.scheduled_for.into(),
            run.started_at.into(),
            kind.into(),
            run.attempt.into(),
            run.parsed.into(),
            run.failing.into(),
            run.drafts.into(),
            outcome.into(),
        ];
        self.sql().exec(
            "INSERT INTO runs (seq, scheduled_for, started_at, kind, attempt, parsed, failing, drafts, outcome) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            values,
        )?;
        Ok(())
    }
}

impl DurableObject for Probe {
    fn new(state: State, _env: Env) -> Self {
        let sql = state.storage().sql();
        let schema = [
            "CREATE TABLE IF NOT EXISTS runs (seq INTEGER NOT NULL, scheduled_for INTEGER NOT NULL, \
             started_at INTEGER NOT NULL, kind TEXT NOT NULL, attempt INTEGER NOT NULL, \
             parsed INTEGER NOT NULL, failing INTEGER NOT NULL, drafts INTEGER NOT NULL, \
             outcome TEXT NOT NULL, PRIMARY KEY (seq, attempt))",
            "CREATE TABLE IF NOT EXISTS chain (id INTEGER PRIMARY KEY CHECK (id = 1), \
             seq INTEGER NOT NULL, last_seq INTEGER NOT NULL, gap_ms INTEGER NOT NULL, \
             fixture TEXT NOT NULL, scheduled_for INTEGER NOT NULL, attempt INTEGER NOT NULL)",
        ];
        for statement in schema {
            if let Err(err) = sql.exec(statement, None) {
                console_error!("schema: {err}");
            }
        }
        Self { state }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let url = req.url()?;
        match (req.method(), url.path()) {
            (Method::Post, "/arm") => match parse_arm(&url) {
                Ok(arm) => self.arm(arm).await,
                Err(msg) => Response::error(msg, 400),
            },
            (Method::Get, "/log") => self.log().await,
            (Method::Post, "/reset") => self.reset().await,
            (_, "/arm" | "/log" | "/reset") => Response::error("method not allowed", 405),
            _ => Response::error("not found", 404),
        }
    }

    async fn alarm(&self) -> Result<Response> {
        let started_at = now_ms();
        let kind = if ALARM_SEEN.swap(true, Ordering::Relaxed) {
            Kind::Warm
        } else {
            Kind::Cold
        };
        let sql = self.sql();
        let Some(chain) = sql
            .exec(
                "SELECT seq, last_seq, gap_ms, fixture, scheduled_for, attempt FROM chain WHERE id = 1",
                None,
            )?
            .to_array::<Chain>()?
            .into_iter()
            .next()
        else {
            console_log!("alarm with no chain");
            return Response::ok("");
        };
        let attempt = chain.attempt + 1;
        console_log!(
            "alarm seq={} kind={}",
            chain.seq,
            match kind {
                Kind::Cold => "cold",
                Kind::Warm => "warm",
            }
        );
        let counts = fixture::count(fixture::FIXTURE)
            .map_err(|err| Error::RustError(format!("fixture: {err}")))?;
        let outcome = if chain.fixture == Fixture::Throw && attempt == 1 {
            Outcome::Error
        } else {
            Outcome::Ok
        };
        self.record(&AlarmRun {
            seq: chain.seq,
            scheduled_for: chain.scheduled_for,
            started_at,
            kind,
            attempt,
            parsed: counts.parsed,
            failing: counts.failing,
            drafts: counts.drafts,
            outcome,
        })?;

        if outcome == Outcome::Error {
            sql.exec(
                "UPDATE chain SET attempt = ? WHERE id = 1",
                vec![attempt.into()],
            )?;
            return Err(Error::RustError(format!(
                "fixture=throw fails attempt 1 of seq {}",
                chain.seq
            )));
        }

        if chain.seq < chain.last_seq {
            let next = now_ms() + chain.gap_ms;
            sql.exec(
                "UPDATE chain SET seq = seq + 1, attempt = 0, scheduled_for = ? WHERE id = 1",
                vec![next.into()],
            )?;
            self.state.storage().set_alarm(at(next)).await?;
        } else {
            sql.exec("DELETE FROM chain", None)?;
        }
        Response::ok("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arm(query: &str) -> std::result::Result<Arm, String> {
        parse_arm(&Url::parse(&format!("https://probe/arm{query}")).expect("url parses"))
    }

    #[test]
    fn arm_defaults_to_one_ok_alarm_two_seconds_out() {
        assert_eq!(
            arm(""),
            Ok(Arm {
                chain: 1,
                gap_ms: DEFAULT_GAP_MS,
                fixture: Fixture::Ok
            })
        );
    }

    #[test]
    fn arm_reads_every_parameter() {
        assert_eq!(
            arm("?chain=10&gap_ms=500&fixture=throw"),
            Ok(Arm {
                chain: 10,
                gap_ms: 500,
                fixture: Fixture::Throw
            })
        );
    }

    #[test]
    fn arm_rejects_out_of_range_and_unknown_input() {
        for query in [
            "?chain=0",
            "?chain=101",
            "?gap_ms=-1",
            "?fixture=boom",
            "?chains=2",
        ] {
            assert!(arm(query).is_err(), "{query} should be rejected");
        }
    }

    #[test]
    fn bearer_key_must_match_exactly() {
        assert!(authorized(Some("Bearer k3y"), "k3y"));
        assert!(!authorized(Some("Bearer k3"), "k3y"));
        assert!(!authorized(Some("k3y"), "k3y"));
        assert!(!authorized(None, "k3y"));
    }
}
