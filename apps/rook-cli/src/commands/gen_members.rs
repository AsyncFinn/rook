//! `rook-tools gen-members`: regenerate the committed member list.

use std::path::Path;

use anyhow::Context;
use rook_reflection::Members;

pub(crate) fn run(dump: &Path, source: &str, out: &Path) -> anyhow::Result<()> {
    let text = fs_err::read_to_string(dump)?;
    let version = rook_reflection::roblox_version();
    let members = Members::from_api_dump(&text, &version, source)?;
    std::fs::write(out, members.to_json()).with_context(|| format!("writing {}", out.display()))?;
    let count = |f: fn(&rook_reflection::members::ClassMembers) -> usize| {
        members.classes.values().map(f).sum::<usize>()
    };
    println!(
        "{} classes, {} functions, {} events, {} callbacks for Roblox {version} -> {}",
        members.classes.len(),
        count(|c| c.functions.len()),
        count(|c| c.events.len()),
        count(|c| c.callbacks.len()),
        out.display()
    );
    Ok(())
}
