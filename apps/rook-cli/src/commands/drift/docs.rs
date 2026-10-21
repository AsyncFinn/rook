//! The documentation checks, over every Markdown file git tracks: links
//! and code paths resolve ([`links`]); titles, headings, file names and
//! the indexes agree ([`structure`]); every behavior page says when it was
//! last checked against Studio ([`behavior`]); the roadmap's marks agree
//! with its steps ([`roadmap`]); and the changelog's releases run newest
//! first ([`changelog`]).

mod behavior;
mod changelog;
mod links;
mod roadmap;
mod structure;

use std::collections::{HashMap, HashSet};

use super::{Report, tracked};

pub(super) fn check(report: &mut Report) -> anyhow::Result<()> {
    let mut docs = Vec::new();
    for path in tracked(&["*.md"])? {
        let text = fs_err::read_to_string(&path)?;
        docs.push(Doc::new(path, &text));
    }
    let by_path: HashMap<&str, &Doc> = docs.iter().map(|d| (d.path.as_str(), d)).collect();
    links::check(&docs, &by_path, report);
    structure::check(&docs, &by_path, report);
    behavior::check(&docs, report);
    roadmap::check(&by_path, report);
    changelog::check(&by_path, report);
    Ok(())
}

/// A Markdown file, without its fenced code blocks.
#[derive(Debug)]
pub(super) struct Doc {
    pub(super) path: String,
    /// The lines outside fenced code, as (line number, text).
    pub(super) lines: Vec<(usize, String)>,
}

#[derive(Debug)]
pub(super) struct Heading<'a> {
    pub(super) line: usize,
    pub(super) level: usize,
    pub(super) text: &'a str,
}

/// A link in a doc: where it points, resolved from the repository's root
/// (`None` for a web link), and its `#anchor`.
#[derive(Debug)]
pub(super) struct Link {
    pub(super) line: usize,
    pub(super) target: String,
    pub(super) path: Option<String>,
    pub(super) anchor: Option<String>,
}

impl Doc {
    pub(super) fn new(path: String, text: &str) -> Self {
        let mut lines = Vec::new();
        let mut fence: Option<&str> = None;
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            let marker = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m));
            match (fence, marker) {
                (None, Some(m)) => fence = Some(m),
                (Some(open), Some(m)) if open == m => fence = None,
                (None, None) => lines.push((i + 1, line.to_owned())),
                _ => {}
            }
        }
        Self { path, lines }
    }

    /// The folder the doc is in, from the repository's root ("" at the
    /// root).
    pub(super) fn folder(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(folder, _)| folder)
    }

    pub(super) fn headings(&self) -> Vec<Heading<'_>> {
        self.lines
            .iter()
            .filter_map(|(line, text)| {
                let level = text.chars().take_while(|&c| c == '#').count();
                let rest = text[level..].strip_prefix(' ')?;
                (level > 0).then_some(Heading {
                    line: *line,
                    level,
                    text: rest.trim(),
                })
            })
            .collect()
    }

    /// The `#anchor`s GitHub gives the headings: the slug, then `-1`,
    /// `-2` for repeats.
    pub(super) fn anchors(&self) -> HashSet<String> {
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut out = HashSet::new();
        for heading in self.headings() {
            let slug = slug(heading.text);
            let count = seen.entry(slug.clone()).or_default();
            out.insert(if *count == 0 {
                slug
            } else {
                format!("{slug}-{count}")
            });
            *count += 1;
        }
        out
    }

    pub(super) fn links(&self) -> Vec<Link> {
        let mut out = Vec::new();
        for (line, text) in &self.lines {
            let text = without_code(text);
            let mut rest = text.as_str();
            while let Some(at) = rest.find("](") {
                rest = &rest[at + 2..];
                let Some(end) = rest.find(')') else { break };
                let target = rest[..end].trim();
                rest = &rest[end..];
                if target.is_empty() {
                    continue;
                }
                let (file, anchor) = match target.split_once('#') {
                    Some((file, anchor)) => (file, Some(anchor.to_owned())),
                    None => (target, None),
                };
                let web = target.contains("://") || target.starts_with("mailto:");
                let path = if web {
                    None
                } else if file.is_empty() {
                    Some(self.path.clone())
                } else {
                    Some(join(self.folder(), file).unwrap_or_else(|| format!("../{file}")))
                };
                out.push(Link {
                    line: *line,
                    target: target.to_owned(),
                    path,
                    anchor,
                });
            }
        }
        out
    }
}

/// GitHub's anchor for a heading: lowercase, spaces to hyphens, and every
/// other character that isn't a letter, digit, `-` or `_` dropped.
pub(super) fn slug(heading: &str) -> String {
    heading
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// `line` with inline code blanked out, so links in it don't count.
pub(super) fn without_code(line: &str) -> String {
    let mut inside = false;
    line.chars()
        .map(|c| {
            if c == '`' {
                inside = !inside;
                ' '
            } else if inside {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// The inline code spans of `line`.
pub(super) fn code_spans(line: &str) -> Vec<&str> {
    line.split('`').skip(1).step_by(2).collect()
}

/// `relative` from `folder`, both from the repository's root, with `.`
/// and `..` worked out; `None` above the root.
pub(super) fn join(folder: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = folder.split('/').filter(|p| !p.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// Days since 1970-01-01 of a `YYYY-MM-DD` date (Howard Hinnant's
/// `days_from_civil`).
pub(super) fn days(date: &str) -> Option<i64> {
    let mut fields = date.split('-');
    let year: i64 = fields.next()?.parse().ok()?;
    let month: i64 = fields.next()?.parse().ok()?;
    let day: i64 = fields.next()?.parse().ok()?;
    let valid = date.len() == 10 && (1..=12).contains(&month) && (1..=31).contains(&day);
    if !valid || fields.next().is_some() {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

#[cfg(test)]
mod tests;
