//! The code checks. Errors: a function copied into another file (bodies
//! over [`MIN_BODY`] characters, so helpers live once, a layer down), an
//! environment read below rook-engine (settings flow down from it), and a
//! file more than [`MAX_DEPTH`] folders below its crate's `src/` (so paths
//! can be guessed). A comment block over [`MAX_COMMENT_LINES`] lines is a
//! warning: findings go in `docs/behavior`. Test code is left out, except
//! for depth. A workspace member without its own `[profile.dev.package]`
//! entry is an error too: it would build at the dependencies' opt-level.

use std::collections::BTreeMap;

use super::source::Source;
use super::{Report, tracked};

const MIN_BODY: usize = 60;
const MAX_COMMENT_LINES: usize = 15;
const MAX_DEPTH: usize = 3;

pub(super) fn check(report: &mut Report) -> anyhow::Result<()> {
    let mut sources = Vec::new();
    for path in tracked(&["*.rs"])? {
        let text = fs_err::read_to_string(&path)?;
        sources.push(Source::new(path, text));
    }
    report.errors.extend(duplicates(&sources));
    report.errors.extend(environment(&sources));
    report.errors.extend(too_deep(&sources));
    report.warnings.extend(long_comments(&sources));
    report.errors.extend(unprofiled_members()?);
    Ok(())
}

/// Members missing from the root manifest's per-crate dev opt-levels.
fn unprofiled_members() -> anyhow::Result<Vec<String>> {
    let root = fs_err::read_to_string("Cargo.toml")?;
    let mut missing = Vec::new();
    for manifest in tracked(&["crates/*/Cargo.toml", "apps/*/Cargo.toml"])? {
        let text = fs_err::read_to_string(&manifest)?;
        let Some(name) = package_name(&text) else {
            continue;
        };
        if !root.contains(&format!("[profile.dev.package.{name}]")) {
            missing.push(format!(
                "{manifest}: add `[profile.dev.package.{name}]` with Rook's opt-level to Cargo.toml"
            ));
        }
    }
    Ok(missing)
}

/// The `name` in a manifest's `[package]` table.
pub(super) fn package_name(manifest: &str) -> Option<&str> {
    let package = manifest.split("[package]").nth(1)?;
    let package = package.split("\n[").next()?;
    package.lines().find_map(|line| {
        let value = line.trim().strip_prefix("name")?.trim().strip_prefix('=')?;
        Some(value.trim().trim_matches('"'))
    })
}

fn duplicates(sources: &[Source]) -> Vec<String> {
    let mut by_code: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for source in sources {
        for f in source.functions() {
            if f.body.len() > MIN_BODY {
                by_code
                    .entry((f.signature, f.body))
                    .or_default()
                    .push(format!("{}:{}", source.path, f.line));
            }
        }
    }
    by_code
        .into_iter()
        .filter(|(_, places)| {
            let file = |p: &String| p.rsplit_once(':').map(|(f, _)| f.to_owned());
            places
                .iter()
                .filter_map(file)
                .collect::<Vec<_>>()
                .windows(2)
                .any(|w| w[0] != w[1])
        })
        .map(|((signature, _), places)| format!("`{signature}` is copied: {}", places.join(", ")))
        .collect()
}

fn long_comments(sources: &[Source]) -> Vec<String> {
    sources
        .iter()
        .flat_map(|s| {
            s.long_comments(MAX_COMMENT_LINES)
                .into_iter()
                .map(move |(line, len)| {
                    format!(
                        "{}:{line}: {len} comment lines; findings go in docs/behavior",
                        s.path
                    )
                })
        })
        .collect()
}

fn environment(sources: &[Source]) -> Vec<String> {
    sources
        .iter()
        .filter(|s| s.path.starts_with("crates/") && !s.path.starts_with("crates/rook-engine/"))
        // Build scripts read what cargo sets for them, not settings.
        .filter(|s| !s.path.ends_with("/build.rs"))
        .filter(|s| s.code_contains("env::var"))
        .map(|s| {
            format!(
                "{} reads the environment; settings belong to rook-engine's settings module",
                s.path
            )
        })
        .collect()
}

fn too_deep(sources: &[Source]) -> Vec<String> {
    sources
        .iter()
        .filter_map(|s| {
            let depth = folders_below_src(&s.path)?;
            (depth > MAX_DEPTH).then(|| {
                format!(
                    "{} is {depth} folders below src/; at most {MAX_DEPTH}, so its path can be guessed",
                    s.path
                )
            })
        })
        .collect()
}

/// How many folders `path` sits in below its crate's `src/`.
pub(super) fn folders_below_src(path: &str) -> Option<usize> {
    let (_, inside) = path.split_once("/src/")?;
    Some(inside.matches('/').count())
}
