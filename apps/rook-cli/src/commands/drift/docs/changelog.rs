//! The changelog opens with `[Unreleased]`, then each release as
//! `## [x.y.z] - YYYY-MM-DD - ...`, newest first.

use std::collections::HashMap;

use super::{Doc, Report, days};

const CHANGELOG: &str = "CHANGELOG.md";

pub(super) fn check(by_path: &HashMap<&str, &Doc>, report: &mut Report) {
    let Some(changelog) = by_path.get(CHANGELOG) else {
        return;
    };
    let releases: Vec<_> = changelog
        .headings()
        .into_iter()
        .filter(|h| h.level == 2 && h.text.starts_with('['))
        .collect();
    if releases.first().is_none_or(|h| h.text != "[Unreleased]") {
        report.errors.push(format!(
            "{CHANGELOG}: its first release heading isn't [Unreleased]"
        ));
    }
    let mut previous: Option<((u32, u32, u32), i64)> = None;
    for heading in releases.iter().skip(1) {
        let place = format!("{CHANGELOG}:{}", heading.line);
        let Some(release) = release(heading.text) else {
            report.errors.push(format!(
                "{place}: \"{}\" isn't `[x.y.z] - YYYY-MM-DD - ...`",
                heading.text
            ));
            continue;
        };
        if let Some((version, day)) = previous
            && (release.0 >= version || release.1 > day)
        {
            report
                .errors
                .push(format!("{place}: releases run newest first"));
        }
        previous = Some(release);
    }
}

/// The version and day of a `[x.y.z] - YYYY-MM-DD` heading.
fn release(heading: &str) -> Option<((u32, u32, u32), i64)> {
    let (version, rest) = heading.strip_prefix('[')?.split_once(']')?;
    let mut numbers = version.split('.').map(str::parse::<u32>);
    let version = (
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
    );
    let date = rest.strip_prefix(" - ")?.get(..10)?;
    Some((version, days(date)?))
}
