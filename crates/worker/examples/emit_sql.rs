//! Prints the statements Radar would send to D1 as literal SQL, one statement
//! per line, so a live lane can run them with `wrangler d1 execute --file`.
//!
//!   cargo run -p radar-worker --features sql-dump --example emit_sql -- \
//!     [--synced-at <epoch>] <fixture.json>...
//!   cargo run -p radar-worker --features sql-dump --example emit_sql -- \
//!     [--synced-at <epoch>] --synthetic <n>
//!   cargo run -p radar-worker --features sql-dump --example emit_sql -- --queries <now-epoch>
//!
//! `--queries` prints the four dashboard reads with `now` filled in.
//!
//! An item fixture is the `domain` classify fixture (`me`, `item`, `expect`)
//! plus an optional `row` object that overrides the generated node id, title,
//! url, head ref, or last-human url. An events fixture is `{"events": [...]}`.

use std::path::Path;
use std::process::ExitCode;

use domain::classify::classify;
use domain::ids::{Login, Timestamp};
use domain::item::Item;
use radar_worker::db::events::{self, EventRow};
use radar_worker::db::items::{self, ItemMeta, ItemRow};
use radar_worker::db::{queries, synthetic, Statement};
use serde::de::IgnoredAny;
use serde::Deserialize;

const USAGE: &str = "usage: emit_sql [--synced-at <epoch>] <fixture.json>...\n       \
                     emit_sql [--synced-at <epoch>] --synthetic <n>";

#[derive(Deserialize)]
struct Probe {
    item: Option<IgnoredAny>,
    events: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemFixture {
    me: Login,
    item: Item,
    #[serde(rename = "expect")]
    _expect: IgnoredAny,
    row: Option<RowOverrides>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RowOverrides {
    node_id: Option<String>,
    title: Option<String>,
    url: Option<String>,
    head_ref: Option<String>,
    last_human_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventsFixture {
    events: Vec<EventRow>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<Vec<String>, String> {
    if let [flag, now] = args {
        if flag == "--queries" {
            return dashboard_queries(now);
        }
    }
    let (synced_at, rest) = match args {
        [flag, value, rest @ ..] if flag == "--synced-at" => {
            let epoch = value
                .parse::<i64>()
                .map_err(|_| format!("--synced-at needs an integer, got `{value}`"))?;
            (Timestamp(epoch), rest)
        }
        _ => (synthetic::NOW, args),
    };
    let statements = match rest {
        [flag, n] if flag == "--synthetic" => {
            let n = n
                .parse::<u32>()
                .map_err(|_| format!("--synthetic needs a count, got `{n}`"))?;
            synthetic::items(n)
                .map_err(|e| e.to_string())?
                .iter()
                .map(items::upsert)
                .collect()
        }
        [] => return Err(USAGE.to_owned()),
        [flag, ..] if flag.starts_with("--") => return Err(USAGE.to_owned()),
        paths => from_fixtures(paths, synced_at)?,
    };
    statements
        .iter()
        .map(|s: &Statement| s.to_literal_sql().map_err(|e| e.to_string()))
        .collect()
}

fn from_fixtures(paths: &[String], synced_at: Timestamp) -> Result<Vec<Statement>, String> {
    let mut statements = Vec::new();
    for path in paths.iter().map(Path::new) {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let fail = |e: serde_json::Error| format!("{}: {e}", path.display());
        let probe: Probe = serde_json::from_str(&text).map_err(fail)?;
        match (probe.item.is_some(), probe.events.is_some()) {
            (true, false) => {
                let fixture: ItemFixture = serde_json::from_str(&text).map_err(fail)?;
                statements.push(items::upsert(&item_row(fixture, synced_at)));
            }
            (false, true) => {
                let fixture: EventsFixture = serde_json::from_str(&text).map_err(fail)?;
                statements.extend(fixture.events.iter().map(events::insert));
            }
            _ => {
                return Err(format!(
                    "{}: expected an item fixture or an events fixture",
                    path.display()
                ))
            }
        }
    }
    Ok(statements)
}

fn item_row(fixture: ItemFixture, synced_at: Timestamp) -> ItemRow {
    let classification = classify(&fixture.item, &fixture.me);
    let mut meta = ItemMeta::placeholder(&fixture.item.item_ref);
    if let Some(row) = fixture.row {
        meta.node_id = row.node_id.unwrap_or(meta.node_id);
        meta.title = row.title.unwrap_or(meta.title);
        meta.url = row.url.unwrap_or(meta.url);
        meta.head_ref = row.head_ref.or(meta.head_ref);
        meta.last_human_url = row.last_human_url.or(meta.last_human_url);
    }
    ItemRow::from_classified(meta, &fixture.item, &fixture.me, &classification, synced_at)
}

fn dashboard_queries(now: &str) -> Result<Vec<String>, String> {
    let now = now
        .parse::<i64>()
        .map(Timestamp)
        .map_err(|_| format!("--queries needs an epoch, got `{now}`"))?;
    [
        queries::your_turn(now),
        queries::their_turn(),
        queries::pipeline(),
        queries::merged_closed(now),
    ]
    .iter()
    .map(|s| s.to_literal_sql().map_err(|e| e.to_string()))
    .collect()
}
