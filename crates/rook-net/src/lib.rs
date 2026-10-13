//! Replication from a server DataModel to a client DataModel.
//!
//! Rook runs Play mode as separate server and client DataModels, the way
//! modern Roblox does, connected first by this in-process replicator
//! (architecture §8). A network transport can later carry the same changes.
//!
//! What replicates (creator-docs "Client-server model", "Replication"):
//! - Instances under services that replicate. Classes marked NotReplicated
//!   in the API dump (ServerScriptService, ServerStorage, Camera,
//!   PlayerScripts, ...) and everything under them never reach clients.
//! - A player's PlayerGui reaches only that player (PlayerReplicated).
//! - Properties except those marked NotReplicated. Server `Script` source
//!   is withheld (Inference: Roblox doesn't send server code to clients);
//!   LocalScript and ModuleScript source is sent.
//! - Attributes and tags.
//!
//! Changes made on the client stay on the client (FilteringEnabled, which
//! modern Roblox always uses).
//!
//! Replication has two halves, so the server and a client can run on
//! different threads (or, later, machines). A [`Source`] reads the server's
//! journal and describes the server's *current* state of each instance a
//! change touched as self-contained [`Update`]s, so several changes in one
//! frame collapse into one update; a [`Sink`] applies them to the client,
//! which alone knows which of its instances copy which server ones. New
//! subtrees are built detached and parented last, so the client sees them
//! arrive at once. [`Replicator`] is the two halves joined directly.

use std::collections::HashMap;

use rook_dom::{Attributes, Change, Dom, InstanceId};
use rook_reflection::{ClassId, PropertyId, PropertyKind, Serialization};
use rook_types::Value;

/// One server instance and its visible subtree, as the server has it now.
/// Ids are the server's; references in property values point at server
/// instances too.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: InstanceId,
    pub class: NodeClass,
    pub name: String,
    /// Explicitly set properties a client may see.
    pub properties: Vec<(PropertyId, Value)>,
    pub attributes: Attributes,
    pub tags: Vec<String>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeClass {
    Known(ClassId),
    /// A class Rook's database doesn't have, by name.
    Unknown(String),
}

/// One change for a client, in journal order.
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    /// `tree.id` is visible: it belongs under `parent` (the server's
    /// parent). `tree` builds it if the client doesn't have it yet.
    Place {
        parent: InstanceId,
        tree: Node,
    },
    /// The instance left what this client sees (parented to nil or to a
    /// server-only container): it disappears on the client.
    Hide(InstanceId),
    Destroying(InstanceId),
    Name {
        id: InstanceId,
        name: String,
    },
    Property {
        id: InstanceId,
        property: PropertyId,
        value: Value,
    },
    Attribute {
        id: InstanceId,
        name: String,
        value: Option<Value>,
    },
    TagAdded {
        id: InstanceId,
        tag: String,
    },
    TagRemoved {
        id: InstanceId,
        tag: String,
    },
}

/// What a new client receives first: the replicated part of every service.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The DataModel itself, without children.
    pub root: Node,
    /// Each service by class name, with its visible children.
    pub services: Vec<(String, Node)>,
}

/// The server half: what one client should see.
#[derive(Debug, Default)]
pub struct Source {
    /// Server journal position already described.
    cursor: u64,
    /// The server-side Player this client is, for PlayerReplicated containers.
    player: Option<InstanceId>,
}

impl Source {
    pub fn new(player: Option<InstanceId>) -> Self {
        Self { cursor: 0, player }
    }

    /// Whether `id` and all its ancestors may be seen by this client.
    pub fn visible(&self, server: &Dom, id: InstanceId) -> bool {
        let root = server.root();
        let mut cur = Some(id);
        while let Some(c) = cur {
            if c == root {
                return true;
            }
            let Some(inst) = server.get(c) else {
                return false;
            };
            if inst.is_destroyed() {
                return false;
            }
            let flags = server.database().class(inst.class).flags;
            if flags.not_replicated {
                return false;
            }
            if flags.player_replicated && server.parent(c) != self.player {
                return false;
            }
            cur = server.parent(c);
        }
        // Detached from the DataModel.
        false
    }

    /// Everything a new client starts with. Later [`Source::updates`]
    /// describe changes after this.
    pub fn snapshot(&mut self, server: &Dom) -> Snapshot {
        let services = server
            .children(server.root())
            .iter()
            .filter(|s| self.visible(server, **s))
            .map(|&s| (server.class_name(s).to_owned(), self.node(server, s)))
            .collect();
        self.cursor = server.journal().end();
        let mut root = self.node(server, server.root());
        root.children.clear();
        Snapshot { root, services }
    }

    /// The server's changes since the last call.
    pub fn updates(&mut self, server: &Dom) -> Vec<Update> {
        profiling::scope!("Source::updates");
        let mut out = Vec::new();
        let visible = |id| self.visible(server, id);
        for change in server.journal().since(self.cursor) {
            match change {
                Change::Reparented { id, .. } => out.push(match server.parent(*id) {
                    Some(parent) if visible(*id) => Update::Place {
                        parent,
                        tree: self.node(server, *id),
                    },
                    _ => Update::Hide(*id),
                }),
                Change::Destroying(id) => out.push(Update::Destroying(*id)),
                Change::Property { id, property } if visible(*id) => {
                    if *property == server.name_property() {
                        out.push(Update::Name {
                            id: *id,
                            name: server.name(*id).to_owned(),
                        });
                    } else if *property != server.parent_property()
                        && let Some(value) = replicated_value(server, *id, *property)
                    {
                        out.push(Update::Property {
                            id: *id,
                            property: *property,
                            value,
                        });
                    }
                }
                Change::Attribute { id, name } if visible(*id) => out.push(Update::Attribute {
                    id: *id,
                    name: name.clone(),
                    value: server.attribute(*id, name).cloned(),
                }),
                Change::TagAdded { id, tag } if visible(*id) => out.push(Update::TagAdded {
                    id: *id,
                    tag: tag.clone(),
                }),
                Change::TagRemoved { id, tag } if visible(*id) => out.push(Update::TagRemoved {
                    id: *id,
                    tag: tag.clone(),
                }),
                _ => {}
            }
        }
        self.cursor = server.journal().end();
        out
    }

    /// `id` and its visible subtree as the server has it now.
    fn node(&self, server: &Dom, id: InstanceId) -> Node {
        let inst = server.get(id).expect("visible instance exists");
        let mut properties: Vec<(PropertyId, Value)> = inst
            .properties()
            .keys()
            .filter_map(|&p| Some((p, replicated_value(server, id, p)?)))
            .collect();
        properties.shrink_to_fit();
        Node {
            id,
            class: match &inst.unknown_class {
                Some(name) => NodeClass::Unknown(name.to_string()),
                None => NodeClass::Known(inst.class),
            },
            name: inst.name().to_owned(),
            properties,
            attributes: inst.attributes.clone(),
            tags: inst.tags.clone(),
            children: server
                .children(id)
                .iter()
                .filter(|c| self.visible(server, **c))
                .map(|&c| self.node(server, c))
                .collect(),
        }
    }
}

/// A property's value as a client receives it, or `None` if it's withheld.
fn replicated_value(server: &Dom, id: InstanceId, property: PropertyId) -> Option<Value> {
    if withheld(server, id, property) {
        return None;
    }
    server.property(id, property)
}

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

/// Both halves joined directly, for a server and client in one thread.
#[derive(Debug, Default)]
pub struct Replicator {
    source: Source,
    sink: Sink,
}

impl Replicator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_player(&mut self, server_player: Option<InstanceId>) {
        self.source.player = server_player;
    }

    pub fn client_id(&self, server: InstanceId) -> Option<InstanceId> {
        self.sink.client_id(server)
    }

    pub fn server_id(&self, client: InstanceId) -> Option<InstanceId> {
        self.sink.server_id(client)
    }

    /// Copy the replicated part of the server tree into a fresh client
    /// DataModel. The client's journal is not touched (like loading a file).
    pub fn initial(&mut self, server: &Dom, client: &mut Dom) {
        let snapshot = self.source.snapshot(server);
        self.sink.snapshot(&snapshot, client);
    }

    /// Apply every server change since the last call. The caller fires
    /// client signals for the client journal entries this produces.
    pub fn sync(&mut self, server: &Dom, client: &mut Dom) {
        let updates = self.source.updates(server);
        self.sink.apply(&updates, client);
    }
}

/// Server `Script` source stays on the server.
fn withheld(server: &Dom, id: InstanceId, property: PropertyId) -> bool {
    let db = server.database();
    let prop = db.property(property);
    // Properties that serialize under another name (`Size` as `size`,
    // `Color` as `Color3uint8`, `Shape` as `shape`) are marked NotReplicated
    // in the API dump because Roblox replicates them under that other name.
    // The value still reaches clients.
    let replicated_as_other = matches!(
        prop.kind,
        PropertyKind::Canonical(Serialization::SerializesAs(_))
    );
    if prop.flags.not_replicated && !replicated_as_other {
        return true;
    }
    if prop.name == "Source" && server.class_name(id) == "Script" {
        // RunContext Client scripts run on the client, so their source goes.
        return !matches!(
            server.property_by_name(id, "RunContext"),
            Some(Value::Enum(2))
        );
    }
    // Properties that only exist to carry another one in files.
    matches!(prop.kind, PropertyKind::Alias { .. })
        || matches!(
            prop.kind,
            PropertyKind::Canonical(Serialization::Migrate { .. })
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(dom: &mut Dom, name: &str, parent: InstanceId) -> InstanceId {
        let p = dom.create_by_name("Part").unwrap();
        dom.set_name(p, name.into()).unwrap();
        dom.set_parent(p, Some(parent)).unwrap();
        p
    }

    fn setup() -> (Dom, Dom, Replicator) {
        let mut server = Dom::new();
        for s in ["Workspace", "ServerStorage", "ReplicatedStorage", "Players"] {
            server.get_service(s).unwrap();
        }
        (server, Dom::new(), Replicator::new())
    }

    #[test]
    fn initial_copy_skips_server_only_containers() {
        let (mut server, mut client, mut r) = setup();
        let ws = server.find_service("Workspace").unwrap();
        let ss = server.find_service("ServerStorage").unwrap();
        let a = part(&mut server, "A", ws);
        part(&mut server, "Secret", ss);
        let v = server.create_by_name("ObjectValue").unwrap();
        server.set_parent(v, Some(ws)).unwrap();
        server
            .set_property_by_name(v, "Value", Value::Ref(Some(a.to_ref())))
            .unwrap();

        r.initial(&server, &mut client);
        let cws = client.find_service("Workspace").unwrap();
        let ca = client.find_first_child(cws, "A", false).unwrap();
        assert!(client.find_service("ServerStorage").is_none());
        let cv = client.find_first_child(cws, "ObjectValue", false).unwrap();
        assert_eq!(
            client.property_by_name(cv, "Value"),
            Some(Value::Ref(Some(ca.to_ref())))
        );
    }

    #[test]
    fn changes_follow_the_server() {
        let (mut server, mut client, mut r) = setup();
        let ws = server.find_service("Workspace").unwrap();
        let ss = server.find_service("ServerStorage").unwrap();
        let rs = server.find_service("ReplicatedStorage").unwrap();
        r.initial(&server, &mut client);
        let cws = client.find_service("Workspace").unwrap();

        // A model built in ServerStorage arrives whole when moved to Workspace.
        let model = server.create_by_name("Model").unwrap();
        server.set_parent(model, Some(ss)).unwrap();
        part(&mut server, "Inner", model);
        r.sync(&server, &mut client);
        assert!(client.find_first_child(cws, "Model", false).is_none());
        server.set_parent(model, Some(ws)).unwrap();
        r.sync(&server, &mut client);
        let cm = client.find_first_child(cws, "Model", false).unwrap();
        assert!(client.find_first_child(cm, "Inner", false).is_some());

        // Property changes and renames.
        let inner = server.find_first_child(model, "Inner", false).unwrap();
        server
            .set_property_by_name(inner, "Anchored", Value::Bool(true))
            .unwrap();
        server.set_name(model, "Renamed".into()).unwrap();
        r.sync(&server, &mut client);
        let ci = client.find_first_child(cm, "Inner", false).unwrap();
        assert_eq!(
            client.property_by_name(ci, "Anchored"),
            Some(Value::Bool(true))
        );
        assert_eq!(client.name(cm), "Renamed");

        // Moving between replicated containers keeps the same client object.
        server.set_parent(model, Some(rs)).unwrap();
        r.sync(&server, &mut client);
        let crs = client.find_service("ReplicatedStorage").unwrap();
        assert_eq!(client.parent(cm), Some(crs));

        // Leaving the replicated tree removes it on the client.
        server.set_parent(model, Some(ss)).unwrap();
        r.sync(&server, &mut client);
        assert!(client.is_destroyed(cm));
        assert_eq!(r.client_id(model), None);
    }

    #[test]
    fn properties_serialized_under_other_names_replicate() {
        let (mut server, mut client, mut r) = setup();
        let ws = server.find_service("Workspace").unwrap();
        let p = part(&mut server, "P", ws);
        let size = server.property_id(p, "Size").unwrap();
        let color = server.property_id(p, "Color").unwrap();
        server
            .set_property(
                p,
                size,
                Value::Vector3(rook_types::Vector3::new(9.0, 8.0, 7.0)),
            )
            .unwrap();
        server
            .set_property(
                p,
                color,
                Value::Color3(rook_types::Color3::new(1.0, 0.0, 0.0)),
            )
            .unwrap();
        r.initial(&server, &mut client);
        let cws = client.find_service("Workspace").unwrap();
        let cp = client.find_first_child(cws, "P", false).unwrap();
        assert_eq!(client.property(cp, size), server.property(p, size));
        assert_eq!(client.property(cp, color), server.property(p, color));
    }

    #[test]
    fn server_script_source_is_withheld() {
        let (mut server, mut client, mut r) = setup();
        let ws = server.find_service("Workspace").unwrap();
        for class in ["Script", "LocalScript"] {
            let s = server.create_by_name(class).unwrap();
            server
                .set_property_by_name(s, "Source", Value::String("print(1)".into()))
                .unwrap();
            server.set_parent(s, Some(ws)).unwrap();
        }
        r.initial(&server, &mut client);
        let cws = client.find_service("Workspace").unwrap();
        let script = client.find_first_child(cws, "Script", false).unwrap();
        let local = client.find_first_child(cws, "LocalScript", false).unwrap();
        assert_eq!(
            client.property_by_name(script, "Source"),
            Some(Value::String(String::new()))
        );
        assert_eq!(
            client.property_by_name(local, "Source"),
            Some(Value::String("print(1)".into()))
        );
    }
}
