//! Every page under `docs/behavior/` opens, under its title, with when it
//! was last checked against Studio: `Checked against Studio 0.743 on
//! 2026-10-21.` Roblox changes Studio every week, so a page checked long
//! ago is pointed out as worth checking again.

use std::time::{SystemTime, UNIX_EPOCH};

use super::{Doc, Report, days};

const PREFIX: &str = "Checked against Studio ";

/// How old a check can get before the page is pointed out.
const MAX_AGE_DAYS: i64 = 90;

pub(super) fn check(docs: &[Doc], report: &mut Report) {
    let today = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| (d.as_secs() / 86_400).cast_signed());
    for doc in docs.iter().filter(|d| d.path.starts_with("docs/behavior/")) {
        let mut lines = doc.lines.iter().filter(|(_, text)| !text.trim().is_empty());
        let _title = lines.next();
        let Some((line, text)) = lines.next() else {
            continue;
        };
        let place = format!("{}:{line}", doc.path);
        match checked(text) {
            None => report.errors.push(format!(
                "{place}: under the title, say when the page was checked: \"{PREFIX}0.743 on 2026-10-21.\""
            )),
            Some(date) if today - date > MAX_AGE_DAYS => report.warnings.push(format!(
                "{place}: last checked against Studio {} days ago; worth checking again",
                today - date
            )),
            Some(_) => {}
        }
    }
}

/// The day of a `Checked against Studio <version> on <date>` line.
pub(super) fn checked(text: &str) -> Option<i64> {
    let (version, rest) = text.strip_prefix(PREFIX)?.split_once(" on ")?;
    let (major, minor) = version.split_once('.')?;
    let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    if !numeric(major) || !numeric(minor) {
        return None;
    }
    let date = rest.get(..10)?;
    if !rest[10..].starts_with(['.', ',']) {
        return None;
    }
    days(date)
}
