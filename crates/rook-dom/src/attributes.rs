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
            self.journal.push(Change::TagRemoved {
                id,
                tag: tag.to_owned(),
            });
        }
        Ok(())
    }
}
