//! Updating the API data to a new Roblox version (`rook-tools gen-api`):
//! the reflection database (`data/database.msgpack`) and the member list
//! (`data/members.json`), both from Roblox's Full API dump.
//!
//! The previous version's dump tells what Roblox changed, so the parts of
//! the database that came from rbx-dom's patches carry over ([`database`]).

mod database;
mod defaults;
mod dump;
mod members;
#[cfg(test)]
mod tests;

use rbx_reflection::ReflectionDatabase;

pub use database::Report;

use crate::Members;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database is not valid MessagePack: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("database doesn't encode: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("unexpected API dump content: {0}")]
    Shape(String),
    #[error("version `{0}` is not `major.minor.patch.build`")]
    Version(String),
}

/// The new files, and what changed.
#[derive(Debug)]
pub struct ApiData {
    pub database: Vec<u8>,
    pub members: String,
    pub report: Report,
}

/// The API data for `dump` (Roblox `version`), from the committed database
/// and the dump it was built from. `source` records where `dump` came from.
pub fn update(
    previous_database: &[u8],
    previous_dump: &str,
    dump: &str,
    version: &str,
    source: &str,
) -> Result<ApiData, UpdateError> {
    let previous: ReflectionDatabase = rmp_serde::from_slice(previous_database)?;
    let old: dump::Root = serde_json::from_str(previous_dump)?;
    let new: dump::Root = serde_json::from_str(dump)?;
    let mut report = Report::default();
    let db = database::update(&previous, &old, &new, parse_version(version)?, &mut report);
    let members = Members::from_api_dump(&new, version, source)?;
    Ok(ApiData {
        database: rmp_serde::to_vec(&db)?,
        members: members.to_json(),
        report,
    })
}

fn parse_version(version: &str) -> Result<[u32; 4], UpdateError> {
    let parts: Vec<u32> = version
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| UpdateError::Version(version.to_owned()))?;
    parts
        .try_into()
        .map_err(|_| UpdateError::Version(version.to_owned()))
}
