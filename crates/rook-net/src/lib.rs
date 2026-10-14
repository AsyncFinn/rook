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

mod replicator;
mod sink;
mod source;

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

pub use replicator::Replicator;
pub use sink::Sink;
pub use source::Source;

#[cfg(test)]
mod tests;
