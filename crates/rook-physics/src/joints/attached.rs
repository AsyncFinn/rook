//! Which constraints and movers hold each part, so a part that moves or
//! is rebuilt re-reads only those, as physics engines keep each body's
//! joints listed on it.

use rook_dom::InstanceId;
use rustc_hash::FxHashMap as HashMap;

#[derive(Debug, Default)]
pub(crate) struct Attached {
    /// Each part's constraints and movers.
    holders: HashMap<InstanceId, Vec<InstanceId>>,
    /// Each constraint's or mover's parts, as last read.
    parts: HashMap<InstanceId, Vec<InstanceId>>,
}

impl Attached {
    /// `holder` now holds `parts` (none once it's gone or can't act).
    pub(crate) fn set(&mut self, holder: InstanceId, mut parts: Vec<InstanceId>) {
        parts.sort_unstable();
        parts.dedup();
        if self
            .parts
            .get(&holder)
            .map_or(parts.is_empty(), |p| *p == parts)
        {
            return;
        }
        for part in self.parts.remove(&holder).unwrap_or_default() {
            if let Some(list) = self.holders.get_mut(&part) {
                list.retain(|&h| h != holder);
                if list.is_empty() {
                    self.holders.remove(&part);
                }
            }
        }
        for &part in &parts {
            self.holders.entry(part).or_default().push(holder);
        }
        if !parts.is_empty() {
            self.parts.insert(holder, parts);
        }
    }

    /// The constraints and movers holding `part`.
    pub(crate) fn holders(&self, part: InstanceId) -> impl Iterator<Item = InstanceId> + '_ {
        self.holders.get(&part).into_iter().flatten().copied()
    }
}
