use super::*;

/// The server half: what one client should see.
#[derive(Debug, Default)]
pub struct Source {
    /// Server journal position already described.
    cursor: u64,
    /// The server-side Player this client is, for PlayerReplicated containers.
    pub(crate) player: Option<InstanceId>,
    /// Values this client sent the server itself (its character's pose),
    /// which it isn't sent back while the server's value is still that.
    echoes: FxHashMap<(InstanceId, PropertyId), Value>,
}

impl Source {
    pub fn new(player: Option<InstanceId>) -> Self {
        Self {
            cursor: 0,
            player,
            echoes: FxHashMap::default(),
        }
    }

    /// The server took `value` for `property` from this client: a change
    /// to it that still holds `value` isn't sent back.
    pub fn echo(&mut self, id: InstanceId, property: PropertyId, value: Value) {
        self.echoes.insert((id, property), value);
    }

    fn echoed(&self, id: InstanceId, property: PropertyId, value: &Value) -> bool {
        self.echoes.get(&(id, property)) == Some(value)
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
        let mut destroyed = Vec::new();
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
                Change::Destroying(id) => {
                    destroyed.push(*id);
                    out.push(Update::Destroying(*id));
                }
                // The property first: most entries are ones no client
                // gets, and seeing whether one may costs a walk up the tree.
                Change::Property { id, property }
                    if (*property == server.name_property()
                        || rule(*property) != Rule::Withheld)
                        && visible(*id) =>
                {
                    if *property == server.name_property() {
                        out.push(Update::Name {
                            id: *id,
                            name: server.name(*id).to_owned(),
                        });
                    } else if *property != server.parent_property()
                        && let Some(value) = replicated_value(server, *id, *property)
                        && !self.echoed(*id, *property, &value)
                    {
                        out.push(Update::Property {
                            id: *id,
                            property: *property,
                            value,
                        });
                        // A moving part's velocities go with its pose, as
                        // Roblox sends them with its physics (observed: a
                        // client reads them); physics doesn't journal them.
                        let pose = Pose::get();
                        if *property == pose.cframe
                            && server.property(*id, pose.anchored) == Some(Value::Bool(false))
                        {
                            for v in pose.velocities {
                                if let Some(value) = server.property(*id, v) {
                                    out.push(Update::Property {
                                        id: *id,
                                        property: v,
                                        value,
                                    });
                                }
                            }
                        }
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
        if !destroyed.is_empty() {
            self.echoes.retain(|(id, _), _| !destroyed.contains(id));
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
            tags: inst.tags().to_vec(),
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

fn withheld(server: &Dom, id: InstanceId, property: PropertyId) -> bool {
    match rule(property) {
        Rule::Sent => false,
        Rule::Withheld => true,
        // An anchored part's velocity carries what stands on it (a
        // conveyor) on the clients that simulate their characters too
        // (observed: a client-owned character rides one). A moving part's
        // goes with its pose instead.
        Rule::Anchored => server.bool_property(id, "Anchored") != Some(true),
        // Server `Script` source stays on the server; RunContext Client
        // scripts run on the client, so their source goes.
        Rule::Source => {
            server.class_name(id) == "Script"
                && server.enum_property(id, "RunContext")
                    != Some(rook_types::RunContext::Client.value())
        }
    }
}

/// A part's pose properties, which replication sends together.
struct Pose {
    cframe: PropertyId,
    anchored: PropertyId,
    velocities: [PropertyId; 2],
}

impl Pose {
    fn get() -> &'static Self {
        static POSE: std::sync::LazyLock<Pose> = std::sync::LazyLock::new(|| {
            let db = rook_reflection::database();
            let part = db.class_by_name("BasePart").expect("BasePart class").id;
            let id = |name| db.find_property(part, name).expect(name).id;
            Pose {
                cframe: id("CFrame"),
                anchored: id("Anchored"),
                velocities: [id("AssemblyLinearVelocity"), id("AssemblyAngularVelocity")],
            }
        });
        &POSE
    }
}

/// Whether a property's changes reach clients, as far as the property
/// alone says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    Sent,
    Withheld,
    /// Sent for anchored parts only.
    Anchored,
    /// `Source`: withheld for server Scripts.
    Source,
}

/// [`Rule`]s by property, worked out once: replication looks at every
/// journal entry, for every client.
fn rule(property: PropertyId) -> Rule {
    static RULES: std::sync::LazyLock<Vec<Rule>> = std::sync::LazyLock::new(|| {
        let db = rook_reflection::database();
        (0..db.property_count())
            .map(|i| rule_of(db, db.property(PropertyId(i as u32))))
            .collect()
    });
    RULES[property.0 as usize]
}

fn rule_of(db: &rook_reflection::Database, prop: &rook_reflection::Property) -> Rule {
    // Properties that serialize under another name (`Size` as `size`,
    // `Color` as `Color3uint8`, `Shape` as `shape`) are marked NotReplicated
    // in the API dump because Roblox replicates them under that other name.
    // The value still reaches clients.
    let replicated_as_other = matches!(
        prop.kind,
        PropertyKind::Canonical(Serialization::SerializesAs(_))
    );
    if prop.flags.not_replicated && !replicated_as_other && !replicated_by_twin(db, prop) {
        return match prop.name {
            "AssemblyLinearVelocity" | "AssemblyAngularVelocity" => Rule::Anchored,
            _ => Rule::Withheld,
        };
    }
    if prop.name == "Source" {
        return Rule::Source;
    }
    // Properties that only exist to carry another one in files.
    let carrier = matches!(prop.kind, PropertyKind::Alias { .. })
        || matches!(
            prop.kind,
            PropertyKind::Canonical(Serialization::Migrate { .. })
        );
    if carrier { Rule::Withheld } else { Rule::Sent }
}

/// Whether Roblox sends a NotReplicated property to clients through a
/// hidden twin that Rook doesn't store, so Rook must send the property
/// itself. The list and how it was found are in
/// `docs/behavior/replication.md`.
fn replicated_by_twin(db: &rook_reflection::Database, prop: &rook_reflection::Property) -> bool {
    if matches!(
        (db.class(prop.owner).name, prop.name),
        ("Sound", "PlaybackSpeed")
            | ("Player", "Team")
            | ("Workspace", "CollisionGroups")
            | ("Terrain", "SmoothGrid")
            | ("TextSource", "UserId")
    ) {
        return true;
    }
    let replicated = |name: &str| {
        db.find_property(prop.owner, name)
            .is_some_and(|twin| twin.owner == prop.owner && !twin.flags.not_replicated)
    };
    ["Replicator", "Replicate", "Replicating"]
        .iter()
        .any(|suffix| replicated(&format!("{}{suffix}", prop.name)))
        || prop.name.strip_suffix("Xml").is_some_and(replicated)
}

#[cfg(test)]
mod twin_tests {
    use super::*;

    /// Exactly these saved, NotReplicated properties reach clients.
    #[test]
    fn replicated_twins() {
        let db = rook_reflection::database();
        let mut found: Vec<String> = (0..db.property_count())
            .map(|i| db.property(PropertyId(i as u32)))
            .filter(|p| p.flags.not_replicated && replicated_by_twin(db, p))
            .map(|p| format!("{}.{}", db.class(p.owner).name, p.name))
            .collect();
        found.sort();
        assert_eq!(
            found,
            [
                "BallSocketConstraint.MaxFrictionTorqueXml",
                "BasePart.CollisionGroup",
                "Humanoid.Jump",
                "Instance.Attributes",
                "MeshPart.RenderFidelity",
                "Player.AccountAge",
                "Player.MembershipType",
                "Player.Team",
                "Sound.PlaybackSpeed",
                "Sound.Playing",
                "Sound.TimePosition",
                "Terrain.SmoothGrid",
                "TextSource.UserId",
                "VideoFrame.Playing",
                "VideoFrame.TimePosition",
                "Workspace.CollisionGroups",
            ]
        );
    }
}
