//! Journal entries → signal firings.

use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::PropertyId;

use super::{SignalKey, SignalKind, Signals};

/// Arguments of a firing, before conversion to Lua values.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FireArgs {
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
    /// `Humanoid.HealthChanged(health)`, for a change of this property,
    /// which only applies when it's the health.
    Health(PropertyId),
    /// `Tool.Equipped(mouse)`: the local player's Mouse on a client.
    Mouse,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Firing {
    pub key: SignalKey,
    pub args: FireArgs,
}

/// The firings one journal entry causes, limited to signals with
/// connections.
///
/// Order within a move, observed in Studio 0.741.19 and 0.743 (conformance
/// cases `signals/reparent_order` and `signals/destroy_order`):
/// `DescendantRemoving` before the move (its own journal entries, per
/// descendant in pre-order, nearest ancestor first), then `ChildRemoved`,
/// `ChildAdded`, `DescendantAdded`, and `AncestryChanged` on the subtree in
/// post-order (children before their parent); the `Parent` property change
/// follows as its own journal entry. `DescendantRemoving`/`DescendantAdded`
/// fire only on ancestors the instance actually leaves or joins; an
/// ancestor of both the old and new parent sees neither.
///
/// A Tool moved into a model holding a Humanoid (a character) fires
/// `Equipped`, and one moved out of it `Unequipped` (creator-docs Tool).
pub(crate) fn expand<H: Clone>(change: &Change, signals: &Signals<H>, dom: &Dom) -> Vec<Firing> {
    let mut out = Vec::new();
    let mut push = |instance: InstanceId, kind: SignalKind, args: FireArgs| {
        let key = SignalKey { instance, kind };
        if signals.has_connections(&key) {
            out.push(Firing { key, args });
        }
    };
    match change {
        Change::Created(_)
        | Change::Destroyed(_)
        | Change::Collected(_)
        | Change::TagAdded { .. }
        | Change::TagRemoved { .. } => {}
        Change::Removing {
            ancestors,
            descendant,
        } => {
            for &ancestor in ancestors {
                push(
                    ancestor,
                    SignalKind::Event("DescendantRemoving"),
                    FireArgs::Instance(*descendant),
                );
            }
        }
        Change::Property { id, property } => {
            push(
                *id,
                SignalKind::Event("Changed"),
                FireArgs::Property(*property),
            );
            push(*id, SignalKind::Property(*property), FireArgs::None);
            push(
                *id,
                SignalKind::Event("HealthChanged"),
                FireArgs::Health(*property),
            );
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
            if dom.is_a(*id, "Tool") {
                let held = |parent: &Option<InstanceId>| {
                    parent.is_some_and(|p| dom.children(p).iter().any(|c| dom.is_a(*c, "Humanoid")))
                };
                match (held(old_parent), held(new_parent)) {
                    (false, true) => push(*id, SignalKind::Event("Equipped"), FireArgs::Mouse),
                    (true, false) => push(*id, SignalKind::Event("Unequipped"), FireArgs::None),
                    _ => {}
                }
            }
            for &ancestor in new_ancestors.iter().filter(|a| !old_ancestors.contains(a)) {
                // One look per ancestor, not one per instance moved.
                if !signals.has_connections(&SignalKey::event(ancestor, "DescendantAdded")) {
                    continue;
                }
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
            .flat_map(|c| expand(c, &s, &dom))
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
