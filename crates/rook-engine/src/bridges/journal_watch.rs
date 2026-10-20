//! A bridge's place in the change journal.

use rook_dom::{Change, Dom};

/// The first look sees the whole tree (a loaded place's instances are there
/// before the journal starts); every later look sees only the journal since
/// the one before.
#[derive(Debug, Default)]
pub(crate) struct JournalWatch {
    cursor: u64,
    scanned: bool,
}

impl JournalWatch {
    /// A watch whose first look sees the journal from `cursor` on.
    pub(crate) fn starting_at(cursor: u64) -> Self {
        Self {
            cursor,
            scanned: false,
        }
    }

    /// Look at the whole tree again from `cursor` (a client DataModel filled
    /// by hand).
    pub(crate) fn rescan(&mut self, cursor: u64) {
        *self = Self::starting_at(cursor);
    }

    /// Whether this is the first look (the caller scans the tree), and the
    /// changes since the last one. The next look starts at the journal's end.
    pub(crate) fn look<'d>(&mut self, dom: &'d Dom) -> (bool, &'d [Change]) {
        let first = !std::mem::replace(&mut self.scanned, true);
        let changes = dom.journal().since(self.cursor);
        self.cursor = dom.journal().end();
        (first, changes)
    }
}
