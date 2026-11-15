//! StyleSheets: StyleRules set properties on what their selectors match in
//! the trees StyleLinks attach their sheets to (creator-docs "Styling").
//!
//! Each frame, before GUI lays out, the trees that changed (instances
//! added or retagged, a state changing) are styled again; a change to
//! the rules or sheets styles every linked tree again. A rule's values
//! apply only to properties the instance hasn't set itself, so what a
//! script sets wins; `$Name` values are the sheet's attribute `Name`
//! (tokens), looked for in the sheets it derives from too. Rules of
//! higher `Priority` win, then later rules.

mod cascade;
mod selector;

pub(crate) use cascade::restyle;

use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;
use std::sync::LazyLock;

use mlua::{Lua, RegistryKey, Table, Value as LuaValue};
use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::ClassId;

use crate::runtime::ctx::with_world;
use selector::Chain;

/// The classes whose changes restyle every linked tree, resolved once:
/// styling looks at every journal entry.
fn style_classes() -> &'static [ClassId; 4] {
    static CLASSES: LazyLock<[ClassId; 4]> = LazyLock::new(|| {
        let db = rook_reflection::database();
        ["StyleRule", "StyleSheet", "StyleLink", "StyleDerive"]
            .map(|name| db.class_by_name(name).expect(name).id)
    });
    &CLASSES
}

#[derive(Debug, Default)]
pub(crate) struct Styles {
    cursor: u64,
    /// Each rule's properties (`SetProperties`), a Lua table.
    pub(crate) properties: FxHashMap<InstanceId, RegistryKey>,
    /// Parsed selectors, by rule, with the text they came from.
    parsed: FxHashMap<InstanceId, (String, Rc<Vec<Chain>>)>,
    /// StyleLinks seen, checked when used.
    links: FxHashSet<InstanceId>,
    /// Whether the DataModel has been searched for StyleLinks yet.
    scanned: bool,
    /// Every linked tree styles again.
    all: bool,
    /// Subtrees to style again.
    pending: FxHashSet<InstanceId>,
    /// Properties styles set on each instance, by name.
    styled: FxHashMap<InstanceId, FxHashSet<String>>,
    /// Pseudo-instances made, by what they're under and their class.
    pseudo: FxHashMap<(InstanceId, String), InstanceId>,
}

impl Styles {
    /// A script set `name` on `id`: the property is its own from now on,
    /// so a rule that stops matching leaves it be rather than resetting
    /// it (the player list sets its own colors when hovered and left).
    pub(crate) fn script_wrote(&mut self, id: InstanceId, name: &str) {
        if let Some(names) = self.styled.get_mut(&id) {
            names.remove(name);
        }
    }

    /// Style `id`'s tree again at the next restyle.
    pub(crate) fn restyle_later(&mut self, id: InstanceId) {
        self.pending.insert(id);
    }

    /// Drop what styling keeps about `id`, which is being freed.
    pub(crate) fn forget(&mut self, id: InstanceId) {
        self.properties.remove(&id);
        self.parsed.remove(&id);
        self.links.remove(&id);
        self.pending.remove(&id);
        self.styled.remove(&id);
        self.pseudo
            .retain(|(under, _), made| *under != id && *made != id);
    }

    /// Rules changed: style every linked tree again.
    pub(crate) fn invalidate(&mut self) {
        self.all = true;
    }

    /// Note what the journal says must be styled again: called before each
    /// restyle, and before the journal is trimmed at the end of a frame
    /// (scripts change most things after the restyle).
    pub(crate) fn note_changes(&mut self, dom: &Dom) {
        let is_style = |id: InstanceId| style_classes().iter().any(|&c| dom.is_a_class(id, c));
        let changes = dom.journal().since(self.cursor);
        rook_types::count!(JournalReadStyles, changes.len());
        for change in changes {
            match change {
                Change::Reparented { id, subtree, .. } => {
                    for &d in subtree {
                        if dom.class_name(d) == "StyleLink" {
                            self.links.insert(d);
                        }
                    }
                    if subtree.iter().any(|&d| is_style(d)) {
                        self.all = true;
                    } else {
                        self.pending.insert(*id);
                    }
                }
                Change::Property { id, .. } | Change::Attribute { id, .. } if is_style(*id) => {
                    self.all = true;
                }
                Change::Property { id, property }
                    if dom.database().property(*property).name == "GuiState" =>
                {
                    self.pending.insert(*id);
                }
                Change::TagAdded { id, .. } | Change::TagRemoved { id, .. } => {
                    self.pending.insert(*id);
                }
                _ => {}
            }
        }
        self.cursor = dom.journal().end();
    }
}
