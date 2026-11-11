#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use radar_worker::github::query::validate_document;

const ALLOWED_WRITERS: &[&str] = &[
    "github/client.rs",
    // Exchanges a login code for a token at /login/oauth/access_token. It never calls the GitHub API.
    "auth/exchange.rs",
    // Names the inbound POST routes /auth/logout and /auth/disconnect. It sends no request.
    "auth/route.rs",
    // Names the inbound POST routes /api/items/:id/dismiss and /snooze. It sends no request.
    "api/route.rs",
    // Names the inbound POST routes /hooks/github and /hooks/checkin. It sends no request.
    "hooks/route.rs",
    // Posts one Telegram sendMessage. It never names a GitHub host, which a test below enforces.
    "alerts/telegram.rs",
];

const GITHUB_NAMES: &[&str] = &[
    "github.com",
    "GITHUB_API_BASE",
    "GITHUB_WEB_BASE",
    "github::",
];

fn github_names_in(text: &str) -> Vec<&'static str> {
    GITHUB_NAMES
        .iter()
        .copied()
        .filter(|name| text.contains(name))
        .collect()
}

const METHOD_PATHS: &[&str] = &[
    "Method::Post",
    "Method::Put",
    "Method::Patch",
    "Method::Delete",
];

const QUOTED_VERBS: &[&str] = &[
    "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "CONNECT", "TRACE",
];

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn files_with_extension(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let entries = fs::read_dir(&next).expect("read_dir");
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == ext) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("under root")
        .to_string_lossy()
        .replace('\\', "/")
}

fn write_method_violations(src: &Path, allowed: &[&str]) -> Vec<String> {
    let mut violations = Vec::new();
    for file in files_with_extension(src, "rs") {
        let rel = relative(src, &file);
        if allowed.contains(&rel.as_str()) {
            continue;
        }
        let text = fs::read_to_string(&file).expect("read source");
        for (index, line) in text.lines().enumerate() {
            let hits = METHOD_PATHS
                .iter()
                .map(|m| (*m).to_owned())
                .chain(QUOTED_VERBS.iter().map(|v| format!("\"{v}\"")))
                .filter(|pattern| line.contains(pattern.as_str()));
            for pattern in hits {
                violations.push(format!("{rel}:{}: {pattern}", index + 1));
            }
        }
    }
    violations
}

fn unregistered_graphql_files(src: &Path) -> Vec<String> {
    let queries = src.join("github/queries");
    let mut invocations = String::new();
    for file in files_with_extension(src, "rs") {
        let text = fs::read_to_string(&file).expect("read source");
        for part in text.split("query_doc!(").skip(1) {
            invocations.push_str(part.split(')').next().unwrap_or_default());
            invocations.push('\n');
        }
    }
    files_with_extension(&queries, "graphql")
        .iter()
        .map(|path| relative(src, path))
        .filter(|rel| {
            let name = rel.rsplit('/').next().unwrap_or_default();
            !invocations.contains(&format!("\"{name}\""))
        })
        .collect()
}

fn default_feature_is_empty(manifest: &str) -> bool {
    let mut in_features = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_features = line == "[features]";
        } else if in_features && line.starts_with("default") {
            let squeezed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            return squeezed == "default=[]";
        }
    }
    true
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("read_only")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn plant(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
    fs::write(path, text).expect("write fixture");
}

#[test]
fn no_write_methods_outside_the_client() {
    let violations = write_method_violations(&src_dir(), ALLOWED_WRITERS);
    assert!(
        violations.is_empty(),
        "write method outside github/client.rs:\n{}",
        violations.join("\n")
    );
}

#[test]
fn hooks_never_reach_the_github_client() {
    let hooks = src_dir().join("hooks");
    for file in files_with_extension(&hooks, "rs") {
        let text = fs::read_to_string(&file).expect("read source");
        for needle in ["crate::github", "github::Client", "Fetch::", "fetch("] {
            assert!(
                !text.contains(needle),
                "{} mentions {needle}",
                relative(&src_dir(), &file)
            );
        }
    }
}

#[test]
fn every_graphql_file_is_registered() {
    let missing = unregistered_graphql_files(&src_dir());
    assert!(
        missing.is_empty(),
        "graphql file not registered through query_doc!: {}",
        missing.join(", ")
    );
}

#[test]
fn every_graphql_file_is_a_query() {
    let queries = src_dir().join("github/queries");
    for file in files_with_extension(&queries, "graphql") {
        let text = fs::read_to_string(&file).expect("read graphql");
        assert!(
            validate_document(&text).is_ok(),
            "{} is not a query-only document",
            relative(&src_dir(), &file)
        );
    }
}

#[test]
fn probe_is_not_a_default_feature() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("read Cargo.toml");
    assert!(
        default_feature_is_empty(&manifest),
        "[features] default in crates/worker/Cargo.toml must be empty"
    );
    assert!(manifest.contains("[features]"), "features table missing");
}

#[test]
fn scanner_fails_on_a_planted_post() {
    let root = scratch("planted_post");
    plant(&root, "lib.rs", "pub fn ok() {}\n");
    plant(
        &root,
        "github/client.rs",
        "fn allowed() { let _ = Method::Post; }\n",
    );
    assert_eq!(
        write_method_violations(&root, ALLOWED_WRITERS),
        Vec::<String>::new()
    );

    plant(
        &root,
        "sync/rogue.rs",
        "fn rogue() {\n    init.with_method(Method::Post);\n}\n",
    );
    plant(&root, "sync/quoted.rs", "const V: &str = \"PUT\";\n");
    plant(&root, "sync/get.rs", "const V: &str = \"GET\";\n");
    let violations = write_method_violations(&root, ALLOWED_WRITERS);
    assert_eq!(
        violations,
        vec![
            "sync/quoted.rs:1: \"PUT\"".to_owned(),
            "sync/rogue.rs:2: Method::Post".to_owned(),
        ]
    );
}

#[test]
fn scanner_fails_on_an_unregistered_graphql_file() {
    let root = scratch("planted_graphql");
    plant(&root, "github/queries/a.graphql", "query A { a }\n");
    plant(&root, "github/queries/b.graphql", "mutation B { b }\n");
    plant(
        &root,
        "github/query.rs",
        "const A: QueryDoc = query_doc!(\"A\", \"a.graphql\");\n",
    );
    assert_eq!(
        unregistered_graphql_files(&root),
        vec!["github/queries/b.graphql".to_owned()]
    );
}

#[test]
fn scanner_fails_on_a_non_empty_default_feature() {
    assert!(default_feature_is_empty(
        "[features]\ndefault = []\nprobe = []\n"
    ));
    assert!(default_feature_is_empty("[features]\nprobe = []\n"));
    assert!(!default_feature_is_empty(
        "[features]\ndefault = [\"probe\"]\nprobe = []\n"
    ));
    assert!(!default_feature_is_empty(
        "[features]\ndefault=[ \"probe\" ]\n"
    ));
}

#[test]
fn telegram_client_never_names_a_github_host() {
    let text = fs::read_to_string(src_dir().join("alerts/telegram.rs")).expect("read telegram.rs");
    assert_eq!(github_names_in(&text), Vec::<&str>::new());
}

#[test]
fn github_host_check_fails_on_a_planted_name() {
    for planted in [
        "let base = env.var(\"GITHUB_API_BASE\");",
        "const URL: &str = \"https://api.github.com\";",
        "use crate::github::client;",
    ] {
        assert!(!github_names_in(planted).is_empty(), "{planted}");
    }
    assert!(github_names_in("let url = format!(\"{base}/bot{token}/sendMessage\");").is_empty());
}

#[test]
fn a_post_outside_the_allow_list_still_fails() {
    let root = scratch("telegram_scope");
    plant(
        &root,
        "alerts/telegram.rs",
        "fn ok() { let _ = Method::Post; }\n",
    );
    plant(
        &root,
        "alerts/mod.rs",
        "fn bad() { let _ = Method::Post; }\n",
    );
    assert_eq!(
        write_method_violations(&root, ALLOWED_WRITERS),
        vec!["alerts/mod.rs:1: Method::Post".to_owned()]
    );
}
