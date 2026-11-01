//! Reads Rook's build settings (`webBase` and `webDomains`),
//! `rook-build.toml` at the repository's root with the file
//! `ROOK_BUILD_CONFIG` names read over it, into the constants
//! `src/build_config.rs` includes, with whether they send asset delivery
//! to Roblox. An unknown key or a bad value fails the
//! build with what is wrong and where.
//! See docs/design/data-and-logging.md#build-settings.
#![expect(
    clippy::disallowed_methods,
    reason = "a build script runs on the build machine, outside rook-platform"
)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

use toml::{Table, Value};

/// Where web requests go, as Rook reads it at run time, so the build and
/// the engine agree on what goes to Roblox.
#[path = "../rook-platform/src/endpoints.rs"]
#[expect(dead_code, unreachable_pub, reason = "the build asks only `is_roblox`")]
mod endpoints;

/// The variable naming a distributor's own file.
const CONFIG_VAR: &str = "ROOK_BUILD_CONFIG";

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let root = manifest.join("../..");
    let defaults = root.join("rook-build.toml");
    println!("cargo::rerun-if-changed={}", defaults.display());
    println!("cargo::rerun-if-env-changed={CONFIG_VAR}");
    let mut files = vec![defaults];
    if let Some(path) = env::var_os(CONFIG_VAR).filter(|p| !p.is_empty()) {
        // Relative to the repository, wherever cargo runs from.
        let path = root.join(path);
        println!("cargo::rerun-if-changed={}", path.display());
        files.push(path);
    }
    match settings(&files) {
        Ok(code) => {
            let out = PathBuf::from(env::var("OUT_DIR").expect("set by cargo"));
            fs::write(out.join("build_config.rs"), code).expect("write build_config.rs");
        }
        Err(e) => {
            println!("cargo::error={e}");
            std::process::exit(1);
        }
    }
}

/// Each file read over the one before, then the constants' code.
fn settings(files: &[PathBuf]) -> Result<String, String> {
    let mut merged = Table::new();
    for file in files {
        let table = read(file)?;
        for (key, value) in &table {
            check(key, value).map_err(|e| format!("{}: {key}: {e}", file.display()))?;
        }
        merged.extend(table);
    }
    Ok(generate(&merged))
}

fn read(path: &Path) -> Result<Table, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.parse::<Table>()
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Whether `value` suits `key`; unknown keys are an error.
fn check(key: &str, value: &Value) -> Result<(), String> {
    match key {
        "webBase" => string(value).and_then(|s| {
            (!s.trim().is_empty())
                .then_some(())
                .ok_or_else(|| "empty; leave the key out for Rook's default".to_owned())
        }),
        "webDomains" => {
            let table = value.as_table().ok_or("must be a table of domain = URL")?;
            for (domain, url) in table {
                let url = string(url).map_err(|e| format!("{domain}: {e}"))?;
                if !url.eq_ignore_ascii_case("off") && !url.contains("://") {
                    return Err(format!("{domain}: {url:?} is neither \"off\" nor a URL"));
                }
            }
            Ok(())
        }
        _ => Err(
            "not a build setting: rook-build.toml holds only webBase and webDomains, and the \
             rest are run-time settings (docs/design/data-and-logging.md#settings)"
                .into(),
        ),
    }
}

fn string(value: &Value) -> Result<&str, String> {
    value.as_str().ok_or_else(|| "must be a string".into())
}

fn generate(t: &Table) -> String {
    let mut code = String::new();
    let mut line = |doc: &str, item: String| {
        writeln!(code, "/// {doc}\n{item}").expect("writing to a String");
    };

    let base = t.get("webBase").and_then(Value::as_str);
    line(
        "`webBase`: where web requests go; `None` is Rook's default.",
        format!("pub const WEB_BASE: Option<&str> = {base:?};"),
    );
    let domains: Vec<(&str, &str)> = t
        .get("webDomains")
        .and_then(Value::as_table)
        .into_iter()
        .flatten()
        .map(|(d, url)| (d.as_str(), url.as_str().expect("checked")))
        .collect();
    line(
        "`webDomains`: a base URL or `off` per domain, winning over `WEB_BASE`.",
        format!("pub const WEB_DOMAINS: &[(&str, &str)] = &{domains:?};"),
    );
    let roblox =
        endpoints::Endpoints::new(base, domains.iter().copied()).is_roblox("assetdelivery");
    line(
        "Whether asset delivery goes to Roblox, the one place an Open Cloud key may be sent.",
        format!("pub const ASSET_DELIVERY_ROBLOX: bool = {roblox};"),
    );
    code
}
