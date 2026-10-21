//! Links between docs, and repository paths in inline code, point at
//! files that exist, and a link's `#anchor` at a heading its doc has.

use std::collections::HashMap;
use std::path::Path;

use super::{Doc, Report, code_spans};

/// Where a path in inline code must start for the check to read it as one.
const ROOTS: &[&str] = &[
    "apps/",
    "conformance/",
    "crates/",
    "docs/",
    "resources/",
    "scripts/",
];

pub(super) fn check(docs: &[Doc], by_path: &HashMap<&str, &Doc>, report: &mut Report) {
    for doc in docs {
        for link in doc.links() {
            let Some(path) = &link.path else { continue };
            let place = format!("{}:{}", doc.path, link.line);
            if !Path::new(path).exists() {
                report
                    .errors
                    .push(format!("{place}: `{}` links to nothing", link.target));
                continue;
            }
            let Some(anchor) = &link.anchor else { continue };
            if let Some(target) = by_path.get(path.as_str())
                && !target.anchors().contains(anchor)
            {
                report.errors.push(format!(
                    "{place}: `{}` names a heading {path} doesn't have",
                    link.target
                ));
            }
        }
        for (line, text) in &doc.lines {
            for span in code_spans(text) {
                let Some(path) = repository_path(span) else {
                    continue;
                };
                if !exists_as_command(path) {
                    report.errors.push(format!(
                        "{}:{line}: `{span}` names a path that doesn't exist",
                        doc.path
                    ));
                }
            }
        }
    }
}

/// Whether `path` exists, or names a command by its file without the
/// extension (`scripts/check` for `check.ps1` and `check.sh`).
fn exists_as_command(path: &str) -> bool {
    let path = Path::new(path);
    if path.exists() {
        return true;
    }
    let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    let Ok(entries) = fs_err::read_dir(folder) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| Path::new(&entry.file_name()).file_stem() == Some(name))
}

/// The repository path an inline code span names, without a `:line`.
fn repository_path(span: &str) -> Option<&str> {
    let looks_like_path = ROOTS.iter().any(|root| span.starts_with(root))
        && !span.contains(|c: char| c.is_whitespace() || "*<>{}|…".contains(c))
        && !span.contains("...");
    if !looks_like_path {
        return None;
    }
    Some(match span.rsplit_once(':') {
        Some((path, line)) if line.chars().all(|c| c.is_ascii_digit()) => path,
        _ => span,
    })
}
