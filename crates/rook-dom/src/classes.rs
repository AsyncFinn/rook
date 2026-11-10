//! Instances by class, for the few classes callers look up often (spawn
//! points on every respawn), so a lookup costs what it finds rather than
//! a walk of the tree.

use super::*;

impl Dom {
    /// Keep the instances of `class` (and its subclasses) indexed from now
    /// on, for [`Dom::instances_of`]. Finds those already made once.
    pub fn track_class(&mut self, class: ClassId) {
        if self.of_class.contains_key(&class) {
            return;
        }
        let db = self.db;
        let found = (self.instances.iter())
            .filter(|(_, i)| db.is_a(i.class, class))
            .map(|(id, _)| id)
            .collect();
        self.of_class.insert(class, found);
    }

    /// The instances of tracked `class`, in the tree or not, in id order;
    /// `None` when `class` isn't tracked.
    pub fn instances_of(&self, class: ClassId) -> Option<Vec<InstanceId>> {
        let mut out: Vec<InstanceId> = self.of_class.get(&class)?.iter().copied().collect();
        out.sort_unstable();
        Some(out)
    }

    /// Index `id`, just made, under the tracked classes it is.
    pub(crate) fn note_class(&mut self, id: InstanceId) {
        let class = self.instances[id].class;
        let db = self.db;
        for (tracked, ids) in &mut self.of_class {
            if db.is_a(class, *tracked) {
                ids.insert(id);
            }
        }
    }

    pub(crate) fn forget_class(&mut self, id: InstanceId, class: ClassId) {
        let db = self.db;
        for (tracked, ids) in &mut self.of_class {
            if db.is_a(class, *tracked) {
                ids.remove(&id);
            }
        }
    }
}
