//! Every doc has one `#` title, its first heading; headings are named by topic, not numbered
//! or tagged with the roadmap step that found them; a doc's file name is
//! words of its title; every doc under `docs/` is in an index; and no
//! folder there holds a lone doc.

use std::collections::{BTreeMap, HashMap};

use super::{Doc, Report, slug};

/// The index every doc under `docs/` is listed in, unless its own
/// folder's README lists it.
const INDEX: &str = "docs/README.md";

pub(super) fn check(docs: &[Doc], by_path: &HashMap<&str, &Doc>, report: &mut Report) {
    for doc in docs {
        headings(doc, report);
        if doc.path.starts_with("docs/") && !doc.path.ends_with("/README.md") {
            file_name(doc, report);
            indexed(doc, by_path, report);
        }
    }
    lone_docs(docs, report);
}

fn headings(doc: &Doc, report: &mut Report) {
    let headings = doc.headings();
    if headings.first().is_none_or(|h| h.level != 1) {
        report
            .errors
            .push(format!("{}: doesn't start with a `#` title", doc.path));
    }
    let titles = headings.iter().filter(|h| h.level == 1).count();
    if titles > 1 {
        report
            .errors
            .push(format!("{}: has {titles} `#` titles; one", doc.path));
    }
    for heading in &headings {
        let place = format!("{}:{}", doc.path, heading.line);
        if numbered(heading.text) {
            report.errors.push(format!(
                "{place}: \"{}\" is numbered; name headings by topic, and link them by anchor",
                heading.text
            ));
        }
        let lower = heading.text.to_lowercase();
        if lower.contains("(step ") || lower.contains("(part ") {
            report.errors.push(format!(
                "{place}: \"{}\" names a roadmap step; the changelog keeps those",
                heading.text
            ));
        }
    }
}

/// The file name is the title's words, or some of them.
fn file_name(doc: &Doc, report: &mut Report) {
    let Some(title) = doc.headings().into_iter().find(|h| h.level == 1) else {
        return;
    };
    let title_words: Vec<String> = slug(title.text).split('-').map(str::to_owned).collect();
    let stem = doc.path.rsplit('/').next().unwrap_or_default();
    let stem = stem.strip_suffix(".md").unwrap_or(stem);
    let missing: Vec<&str> = stem
        .split('-')
        .filter(|word| !title_words.iter().any(|t| t == word))
        .collect();
    if !missing.is_empty() {
        report.errors.push(format!(
            "{}: its name has \"{}\", which its title \"{}\" doesn't",
            doc.path,
            missing.join("\", \""),
            title.text
        ));
    }
}

fn indexed(doc: &Doc, by_path: &HashMap<&str, &Doc>, report: &mut Report) {
    let folder_index = format!("{}/README.md", doc.folder());
    let listed = [INDEX, folder_index.as_str()].into_iter().any(|index| {
        by_path.get(index).is_some_and(|index| {
            index
                .links()
                .iter()
                .any(|link| link.path.as_deref() == Some(doc.path.as_str()))
        })
    });
    if !listed {
        report.errors.push(format!(
            "{}: isn't listed in {INDEX} (or its folder's README.md)",
            doc.path
        ));
    }
}

/// A folder under `docs/` with one doc and no subfolders: the doc belongs
/// a level up, or the topic needs its second doc first.
fn lone_docs(docs: &[Doc], report: &mut Report) {
    let mut folders: BTreeMap<&str, (usize, bool)> = BTreeMap::new();
    for doc in docs.iter().filter(|d| d.path.starts_with("docs/")) {
        folders.entry(doc.folder()).or_default().0 += 1;
        let mut folder = doc.folder();
        while let Some((parent, _)) = folder.rsplit_once('/') {
            folders.entry(parent).or_default().1 = true;
            folder = parent;
        }
    }
    for (folder, (count, has_folders)) in folders {
        if count == 1 && !has_folders {
            report.errors.push(format!(
                "{folder}/ holds a single doc; move it up a level until its topic has a second"
            ));
        }
    }
}

/// Whether a heading starts with a section number (`8.`, `5a.`).
fn numbered(heading: &str) -> bool {
    let digits = heading.chars().take_while(char::is_ascii_digit).count();
    let rest = &heading[digits..];
    let rest = rest
        .strip_prefix(|c: char| c.is_ascii_lowercase())
        .unwrap_or(rest);
    digits > 0 && rest.starts_with('.')
}
