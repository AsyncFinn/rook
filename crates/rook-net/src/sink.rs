use super::*;

/// The client half: which client instance copies which server one.
#[derive(Debug, Default)]
pub struct Sink {
    to_client: HashMap<InstanceId, InstanceId>,
    to_server: HashMap<InstanceId, InstanceId>,
}

impl Sink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn client_id(&self, server: InstanceId) -> Option<InstanceId> {
        self.to_client.get(&server).copied()
    }

    pub fn server_id(&self, client: InstanceId) -> Option<InstanceId> {
        self.to_server.get(&client).copied()
    }

    fn map(&mut self, server: InstanceId, client: InstanceId) {
        self.to_client.insert(server, client);
        self.to_server.insert(client, server);
    }

    fn unmap_subtree(&mut self, client: &Dom, root: InstanceId) {
        let mut ids = vec![root];
        ids.extend(client.descendants(root));
        for c in ids {
            if let Some(s) = self.to_server.remove(&c) {
                self.to_client.remove(&s);
            }
        }
    }

    /// Fill a fresh client DataModel. The client's journal is not touched
    /// (like loading a file).
    pub fn snapshot(&mut self, snapshot: &Snapshot, client: &mut Dom) {
        let was = client.journal().is_enabled();
        client.journal_mut().set_enabled(false);
        self.map(snapshot.root.id, client.root());
        for (class, service) in &snapshot.services {
            let Ok(cid) = client.get_service(class) else {
                continue;
            };
            self.map(service.id, cid);
            self.copy_data(service, client, cid, false);
            for child in &service.children {
                let c = self.build(child, client);
                client.attach_unchecked(c, cid);
            }
        }
        // References to instances created later in the walk.
        self.copy_data(&snapshot.root, client, client.root(), false);
        for (_, service) in &snapshot.services {
            self.copy_tree(service, client);
        }
        client.journal_mut().set_enabled(was);
    }

    fn copy_tree(&self, node: &Node, client: &mut Dom) {
        if let Some(c) = self.client_id(node.id) {
            self.copy_data(node, client, c, false);
        }
        for child in &node.children {
            self.copy_tree(child, client);
        }
    }

    /// Apply updates from the server. The caller fires client signals for
    /// the client journal entries this produces.
    pub fn apply(&mut self, updates: &[Update], client: &mut Dom) {
        profiling::scope!("Sink::apply");
        for update in updates {
            match update {
                Update::Place { parent, tree } => {
                    let Some(parent) = self.client_id(*parent) else {
                        continue;
                    };
                    let c = match self.client_id(tree.id) {
                        Some(c) => c,
                        None => self.build(tree, client),
                    };
                    let _ = client.set_parent(c, Some(parent));
                }
                Update::Hide(id) | Update::Destroying(id) => {
                    if let Some(c) = self.client_id(*id) {
                        self.unmap_subtree(client, c);
                        let _ = client.destroy(c);
                    }
                }
                Update::Name { id, name } => {
                    if let Some(c) = self.client_id(*id) {
                        let _ = client.set_name(c, name.clone());
                    }
                }
                Update::Property {
                    id,
                    property,
                    value,
                } => {
                    if let Some(c) = self.client_id(*id) {
                        let _ = client.set_property(c, *property, self.to_client_value(value));
                    }
                }
                Update::Attribute { id, name, value } => {
                    if let Some(c) = self.client_id(*id) {
                        let _ = client.set_attribute(c, name, value.clone());
                    }
                }
                Update::TagAdded { id, tag } => {
                    if let Some(c) = self.client_id(*id) {
                        let _ = client.add_tag(c, tag);
                    }
                }
                Update::TagRemoved { id, tag } => {
                    if let Some(c) = self.client_id(*id) {
                        let _ = client.remove_tag(c, tag);
                    }
                }
            }
        }
    }

    /// Build a detached copy of a server subtree on the client.
    fn build(&mut self, node: &Node, client: &mut Dom) -> InstanceId {
        let cid = match &node.class {
            NodeClass::Unknown(name) => client.create_unknown(name),
            NodeClass::Known(class) => client.create(*class),
        };
        let _ = client.set_name(cid, node.name.clone());
        self.map(node.id, cid);
        for child in &node.children {
            let c = self.build(child, client);
            // Detached parent: no checks needed, no one is listening.
            let _ = client.set_parent(c, Some(cid));
        }
        self.copy_data(node, client, cid, false);
        cid
    }

    /// Store a node's properties, attributes and tags on its copy.
    fn copy_data(&self, node: &Node, client: &mut Dom, cid: InstanceId, journaled: bool) {
        for (property, value) in &node.properties {
            let value = self.to_client_value(value);
            if journaled {
                let _ = client.set_property(cid, *property, value);
            } else {
                client.load_property(cid, *property, value);
            }
        }
        if let Some(c) = client.instance_mut(cid) {
            for (k, v) in node.attributes.iter() {
                c.attributes.insert(k.to_owned(), v.clone());
            }
            c.tags.clone_from(&node.tags);
        }
    }

    /// References follow the instance to its client copy, or become nil if
    /// the client can't see it.
    fn to_client_value(&self, value: &Value) -> Value {
        match value {
            Value::Ref(Some(target)) => Value::Ref(
                self.client_id(InstanceId::from_ref(*target))
                    .map(InstanceId::to_ref),
            ),
            other => other.clone(),
        }
    }
}
