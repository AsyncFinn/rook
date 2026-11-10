//! Replication from a server DataModel to a client DataModel
//! (docs/design/architecture.md#engine-systems). What replicates is in
//! `docs/behavior/replication.md`.
//!
//! A [`Source`] describes the server's current state of each instance a
//! change touched and a [`Sink`] applies it to a client, so the two halves
//! can run on different threads. New subtrees are built detached and
//! parented last, so the client sees them arrive at once.

mod replicator;
mod sink;
mod source;

use rustc_hash::FxHashMap;

use rook_dom::{Attributes, Change, Dom, InstanceId};
use rook_reflection::{ClassId, PropertyId, PropertyKind, Serialization};
use rook_terrain::Grid;
use rook_terrain::grid::edit::Bounds;
use rook_types::Value;

/// Ids are the server's, and so are references in property values.
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
    /// A Terrain's voxels in `bounds` changed: `voxels` holds the chunks
    /// holding them, as the server has them now. Roblox sends an edited
    /// block of terrain too, never the whole grid
    /// (`docs/behavior/replication.md#terrain`).
    Voxels {
        id: InstanceId,
        bounds: Bounds,
        voxels: Grid,
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
