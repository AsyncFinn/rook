use std::collections::HashSet;

use super::*;

/// The client half: which client instance copies which server one.
#[derive(Debug, Default)]
pub struct Sink {
    to_client: HashMap<InstanceId, InstanceId>,
    to_server: HashMap<InstanceId, InstanceId>,
    /// References to server instances the client doesn't have yet, by the
    /// client property holding them: nil until the target arrives, then
    /// set. A script may set a reference before parenting its target
    /// (`Player.Character` is set before the character enters Workspace).
    waiting: HashMap<(InstanceId, PropertyId), InstanceId>,
    /// The same, by server target.
    waiting_for: HashMap<InstanceId, HashSet<(InstanceId, PropertyId)>>,
    /// Server instances mapped during the current call, whose waiting
    /// references resolve at its end.
    arrived: Vec<InstanceId>,
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
        if self.waiting_for.contains_key(&server) {
            self.arrived.push(server);
        }
    }

    /// Store a property's value on a client instance, through the
    /// journal or not. A reference to a server instance the client
    /// doesn't have yet is stored as nil and waits for it.
    fn store(
        &mut self,
        client: &mut Dom,
        cid: InstanceId,
        property: PropertyId,
        value: &Value,
        journaled: bool,
    ) {
        let key = (cid, property);
        if let Some(old) = self.waiting.remove(&key)
            && let Some(set) = self.waiting_for.get_mut(&old)
        {
            set.remove(&key);
            if set.is_empty() {
                self.waiting_for.remove(&old);
            }
        }
        if let Value::Ref(Some(target)) = value {
            let target = InstanceId::from_ref(*target);
            if self.client_id(target).is_none() {
                self.waiting.insert(key, target);
                self.waiting_for.entry(target).or_default().insert(key);
            }
        }
        let value = self.to_client_value(value);
        if journaled {
            warn_failed(
                client.set_property(cid, property, value),
                client,
                cid,
                "setting a property",
            );
        } else {
            client.load_property(cid, property, value);
        }
    }

    /// Set the references that were waiting for instances that arrived.
    fn resolve_arrived(&mut self, client: &mut Dom, journaled: bool) {
        for server in std::mem::take(&mut self.arrived) {
            let Some(keys) = self.waiting_for.remove(&server) else {
                continue;
            };
            let Some(target) = self.client_id(server) else {
                continue;
            };
            for key @ (cid, property) in keys {
                self.waiting.remove(&key);
                let value = Value::Ref(Some(target.to_ref()));
                if journaled {
                    let result = client.set_property(cid, property, value);
                    warn_failed(result, client, cid, "setting a reference");
                } else {
                    client.load_property(cid, property, value);
                }
            }
        }
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
        self.resolve_arrived(client, false);
        client.journal_mut().set_enabled(was);
    }

    fn copy_tree(&mut self, node: &Node, client: &mut Dom) {
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
                    warn_failed(client.set_parent(c, Some(parent)), client, c, "parenting");
                }
                Update::Hide(id) | Update::Destroying(id) => {
                    if let Some(c) = self.client_id(*id) {
                        self.unmap_subtree(client, c);
                        warn_failed(client.destroy(c), client, c, "destroying");
                    }
                }
                Update::Name { id, name } => {
                    if let Some(c) = self.client_id(*id) {
                        warn_failed(client.set_name(c, name.clone()), client, c, "renaming");
                    }
                }
                Update::Property {
                    id,
                    property,
                    value,
                } => {
                    if let Some(c) = self.client_id(*id) {
                        self.store(client, c, *property, value, true);
                    }
                }
                Update::Attribute { id, name, value } => {
                    if let Some(c) = self.client_id(*id) {
                        let result = client.set_attribute(c, name, value.clone());
                        warn_failed(result, client, c, "setting an attribute");
                    }
                }
                Update::TagAdded { id, tag } => {
                    if let Some(c) = self.client_id(*id) {
                        warn_failed(client.add_tag(c, tag), client, c, "adding a tag");
                    }
                }
                Update::TagRemoved { id, tag } => {
                    if let Some(c) = self.client_id(*id) {
                        warn_failed(client.remove_tag(c, tag), client, c, "removing a tag");
                    }
                }
            }
        }
        self.resolve_arrived(client, true);
    }

    /// Build a detached copy of a server subtree on the client.
    fn build(&mut self, node: &Node, client: &mut Dom) -> InstanceId {
        let cid = match &node.class {
            NodeClass::Unknown(name) => client.create_unknown(name),
            NodeClass::Known(class) => client.create(*class),
        };
        warn_failed(
            client.set_name(cid, node.name.clone()),
            client,
            cid,
            "naming",
        );
        self.map(node.id, cid);
        for child in &node.children {
            let c = self.build(child, client);
            // Detached parent: no one is listening yet.
            warn_failed(client.set_parent(c, Some(cid)), client, c, "parenting");
        }
        self.copy_data(node, client, cid, false);
        cid
    }

    /// Store a node's properties, attributes and tags on its copy.
    fn copy_data(&mut self, node: &Node, client: &mut Dom, cid: InstanceId, journaled: bool) {
        for (property, value) in &node.properties {
            self.store(client, cid, *property, value, journaled);
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

/// Report a client write that failed: the client no longer matches the
/// server there.
fn warn_failed(result: Result<(), rook_dom::DomError>, client: &Dom, id: InstanceId, what: &str) {
    if let Err(e) = result {
        tracing::warn!("replication: {what} {} failed: {e}", client.full_name(id));
    }
}
