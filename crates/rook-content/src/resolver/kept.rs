//! Content Rook itself loads in nearly every place, kept for good in the
//! data folder (`%LOCALAPPDATA%\Rook\content` on Windows) rather than the
//! download cache, which the system may clear: the built-in materials'
//! texture packs, terrain's, and the images they name.
//!
//! The list is fixed ([`PINNED`], `data/kept.txt`), never what places
//! happen to load, and pins each asset to a version: an asset id can be
//! uploaded again, an asset version never changes. A reference to a kept
//! asset by its bare id is answered with the pinned version, from the kept
//! folder first, then the cache, then asset delivery; what is downloaded
//! goes into the kept folder, which is never trimmed. The folder has the
//! cache's layout ([`super::cache`]), so one reader handles both.
//!
//! `rook content pin` writes the list again from the materials' and
//! terrain's tables, with each asset's version today.

use std::collections::HashMap;
use std::sync::OnceLock;

/// `<asset id> <version>` per line; `#` starts a comment.
const PINNED: &str = include_str!("../../data/kept.txt");

/// The pinned version of a kept asset.
pub fn version(id: u64) -> Option<u32> {
    static TABLE: OnceLock<HashMap<u64, u32>> = OnceLock::new();
    TABLE.get_or_init(|| parse(PINNED)).get(&id).copied()
}

/// Every kept asset and its version.
pub fn all() -> Vec<(u64, u32)> {
    let mut all: Vec<_> = parse(PINNED).into_iter().collect();
    all.sort_unstable();
    all
}

fn parse(text: &str) -> HashMap<u64, u32> {
    text.lines()
        .filter_map(|line| {
            let line = line.split('#').next()?;
            let mut parts = line.split_whitespace();
            Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
        })
        .collect()
}
