use raven_reflection::PropertyId;

use crate::InstanceId;

/// One recorded mutation.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Created(InstanceId),
    /// A property changed, including `Name` and `Parent`.
    Property {
        id: InstanceId,
        property: PropertyId,
    },
    Attribute {
        id: InstanceId,
        name: String,
    },
    /// `id` moved. Carries what signal dispatch needs to expand it into
    /// `ChildRemoved`/`ChildAdded`, `DescendantRemoving`/`DescendantAdded`
    /// and `AncestryChanged`, as the tree looked at the time of the move.
    Reparented {
        id: InstanceId,
        old_parent: Option<InstanceId>,
        new_parent: Option<InstanceId>,
        /// Former parent first, up to its topmost ancestor.
        old_ancestors: Vec<InstanceId>,
        /// New parent first, up to its topmost ancestor.
        new_ancestors: Vec<InstanceId>,
        /// `id` and its descendants, in pre-order.
        subtree: Vec<InstanceId>,
    },
    /// Recorded before `Destroy` changes anything.
    Destroying(InstanceId),
    TagAdded {
        id: InstanceId,
        tag: String,
    },
    TagRemoved {
        id: InstanceId,
        tag: String,
    },
}

/// The change log. Readers keep a cursor (an absolute position) and read
/// everything after it; the owner trims entries every reader has seen.
#[derive(Debug)]
pub struct Journal {
    base: u64,
    entries: Vec<Change>,
    enabled: bool,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            base: 0,
            entries: Vec::new(),
            enabled: true,
        }
    }
}

impl Journal {
    pub(crate) fn push(&mut self, change: Change) {
        if self.enabled {
            self.entries.push(change);
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Disable while bulk-loading a file so the load isn't replayed as changes.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Absolute position after the last entry.
    pub fn end(&self) -> u64 {
        self.base + self.entries.len() as u64
    }

    /// Entries from `cursor` to the end. Trimmed entries are skipped.
    pub fn since(&self, cursor: u64) -> &[Change] {
        let start = usize::try_from(cursor.saturating_sub(self.base)).unwrap_or(usize::MAX);
        self.entries.get(start..).unwrap_or(&[])
    }

    /// The entry at an absolute position, if it hasn't been trimmed.
    pub fn get(&self, position: u64) -> Option<&Change> {
        let i = position.checked_sub(self.base)?;
        self.entries.get(usize::try_from(i).ok()?)
    }

    /// Drop entries before `cursor`.
    pub fn trim(&mut self, cursor: u64) {
        let n = usize::try_from(cursor.saturating_sub(self.base))
            .unwrap_or(usize::MAX)
            .min(self.entries.len());
        self.entries.drain(..n);
        self.base += n as u64;
    }
}
