//! Attributes and CollectionService tags.

use super::*;

impl Dom {
    pub fn attribute(&self, id: InstanceId, name: &str) -> Option<&Value> {
        self.slot(id)?.attributes.get(name)
    }

    pub fn set_attribute(
        &mut self,
        id: InstanceId,
        name: &str,
        value: Option<Value>,
    ) -> Result<(), DomError> {
        let attrs = &mut self.inst_mut(id)?.attributes;
        let changed = match value {
            Some(v) => attrs.insert(name.to_owned(), v),
            None => attrs.remove(name),
        };
        if changed {
            self.journal.push(Change::Attribute {
                id,
                name: name.to_owned(),
            });
        }
        Ok(())
    }

    pub fn has_tag(&self, id: InstanceId, tag: &str) -> bool {
        self.instances
            .get(id)
            .is_some_and(|i| i.tags.iter().any(|t| t == tag))
    }

    pub fn add_tag(&mut self, id: InstanceId, tag: &str) -> Result<(), DomError> {
        let tags = &mut self.inst_mut(id)?.tags;
        if !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_owned());
            self.tagged.entry(tag.to_owned()).or_default().insert(id);
            self.journal.push(Change::TagAdded {
                id,
                tag: tag.to_owned(),
            });
        }
        Ok(())
    }

    pub fn remove_tag(&mut self, id: InstanceId, tag: &str) -> Result<(), DomError> {
        let tags = &mut self.inst_mut(id)?.tags;
        if let Some(pos) = tags.iter().position(|t| t == tag) {
            tags.remove(pos);
            self.untag(id, tag);
            self.journal.push(Change::TagRemoved {
                id,
                tag: tag.to_owned(),
            });
        }
        Ok(())
    }

    /// Give `id` exactly `tags` without journaling it: an instance being
    /// made (loaded from a file, a clone, a replicated copy).
    pub fn set_tags(&mut self, id: InstanceId, tags: Vec<String>) -> Result<(), DomError> {
        let old = std::mem::take(&mut self.inst_mut(id)?.tags);
        self.untag_all(id, &old);
        for tag in &tags {
            self.tagged.entry(tag.clone()).or_default().insert(id);
        }
        self.inst_mut(id)?.tags = tags;
        Ok(())
    }

    /// The instances in the DataModel with `tag`, in id order (Studio's
    /// order follows neither the tree nor tagging, so scripts can't rely
    /// on one).
    pub fn tagged(&self, tag: &str) -> Vec<InstanceId> {
        let Some(ids) = self.tagged.get(tag) else {
            return Vec::new();
        };
        let mut out: Vec<InstanceId> = (ids.iter().copied())
            .filter(|&id| self.in_data_model(id))
            .collect();
        out.sort_unstable();
        out
    }

    /// Every tag an instance in the DataModel has, sorted.
    pub fn all_tags(&self) -> Vec<String> {
        let mut out: Vec<String> = (self.tagged.iter())
            .filter(|(_, ids)| ids.iter().any(|&id| self.in_data_model(id)))
            .map(|(tag, _)| tag.clone())
            .collect();
        out.sort_unstable();
        out
    }

    fn in_data_model(&self, id: InstanceId) -> bool {
        self.is_ancestor_of(self.root, id)
    }

    fn untag(&mut self, id: InstanceId, tag: &str) {
        if let Some(ids) = self.tagged.get_mut(tag) {
            ids.remove(&id);
            if ids.is_empty() {
                self.tagged.remove(tag);
            }
        }
    }

    pub(crate) fn untag_all(&mut self, id: InstanceId, tags: &[String]) {
        for tag in tags {
            self.untag(id, tag);
        }
    }
}
