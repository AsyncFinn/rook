//! Hierarchy: parents, children, destroy, clone and services.

use super::*;
use crate::properties::ref_target;

impl Dom {
    pub fn parent(&self, id: InstanceId) -> Option<InstanceId> {
        self.instances.get(id).and_then(|i| i.parent)
    }

    pub fn children(&self, id: InstanceId) -> &[InstanceId] {
        self.instances.get(id).map_or(&[], |i| &i.children)
    }

    /// Descendants in pre-order (the order `GetDescendants` returns).
    pub fn descendants(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut out = Vec::new();
        self.collect_descendants(id, &mut out);
        out
    }

    fn collect_descendants(&self, id: InstanceId, out: &mut Vec<InstanceId>) {
        for &child in self.children(id) {
            out.push(child);
            self.collect_descendants(child, out);
        }
    }

    fn collect_post_order(&self, id: InstanceId, out: &mut Vec<InstanceId>) {
        for &child in self.children(id) {
            self.collect_post_order(child, out);
        }
        out.push(id);
    }

    /// Parent first, up to the topmost ancestor.
    pub fn ancestors(&self, id: InstanceId) -> impl Iterator<Item = InstanceId> + '_ {
        let mut cur = self.parent(id);
        std::iter::from_fn(move || {
            let c = cur?;
            cur = self.parent(c);
            Some(c)
        })
    }

    pub fn is_ancestor_of(&self, ancestor: InstanceId, descendant: InstanceId) -> bool {
        self.ancestors(descendant).any(|a| a == ancestor)
    }

    pub fn find_first_child(
        &self,
        id: InstanceId,
        name: &str,
        recursive: bool,
    ) -> Option<InstanceId> {
        if recursive {
            self.descendants(id)
                .into_iter()
                .find(|c| self.name(*c) == name)
        } else {
            self.children(id)
                .iter()
                .copied()
                .find(|c| self.name(*c) == name)
        }
    }

    /// `Instance:FindFirstChildOfClass()`.
    pub fn find_first_child_of_class(&self, id: InstanceId, class: &str) -> Option<InstanceId> {
        // Compare class ids, not names: engine code finds services this way
        // in its inner loops (every joint checks it's in Workspace).
        let mut children = self.children(id).iter().copied();
        let Some(known) = self.db.class_by_name(class) else {
            return children.find(|c| self.class_name(*c) == class);
        };
        children.find(|c| {
            self.instances
                .get(*c)
                .is_some_and(|i| i.class == known.id && i.unknown_class.is_none())
        })
    }

    pub fn is_parent_locked(&self, id: InstanceId) -> bool {
        self.instances.get(id).is_some_and(|i| i.parent_locked)
    }

    pub fn is_destroyed(&self, id: InstanceId) -> bool {
        self.instances.get(id).is_none_or(|i| i.destroyed)
    }

    fn display_parent(&self, p: Option<InstanceId>) -> String {
        p.map_or_else(|| "NULL".to_owned(), |p| self.full_name(p))
    }

    /// Set `Parent`, with Roblox's checks.
    pub fn set_parent(
        &mut self,
        id: InstanceId,
        parent: Option<InstanceId>,
    ) -> Result<(), DomError> {
        let current = self.inst(id)?.parent;
        if let Some(p) = parent {
            self.inst(p)?;
        }
        if self.is_parent_locked(id) {
            return Err(DomError::ParentLocked {
                name: self.full_name(id),
                current: self.display_parent(current),
                new: self.display_parent(parent),
            });
        }
        if current == parent {
            return Ok(());
        }
        if let Some(p) = parent {
            if p == id {
                return Err(DomError::OwnParent(self.full_name(id)));
            }
            if self.is_ancestor_of(id, p) {
                return Err(DomError::Circular {
                    child: self.full_name(id),
                    parent: self.full_name(p),
                });
            }
        }
        self.move_unchecked(id, parent);
        Ok(())
    }

    pub(crate) fn move_unchecked(&mut self, id: InstanceId, parent: Option<InstanceId>) {
        let old_parent = self.instances[id].parent;
        let journal = self.journal.is_enabled();
        let old_ancestors: Vec<_> = if journal {
            old_parent
                .into_iter()
                .chain(old_parent.into_iter().flat_map(|p| self.ancestors(p)))
                .collect()
        } else {
            Vec::new()
        };

        if let Some(op) = old_parent {
            let siblings = &mut self.instances[op].children;
            if let Some(pos) = siblings.iter().position(|c| *c == id) {
                siblings.remove(pos);
            }
        }
        self.instances[id].parent = parent;
        if let Some(np) = parent {
            self.instances[np].children.push(id);
        }

        if journal {
            let new_ancestors: Vec<_> = parent
                .into_iter()
                .chain(parent.into_iter().flat_map(|p| self.ancestors(p)))
                .collect();
            let mut subtree = vec![id];
            self.collect_descendants(id, &mut subtree);
            let mut subtree_post_order = Vec::with_capacity(subtree.len());
            self.collect_post_order(id, &mut subtree_post_order);
            self.journal.push(Change::Reparented {
                id,
                old_parent,
                new_parent: parent,
                old_ancestors,
                new_ancestors,
                subtree,
                subtree_post_order,
            });
            let property = self.parent_prop;
            self.journal.push(Change::Property { id, property });
        }
    }

    /// `Instance:Destroy()`: fire `Destroying`, set Parent to nil, lock it,
    /// and destroy every descendant.
    pub fn destroy(&mut self, id: InstanceId) -> Result<(), DomError> {
        if self.inst(id)?.destroyed {
            return Ok(());
        }
        if id == self.root {
            return Err(DomError::ParentLocked {
                name: self.full_name(id),
                current: "NULL".to_owned(),
                new: "NULL".to_owned(),
            });
        }
        self.journal.push(Change::Destroying(id));
        let inst = &mut self.instances[id];
        inst.destroyed = true;
        // A locked instance (a service, or one already destroyed) still
        // gets destroyed; only the parent assignment is skipped.
        if inst.parent.is_some() {
            self.move_unchecked(id, None);
        }
        self.instances[id].parent_locked = true;
        for child in self.children(id).to_vec() {
            self.destroy(child)?;
        }
        Ok(())
    }

    /// `Instance:ClearAllChildren()`: destroy every child.
    pub fn clear_all_children(&mut self, id: InstanceId) -> Result<(), DomError> {
        for child in self.inst(id)?.children.clone() {
            // Locked children (services) are skipped, as in Roblox.
            if !self.is_parent_locked(child) {
                self.destroy(child)?;
            }
        }
        Ok(())
    }

    /// Remove an instance and its descendants from the arena entirely.
    /// Called by the engine once nothing references a destroyed instance.
    pub fn remove(&mut self, id: InstanceId) {
        if id == self.root {
            return;
        }
        if self.parent(id).is_some() {
            self.move_unchecked(id, None);
        }
        let mut doomed = vec![id];
        self.collect_descendants(id, &mut doomed);
        for d in doomed {
            if let Some(inst) = self.instances.remove(d) {
                for (property, value) in &inst.properties {
                    self.unlink(d, *property, value);
                }
            }
            self.referrers.remove(&d);
        }
    }

    /// `Instance:Clone()`. Returns `None` when `id` isn't archivable;
    /// non-archivable descendants are skipped. References to instances
    /// inside the cloned tree point at the copies.
    pub fn clone_instance(&mut self, id: InstanceId) -> Option<InstanceId> {
        if !self.archivable(id) {
            return None;
        }
        let mut map = HashMap::new();
        let new = self.clone_rec(id, None, &mut map);
        for &copy in map.values() {
            for value in self.instances[copy].properties.values_mut() {
                if let Value::Ref(Some(r)) = value
                    && let Some(mapped) = map.get(&InstanceId::from_ref(*r))
                {
                    *r = mapped.to_ref();
                }
            }
            let refs: Vec<_> = self.instances[copy]
                .properties
                .iter()
                .filter_map(|(p, v)| ref_target(v).map(|t| (*p, t)))
                .collect();
            for (property, target) in refs {
                self.referrers
                    .entry(target)
                    .or_default()
                    .push((copy, property));
            }
        }
        Some(new)
    }

    fn clone_rec(
        &mut self,
        id: InstanceId,
        parent: Option<InstanceId>,
        map: &mut HashMap<InstanceId, InstanceId>,
    ) -> InstanceId {
        let src = &self.instances[id];
        let mut copy = Instance::new(src.class, src.name.clone());
        copy.unknown_class = src.unknown_class.clone();
        copy.properties = src.properties.clone();
        copy.unknown_properties = src.unknown_properties.clone();
        copy.attributes = src.attributes.clone();
        copy.tags = src.tags.clone();
        copy.parent = parent;
        let children = src.children.clone();
        let new = self.instances.insert(copy);
        self.journal.push(Change::Created(new));
        map.insert(id, new);
        for child in children {
            if self.archivable(child) {
                let c = self.clone_rec(child, Some(new), map);
                self.instances[new].children.push(c);
            }
        }
        new
    }

    fn archivable(&self, id: InstanceId) -> bool {
        !matches!(
            self.property_by_name(id, "Archivable"),
            Some(Value::Bool(false))
        )
    }

    /// `DataModel:FindService()`.
    pub fn find_service(&self, class: &str) -> Option<InstanceId> {
        self.find_first_child_of_class(self.root, class)
    }

    /// `DataModel:GetService()`: find the service, creating it if needed.
    pub fn get_service(&mut self, class: &str) -> Result<InstanceId, DomError> {
        if let Some(s) = self.find_service(class) {
            return Ok(s);
        }
        let info = self
            .db
            .class_by_name(class)
            .filter(|c| c.flags.service)
            .ok_or_else(|| DomError::NotAService(class.to_owned()))?;
        let id = self.create(info.id);
        self.move_unchecked(id, Some(self.root));
        self.instances[id].parent_locked = true;
        Ok(id)
    }
}
