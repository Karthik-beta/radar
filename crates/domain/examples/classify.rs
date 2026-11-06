use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use domain::classify::classify;
use domain::ids::{Login, Timestamp};
use domain::item::Item;
use domain::stage::Stage;
use domain::turn::{Classification, Turn};
use serde::{Deserialize, Serialize};

const USAGE: &str = "usage: classify <fixture.json>...\n       classify --bench <runs> <dir>";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    me: Login,
    item: Item,
    #[expect(dead_code, reason = "parsed only so a bad fixture is rejected")]
    expect: Expect,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(dead_code, reason = "parsed only so a bad fixture is rejected")]
struct Expect {
    turn: Turn,
    stage: Option<Stage>,
    since: Timestamp,
}

#[derive(Serialize)]
struct Output<'a> {
    fixture: &'a str,
    #[serde(flatten)]
    classification: &'a Classification,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [flag, runs, dir] if flag == "--bench" => bench(runs, Path::new(dir)),
        [] => Err(USAGE.to_owned()),
        [flag, ..] if flag.starts_with("--") => Err(USAGE.to_owned()),
        paths => print_all(paths),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn load(path: &Path) -> Result<Fixture, String> {
    let text = std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let de = &mut serde_json::Deserializer::from_str(&text);
    serde_path_to_error::deserialize(de).map_err(|err| format!("{}: {err}", path.display()))
}

fn fixture_name(path: &Path) -> String {
    path.file_stem().map_or_else(
        || path.display().to_string(),
        |s| s.to_string_lossy().into_owned(),
    )
}

fn print_all(paths: &[String]) -> Result<(), String> {
    let mut failures = Vec::new();
    for path in paths.iter().map(Path::new) {
        match load(path) {
            Ok(fixture) => {
                let classification = classify(&fixture.item, &fixture.me);
                let output = Output {
                    fixture: &fixture_name(path),
                    classification: &classification,
                };
                let line = serde_json::to_string(&output).map_err(|err| err.to_string())?;
                println!("{line}");
            }
            Err(message) => failures.push(message),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\nerror: "))
    }
}

fn bench(runs: &str, dir: &Path) -> Result<(), String> {
    let runs: u32 = runs
        .parse()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("runs must be a positive integer, got `{runs}`"))?;
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|err| format!("{}: {err}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    let fixtures = paths
        .iter()
        .map(|path| load(path))
        .collect::<Result<Vec<_>, _>>()?;
    if fixtures.is_empty() {
        return Err(format!("{}: no .json fixtures", dir.display()));
    }

    let start = Instant::now();
    for _ in 0..runs {
        for fixture in &fixtures {
            black_box(classify(black_box(&fixture.item), black_box(&fixture.me)));
        }
    }
    let elapsed = start.elapsed();

    let classified = f64::from(runs) * fixtures.len() as f64;
    let us_per_fixture = elapsed.as_secs_f64() * 1e6 / classified;
    println!(
        "fixtures={} runs={runs} us_per_fixture={us_per_fixture:.3} us_per_20_batch={:.3}",
        fixtures.len(),
        us_per_fixture * 20.0
    );
    Ok(())
}
