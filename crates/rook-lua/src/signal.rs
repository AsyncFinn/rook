//! `RBXScriptSignal` bookkeeping: connections, firing order, and turning
//! DOM journal entries into signal firings.
//!
//! This module knows nothing about Lua. Handlers are an opaque `H` chosen by
//! the caller, and whether a firing runs now (Immediate) or later (Deferred)
//! is the caller's policy. See `docs/design/architecture.md` §6.

mod expand;

pub(crate) use expand::{FireArgs, Firing, expand};

use rook_dom::InstanceId;
use rook_reflection::PropertyId;
use rustc_hash::FxHashMap;
use slotmap::SlotMap;

/// Which signal of an instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SignalKind {
    /// A named event from the class's member list (`Touched`, `ChildAdded`, ...).
    Event(&'static str),
    /// `GetPropertyChangedSignal(property)`.
    Property(PropertyId),
    /// `GetAttributeChangedSignal(name)`.
    Attribute(String),
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

#[derive(Debug)]
struct Connection<H> {
    key: SignalKey,
    handler: H,
    once: bool,
}

/// Maximum nesting of Immediate signal handlers. Firing a signal from inside
/// handlers nested this deep raises an error instead (Confirmed: creator-docs,
/// "Event behavior").
pub(crate) const MAX_REENTRANCY: u32 = 10;

/// Every connection in one DataModel.
#[derive(Debug)]
pub struct Signals<H> {
    connections: SlotMap<ConnectionId, Connection<H>>,
    /// Connections per signal, in connection order.
    by_key: FxHashMap<SignalKey, Vec<ConnectionId>>,
    /// Signals per instance with live connections, for `Destroy`.
    by_instance: FxHashMap<InstanceId, Vec<SignalKey>>,
    depth: u32,
}

impl<H> Default for Signals<H> {
    fn default() -> Self {
        Self {
            connections: SlotMap::with_key(),
            by_key: FxHashMap::default(),
            by_instance: FxHashMap::default(),
            depth: 0,
        }
    }
}

impl<H: Clone> Signals<H> {
    pub fn connect(&mut self, key: SignalKey, handler: H, once: bool) -> ConnectionId {
        let id = self.connections.insert(Connection {
            key: key.clone(),
            handler,
            once,
        });
        let list = self.by_key.entry(key.clone()).or_default();
        if list.is_empty() {
            self.by_instance.entry(key.instance).or_default().push(key);
        }
        list.push(id);
        id
    }

    pub fn is_connected(&self, id: ConnectionId) -> bool {
        self.connections.contains_key(id)
    }

    /// Returns the handler if the connection was live.
    pub fn disconnect(&mut self, id: ConnectionId) -> Option<H> {
        let conn = self.connections.remove(id)?;
        if let Some(list) = self.by_key.get_mut(&conn.key) {
            list.retain(|c| *c != id);
            if list.is_empty() {
                self.by_key.remove(&conn.key);
                if let Some(keys) = self.by_instance.get_mut(&conn.key.instance) {
                    keys.retain(|k| *k != conn.key);
                    if keys.is_empty() {
                        self.by_instance.remove(&conn.key.instance);
                    }
                }
            }
        }
        Some(conn.handler)
    }

    /// Disconnect every connection to an instance's signals (`Destroy`).
    pub fn disconnect_instance(&mut self, instance: InstanceId) -> Vec<H> {
        let mut out = Vec::new();
        for key in self.by_instance.remove(&instance).unwrap_or_default() {
            for id in self.by_key.remove(&key).unwrap_or_default() {
                if let Some(c) = self.connections.remove(id) {
                    out.push(c.handler);
                }
            }
        }
        out
    }

    pub fn has_connections(&self, key: &SignalKey) -> bool {
        self.by_key.contains_key(key)
    }

    pub fn instance_has_connections(&self, instance: InstanceId) -> bool {
        self.by_instance.contains_key(&instance)
    }

    /// The handlers a firing reaches, in the order they run. `Once`
    /// connections are disconnected before their handler is returned.
    ///
    /// Order: most recent connection first. Inference from long-standing
    /// Immediate-mode behavior; Roblox documents the order as unspecified.
    /// Pinned by the conformance suite.
    pub fn take_handlers(&mut self, key: &SignalKey) -> Vec<(ConnectionId, H)> {
        let Some(ids) = self.by_key.get(key) else {
            return Vec::new();
        };
        let ids: Vec<ConnectionId> = ids.iter().rev().copied().collect();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let conn = &self.connections[id];
            out.push((id, conn.handler.clone()));
            if conn.once {
                self.disconnect(id);
            }
        }
        out
    }

    /// Enter an Immediate handler. Fails when already [`MAX_REENTRANCY`] deep.
    pub fn enter(&mut self) -> Result<(), ReentrancyError> {
        if self.depth >= MAX_REENTRANCY {
            return Err(ReentrancyError);
        }
        self.depth += 1;
        Ok(())
    }

    pub fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn len(&self) -> usize {
        self.connections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReentrancyError;

impl std::fmt::Display for ReentrancyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Maximum event re-entrancy depth exceeded for {MAX_REENTRANCY}"
        )
    }
}

impl std::error::Error for ReentrancyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use rook_dom::Dom;

    #[test]
    fn connection_lifecycle() {
        let mut dom = Dom::new();
        let a = dom.create_by_name("Part").unwrap();
        let mut s: Signals<&str> = Signals::default();
        let key = SignalKey::event(a, "Touched");
        let first = s.connect(key.clone(), "first", false);
        s.connect(key.clone(), "once", true);
        let handlers: Vec<_> = s.take_handlers(&key).into_iter().map(|h| h.1).collect();
        assert_eq!(handlers, ["once", "first"]);
        let handlers: Vec<_> = s.take_handlers(&key).into_iter().map(|h| h.1).collect();
        assert_eq!(handlers, ["first"]);
        assert_eq!(s.disconnect(first), Some("first"));
        assert!(!s.has_connections(&key));
        assert!(!s.instance_has_connections(a));
    }

    #[test]
    fn destroy_disconnects_everything() {
        let mut dom = Dom::new();
        let a = dom.create_by_name("Part").unwrap();
        let mut s: Signals<u32> = Signals::default();
        s.connect(SignalKey::event(a, "Touched"), 1, false);
        s.connect(SignalKey::event(a, "ChildAdded"), 2, false);
        assert_eq!(s.disconnect_instance(a).len(), 2);
        assert!(s.is_empty());
    }

    #[test]
    fn reentrancy_limit() {
        let mut s: Signals<()> = Signals::default();
        for _ in 0..MAX_REENTRANCY {
            s.enter().unwrap();
        }
        assert!(s.enter().is_err());
        s.leave();
        assert!(s.enter().is_ok());
    }
}
