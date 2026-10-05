//! Journal entries → signal firings.

use rook_dom::{Change, InstanceId};
use rook_reflection::PropertyId;

use crate::{SignalKey, SignalKind, Signals};

/// Arguments of a firing, before conversion to Lua values.
#[derive(Debug, Clone, PartialEq)]
pub enum FireArgs {
    None,
    Instance(InstanceId),
    /// `AncestryChanged(child, parent)`.
    Ancestry {
        child: InstanceId,
        parent: Option<InstanceId>,
    },
    /// `Changed(property)`. ValueBase objects pass the new value instead;
    /// the Lua layer handles that.
    Property(PropertyId),
    /// `AttributeChanged(name)`.
    Attribute(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Firing {
    pub key: SignalKey,
    pub args: FireArgs,
}

/// The firings one journal entry causes, limited to signals with
/// connections.
///
/// Order within a move, observed in Studio 0.741.19 (conformance case
/// `signals/reparent_order`): `ChildRemoved`, `ChildAdded`, then
/// `AncestryChanged` on the subtree in post-order (children before their
/// parent); the `Parent` property change follows as its own journal entry.
/// `DescendantRemoving`/`DescendantAdded` fire only on ancestors the
/// instance actually leaves or joins; an ancestor of both the old and new
/// parent sees neither (Confirmed). Their position in the order relative
/// to the child events is Inference.
pub fn expand<H: Clone>(change: &Change, signals: &Signals<H>) -> Vec<Firing> {
    let mut out = Vec::new();
    let mut push = |instance: InstanceId, kind: SignalKind, args: FireArgs| {
        let key = SignalKey { instance, kind };
        if signals.has_connections(&key) {
            out.push(Firing { key, args });
        }
    };
    match change {
        Change::Created(_) | Change::TagAdded { .. } | Change::TagRemoved { .. } => {}
        Change::Property { id, property } => {
            push(
                *id,
                SignalKind::Event("Changed"),
                FireArgs::Property(*property),
            );
            push(*id, SignalKind::Property(*property), FireArgs::None);
        }
        Change::Attribute { id, name } => {
            push(
                *id,
                SignalKind::Event("AttributeChanged"),
                FireArgs::Attribute(name.clone()),
            );
            push(*id, SignalKind::Attribute(name.clone()), FireArgs::None);
        }
        Change::Destroying(id) => {
            push(*id, SignalKind::Event("Destroying"), FireArgs::None);
        }
        Change::Reparented {
            id,
            old_parent,
            new_parent,
            old_ancestors,
            new_ancestors,
            subtree,
            subtree_post_order,
        } => {
            for &ancestor in old_ancestors.iter().filter(|a| !new_ancestors.contains(a)) {
                for &d in subtree {
                    push(
                        ancestor,
                        SignalKind::Event("DescendantRemoving"),
                        FireArgs::Instance(d),
                    );
                }
            }
            if let Some(p) = old_parent {
                push(
                    *p,
                    SignalKind::Event("ChildRemoved"),
                    FireArgs::Instance(*id),
                );
            }
            if let Some(p) = new_parent {
                push(*p, SignalKind::Event("ChildAdded"), FireArgs::Instance(*id));
            }
            for &ancestor in new_ancestors.iter().filter(|a| !old_ancestors.contains(a)) {
                for &d in subtree {
                    push(
                        ancestor,
                        SignalKind::Event("DescendantAdded"),
                        FireArgs::Instance(d),
                    );
                }
            }
            for &d in subtree_post_order {
                push(
                    d,
                    SignalKind::Event("AncestryChanged"),
                    FireArgs::Ancestry {
                        child: *id,
                        parent: *new_parent,
                    },
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rook_dom::Dom;

    #[test]
    fn move_expands_only_connected_signals() {
        let mut dom = Dom::new();
        let ws = dom.get_service("Workspace").unwrap();
        let model = dom.create_by_name("Model").unwrap();
        let part = dom.create_by_name("Part").unwrap();
        dom.set_parent(part, Some(model)).unwrap();

        let mut s: Signals<()> = Signals::default();
        s.connect(SignalKey::event(ws, "ChildAdded"), (), false);
        s.connect(SignalKey::event(dom.root(), "DescendantAdded"), (), false);
        s.connect(SignalKey::event(part, "AncestryChanged"), (), false);

        let start = dom.journal().end();
        dom.set_parent(model, Some(ws)).unwrap();
        let firings: Vec<Firing> = dom
            .journal()
            .since(start)
            .iter()
            .flat_map(|c| expand(c, &s))
            .collect();
        assert_eq!(
            firings.iter().map(|f| &f.args).collect::<Vec<_>>(),
            [
                &FireArgs::Instance(model),
                &FireArgs::Instance(model),
                &FireArgs::Instance(part),
                &FireArgs::Ancestry {
                    child: model,
                    parent: Some(ws)
                },
            ]
        );
    }
}
