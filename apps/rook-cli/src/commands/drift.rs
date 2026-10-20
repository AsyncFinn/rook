//! `rook-tools drift`: the checks `scripts/check` runs against the codebase
//! drifting (CONTRIBUTING, "Keeping it clean"). Errors fail the check;
//! warnings point at something worth a look.
//!
//! - Two functions in different files with the same signature and body
//!   (whitespace and comments aside, bodies over [`MIN_BODY`] characters)
//!   are an error: the helper belongs one layer down, once.
//! - A comment block over [`MAX_COMMENT_LINES`] lines is a warning:
//!   findings go in `docs/behavior`.
//! - Reading the environment in a crate other than rook-engine and the
//!   apps is an error: settings flow down from rook-engine.
//!
//! Test code (`tests.rs`, `tests/`, after `#[cfg(test)]`) is left out.

mod source;

use std::collections::BTreeMap;

use source::Source;

const MIN_BODY: usize = 60;
const MAX_COMMENT_LINES: usize = 15;

pub(crate) fn run() -> anyhow::Result<()> {
    let listed = std::process::Command::new("git")
        .args(["ls-files", "*.rs"])
        .output()?;
    anyhow::ensure!(listed.status.success(), "git ls-files failed");
    let mut sources = Vec::new();
    for path in String::from_utf8(listed.stdout)?.lines() {
        let text = fs_err::read_to_string(path)?;
        sources.push(Source::new(path.to_owned(), text));
    }
    let mut errors = duplicates(&sources);
    errors.extend(environment(&sources));
    for warning in long_comments(&sources) {
        println!("warning: {warning}");
    }
    for error in &errors {
        println!("error: {error}");
    }
    anyhow::ensure!(errors.is_empty(), "{} drift error(s)", errors.len());
    Ok(())
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
        .filter(|s| s.code_contains("env::var"))
        .map(|s| {
            format!(
                "{} reads the environment; settings belong to rook-engine's settings module",
                s.path
            )
        })
        .collect()
}
