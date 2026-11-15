//! `RBXScriptSignal` bookkeeping: connections, firing order, and turning
//! DOM journal entries into signal firings.
//!
//! This module knows nothing about Lua. Handlers are an opaque `H` chosen by
//! the caller, and whether a firing runs now (Immediate) or later (Deferred)
//! is the caller's policy. See `docs/design/architecture/scripting.md#signals-and-scheduler-rook-luas-signal-and-scheduler`.

mod connections;
mod expand;

pub(crate) use connections::Signals;
pub(crate) use expand::{FireArgs, Firing, expand};

use rook_dom::InstanceId;
use rook_reflection::PropertyId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SignalKind {
    /// A named event from the class's member list (`Touched`, `ChildAdded`, ...).
    Event(&'static str),
    /// `GetPropertyChangedSignal(property)`.
    Property(PropertyId),
    /// `GetAttributeChangedSignal(name)`.
    Attribute(String),
    /// CollectionService's `GetInstanceAddedSignal(tag)` (`added`) and
    /// `GetInstanceRemovedSignal(tag)`, kept on the DataModel's root.
    Tag { tag: String, added: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignalKey {
    pub instance: InstanceId,
    pub kind: SignalKind,
}

impl SignalKey {
    pub fn event(instance: InstanceId, name: &'static str) -> Self {
        Self {
            instance,
            kind: SignalKind::Event(name),
        }
    }
}

slotmap::new_key_type! {
    pub struct ConnectionId;
}
