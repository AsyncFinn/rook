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
//! The replicator reads the server's journal and mirrors the server's
//! *current* state of each instance a change touched, so several changes in
//! one frame collapse into one update. New subtrees are built detached and
//! parented last, so the client sees them arrive at once.

use std::collections::HashMap;

use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::{PropertyId, PropertyKind, Serialization};
use rook_types::Value;

#[derive(Debug, Default)]
pub struct Replicator {
    to_client: HashMap<InstanceId, InstanceId>,
    to_server: HashMap<InstanceId, InstanceId>,
    /// Server journal position already replicated.
    cursor: u64,
    /// The server-side Player this client is, for PlayerReplicated containers.
    player: Option<InstanceId>,
}

impl Replicator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_player(&mut self, server_player: Option<InstanceId>) {
        self.player = server_player;
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

    /// Copy the replicated part of the server tree into a fresh client
    /// DataModel. The client's journal is not touched (like loading a file).
    pub fn initial(&mut self, server: &Dom, client: &mut Dom) {
        let was = client.journal().is_enabled();
        client.journal_mut().set_enabled(false);
        self.map(server.root(), client.root());
        for &service in server.children(server.root()) {
            if !self.visible(server, service) {
                continue;
            }
            let class = server.class_name(service).to_owned();
            let cid = match client.get_service(&class) {
                Ok(id) => id,
                Err(_) => continue,
            };
            self.map(service, cid);
            copy_properties(server, service, client, cid, self);
            for &child in server.children(service) {
                if self.visible(server, child) {
                    let c = self.build(server, child, client);
                    client.attach_unchecked(c, cid);
                }
            }
        }
        // Refs pointing at instances created later in the walk.
        let pairs: Vec<_> = self.to_client.iter().map(|(s, c)| (*s, *c)).collect();
        for (s, c) in pairs {
            copy_properties(server, s, client, c, self);
        }
        client.journal_mut().set_enabled(was);
        self.cursor = server.journal().end();
    }

    /// Build a detached copy of `id`'s visible subtree on the client.
    fn build(&mut self, server: &Dom, id: InstanceId, client: &mut Dom) -> InstanceId {
        let inst = server.get(id).expect("visible instance exists");
        let cid = match &inst.unknown_class {
            Some(name) => client.create_unknown(name),
            None => client.create(inst.class),
        };
        let _ = client.set_name(cid, inst.name().to_owned());
        self.map(id, cid);
        for &child in server.children(id) {
            if self.visible(server, child) {
                let c = self.build(server, child, client);
                // Detached parent: no checks needed, no one is listening.
                let _ = client.set_parent(c, Some(cid));
            }
        }
        copy_properties(server, id, client, cid, self);
        cid
    }

    /// Apply every server change since the last call. The caller fires
    /// client signals for the client journal entries this produces.
    pub fn sync(&mut self, server: &Dom, client: &mut Dom) {
        profiling::scope!("Replicator::sync");
        let changes: Vec<Change> = server.journal().since(self.cursor).to_vec();
        self.cursor = server.journal().end();
        for change in changes {
            match change {
                Change::Reparented { id, .. } => self.place(server, client, id),
                Change::Destroying(id) => {
                    if let Some(c) = self.client_id(id) {
                        self.unmap_subtree(client, c);
                        let _ = client.destroy(c);
                    }
                }
                Change::Property { id, property } => {
                    if let Some(c) = self.client_id(id) {
                        if property == server.name_property() {
                            let _ = client.set_name(c, server.name(id).to_owned());
                        } else if property != server.parent_property() {
                            copy_property(server, id, client, c, property, self, true);
                        }
                    }
                }
                Change::Attribute { id, name } => {
                    if let Some(c) = self.client_id(id) {
                        let value = server.attribute(id, &name).cloned();
                        let _ = client.set_attribute(c, &name, value);
                    }
                }
                Change::TagAdded { id, tag } => {
                    if let Some(c) = self.client_id(id) {
                        let _ = client.add_tag(c, &tag);
                    }
                }
                Change::TagRemoved { id, tag } => {
                    if let Some(c) = self.client_id(id) {
                        let _ = client.remove_tag(c, &tag);
                    }
                }
                Change::Created(_) => {}
            }
        }
    }

    /// Make the client's copy of `id` match the server's current placement.
    fn place(&mut self, server: &Dom, client: &mut Dom, id: InstanceId) {
        let visible = self.visible(server, id);
        let existing = self.client_id(id);
        match (visible, existing) {
            (false, Some(c)) => {
                // Left the replicated tree (parented to nil or to a server-only
                // container): it disappears on the client.
                self.unmap_subtree(client, c);
                let _ = client.destroy(c);
            }
            (true, existing) => {
                let Some(parent) = server.parent(id).and_then(|p| self.client_id(p)) else {
                    return;
                };
                let c = match existing {
                    Some(c) => c,
                    None => self.build(server, id, client),
                };
                let _ = client.set_parent(c, Some(parent));
            }
            (false, None) => {}
        }
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

fn copy_properties(
    server: &Dom,
    id: InstanceId,
    client: &mut Dom,
    cid: InstanceId,
    r: &Replicator,
) {
    let Some(inst) = server.get(id) else {
        return;
    };
    let props: Vec<PropertyId> = inst.properties().keys().copied().collect();
    for p in props {
        copy_property(server, id, client, cid, p, r, false);
    }
    let attrs: Vec<(String, Value)> = inst
        .attributes
        .iter()
        .map(|(k, v)| (k.to_owned(), v.clone()))
        .collect();
    let tags = inst.tags.clone();
    if let Some(c) = client.instance_mut(cid) {
        for (k, v) in attrs {
            c.attributes.insert(k, v);
        }
        c.tags = tags;
    }
}

fn copy_property(
    server: &Dom,
    id: InstanceId,
    client: &mut Dom,
    cid: InstanceId,
    property: PropertyId,
    r: &Replicator,
    journaled: bool,
) {
    if withheld(server, id, property) {
        return;
    }
    let Some(mut value) = server.property(id, property) else {
        return;
    };
    // References follow the instance to its client copy, or become nil if
    // the client can't see it.
    if let Value::Ref(Some(target)) = value {
        value = Value::Ref(
            r.client_id(InstanceId::from_ref(target))
                .map(InstanceId::to_ref),
        );
    }
    if journaled {
        let _ = client.set_property(cid, property, value);
    } else {
        client.load_property(cid, property, value);
    }
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
