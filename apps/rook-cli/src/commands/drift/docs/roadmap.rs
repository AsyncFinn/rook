//! The roadmap's milestone table agrees with each milestone's steps: ✅
//! when every step is ✅, ⬜ when none is, and 🟨 in between.

use std::collections::HashMap;

use super::{Doc, Report};

const INDEX: &str = "docs/roadmap/README.md";

pub(super) fn check(by_path: &HashMap<&str, &Doc>, report: &mut Report) {
    let Some(index) = by_path.get(INDEX) else {
        report.errors.push(format!("{INDEX} is missing"));
        return;
    };
    let links = index.links();
    for (line, text) in &index.lines {
        let Some(mark) = mark(text) else { continue };
        let Some(path) = links
            .iter()
            .find(|link| link.line == *line)
            .and_then(|link| link.path.as_deref())
        else {
            continue;
        };
        let Some(milestone) = by_path.get(path) else {
            continue;
        };
        let steps = steps(milestone);
        let done = steps.iter().filter(|&&done| done).count();
        let fits = match mark {
            '✅' => done == steps.len(),
            '⬜' => done == 0,
            _ => done < steps.len() || steps.is_empty(),
        };
        if !fits {
            report.errors.push(format!(
                "{INDEX}:{line}: marked {mark}, but {done} of {path}'s {} steps are ✅",
                steps.len()
            ));
        }
    }
}

/// The ✅, 🟨 or ⬜ a table row starts with.
fn mark(row: &str) -> Option<char> {
    let first = row.strip_prefix('|')?.split('|').next()?.trim();
    let mut chars = first.chars();
    let mark = chars.next()?;
    (chars.next().is_none() && "✅🟨⬜".contains(mark)).then_some(mark)
}

/// Whether each numbered step is done (starts with ✅).
fn steps(milestone: &Doc) -> Vec<bool> {
    milestone
        .lines
        .iter()
        .filter_map(|(_, text)| {
            let digits = text.chars().take_while(char::is_ascii_digit).count();
            let rest = text[digits..].strip_prefix(". ")?;
            (digits > 0).then(|| rest.starts_with('✅'))
        })
        .collect()
}
