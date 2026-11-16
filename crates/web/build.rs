//! Writes the `chart` tokens of `design/tokens.json` as Rust constants, so the
//! charts' sizes come from the same file as the CSS.

use std::fmt::Write as _;
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), String> {
    let manifest = env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
    let tokens = PathBuf::from(manifest).join("../../design/tokens.json");
    println!("cargo:rerun-if-changed={}", tokens.display());
    let text = fs::read_to_string(&tokens).map_err(|e| e.to_string())?;
    let tree: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let chart = tree
        .get("chart")
        .and_then(|group| group.as_object())
        .ok_or("design/tokens.json has no chart group")?;
    let mut out = String::new();
    for (name, token) in chart {
        if name.starts_with('$') {
            continue;
        }
        let value = token["$value"]["value"]
            .as_f64()
            .ok_or("chart tokens are dimensions")?;
        let constant = name.to_uppercase().replace('-', "_");
        writeln!(out, "pub const {constant}: f64 = {value:?};").map_err(|e| e.to_string())?;
    }
    let dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
    fs::write(dir.join("tokens.rs"), out).map_err(|e| e.to_string())
}
