//! `rook-tools gen-api`: update the committed API data (reflection database
//! and member list) to a new Roblox version.

use std::path::Path;

use anyhow::Context;

pub(crate) struct Paths<'a> {
    pub dump: &'a Path,
    pub previous_dump: &'a Path,
    pub database: &'a Path,
    pub members: &'a Path,
}

pub(crate) fn run(paths: &Paths, version: &str, source: &str) -> anyhow::Result<()> {
    let read = |p: &Path| fs_err::read_to_string(p);
    let data = rook_reflection::update::update(
        &fs_err::read(paths.database)?,
        &read(paths.previous_dump)?,
        &read(paths.dump)?,
        version,
        source,
    )?;
    let files = [
        (paths.database, data.database.as_slice()),
        (paths.members, data.members.as_bytes()),
    ];
    for (out, bytes) in files {
        std::fs::write(out, bytes).with_context(|| format!("writing {}", out.display()))?;
    }
    println!("{}", data.report);
    println!(
        "Roblox {version} -> {}, {}",
        paths.database.display(),
        paths.members.display()
    );
    Ok(())
}
