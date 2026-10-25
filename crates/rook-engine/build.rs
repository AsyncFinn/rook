//! Reads Rook's build settings, `rook-build.toml` at the repository's root
//! with the file `ROOK_BUILD_CONFIG` names read over it, into the
//! constants `src/build_config.rs` includes. An unknown key or a bad value
//! fails the build with what is wrong and where.
//! See docs/design/data-and-logging.md#build-settings.
#![expect(
    clippy::disallowed_methods,
    reason = "a build script runs on the build machine, outside rook-platform"
)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

use toml::{Table, Value};

/// The variable naming a distributor's own file.
const CONFIG_VAR: &str = "ROOK_BUILD_CONFIG";

/// `contentOrder`'s names, and the `ProviderKind` each stands for.
const PROVIDERS: &[(&str, &str)] = &[
    ("defaults", "Defaults"),
    ("install", "Install"),
    ("folders", "Folders"),
    ("delivery", "Delivery"),
];

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
    generate(&merged).map_err(|e| format!("{}: {e}", files[0].display()))
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
        "contentOrder" => {
            let names = value.as_array().ok_or("must be a list of names")?;
            if names.is_empty() {
                return Err("lists no provider, so nothing would load".into());
            }
            let mut seen = Vec::new();
            for name in names {
                let name = string(name)?;
                provider(name)?;
                if seen.contains(&name) {
                    return Err(format!("{name:?} is listed twice"));
                }
                seen.push(name);
            }
            Ok(())
        }
        "coreScripts" | "assetDelivery" | "developerOverrides" => value
            .as_bool()
            .map(drop)
            .ok_or_else(|| "must be true or false".into()),
        "cacheMaxSize" => size(string(value)?).map(drop),
        "threads" => match value.as_integer() {
            Some(n) if n > 0 => Ok(()),
            _ => Err("must be a whole number above 0".into()),
        },
        _ => Err("not a build setting".into()),
    }
}

fn string(value: &Value) -> Result<&str, String> {
    value.as_str().ok_or_else(|| "must be a string".into())
}

fn provider(name: &str) -> Result<&'static str, String> {
    PROVIDERS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, variant)| *variant)
        .ok_or_else(|| {
            let names: Vec<&str> = PROVIDERS.iter().map(|(n, _)| *n).collect();
            format!("{name:?} is not one of {}", names.join(", "))
        })
}

/// `500MB`, `2GB`: a number and a unit, in bytes.
fn size(text: &str) -> Result<u64, String> {
    let t = text.trim().to_ascii_uppercase();
    let split = t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len());
    let (digits, unit) = t.split_at(split);
    let scale: u64 = match unit {
        "KB" => 1 << 10,
        "MB" => 1 << 20,
        "GB" => 1 << 30,
        "TB" => 1 << 40,
        _ => return Err(format!("{text:?} needs a unit: KB, MB, GB or TB")),
    };
    let n: u64 = digits
        .trim()
        .parse()
        .map_err(|_| format!("{text:?} is not a whole number and a unit"))?;
    Ok(n * scale)
}

fn generate(t: &Table) -> Result<String, String> {
    let required = |key: &str| t.get(key).ok_or_else(|| format!("{key} must be set"));
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
    let core = required("coreScripts")?.as_bool().expect("checked");
    line(
        "`coreScripts`: clients run Roblox's CoreScripts.",
        format!("pub const CORE_SCRIPTS: bool = {core};"),
    );
    let order = required("contentOrder")?
        .as_array()
        .expect("checked")
        .iter()
        .map(|n| provider(n.as_str().expect("checked")).map(|v| format!("ProviderKind::{v}")))
        .collect::<Result<Vec<_>, _>>()?;
    line(
        "`contentOrder`: where content comes from, in order.",
        format!(
            "pub const CONTENT_ORDER: &[rook_content::ProviderKind] = {{ use rook_content::ProviderKind; &[{}] }};",
            order.join(", ")
        ),
    );
    let delivery = t.get("assetDelivery").and_then(Value::as_bool);
    line(
        "`assetDelivery`: whether content may be downloaded; `None` leaves it to each program.",
        format!("pub const ASSET_DELIVERY: Option<bool> = {delivery:?};"),
    );
    let max = size(required("cacheMaxSize")?.as_str().expect("checked"))?;
    line(
        "`cacheMaxSize`: the download cache's size limit, in bytes.",
        format!("pub const CACHE_MAX_SIZE: u64 = {max};"),
    );
    let threads = t.get("threads").and_then(Value::as_integer);
    line(
        "`threads`: logical cores to plan background threads for; `None` is the machine's.",
        format!("pub const THREADS: Option<usize> = {threads:?};"),
    );
    let overrides = t
        .get("developerOverrides")
        .and_then(Value::as_bool)
        .map_or_else(|| "cfg!(debug_assertions)".to_owned(), |b| b.to_string());
    line(
        "`developerOverrides`: flags and `ROOK_*` variables may override these settings.",
        format!("pub const DEVELOPER_OVERRIDES: bool = {overrides};"),
    );
    Ok(code)
}
