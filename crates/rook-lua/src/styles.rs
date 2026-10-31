//! StyleSheets: StyleRules set properties on what their selectors match in
//! the trees StyleLinks attach their sheets to (creator-docs "Styling").
//!
//! Each frame, before GUI lays out, the trees that changed (instances
//! added or retagged, a state changing) are styled again; a change to
//! the rules or sheets styles every linked tree again. A rule's values
//! apply only to properties the instance hasn't set itself, so what a
//! script sets wins; `$Name` values are the sheet's attribute `Name`
//! (tokens), looked for in the sheets it derives from too. Rules of
//! higher `Priority` win, then later rules. Rook doesn't support
//! `@` queries, nor nested rules.

mod selector;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use mlua::{Lua, RegistryKey, Table, Value as LuaValue};
use rook_dom::{Change, Dom, InstanceId};

use crate::ctx::with_world;
use selector::Chain;

const STYLE_CLASSES: [&str; 4] = ["StyleRule", "StyleSheet", "StyleLink", "StyleDerive"];

/// Styling state, kept in the World.
#[derive(Debug, Default)]
pub(crate) struct Styles {
    /// Journal position read up to.
    cursor: u64,
    /// Each rule's properties (`SetProperties`), a Lua table.
    pub(crate) properties: HashMap<InstanceId, RegistryKey>,
    /// Parsed selectors, by rule, with the text they came from.
    parsed: HashMap<InstanceId, (String, Rc<Vec<Chain>>)>,
    /// StyleLinks seen, checked when used.
    links: HashSet<InstanceId>,
    /// Whether the DataModel has been searched for StyleLinks yet.
    scanned: bool,
    /// Every linked tree styles again.
    all: bool,
    /// Subtrees to style again.
    pending: HashSet<InstanceId>,
    /// Properties styles set on each instance, by name.
    styled: HashMap<InstanceId, HashSet<String>>,
    /// Pseudo-instances made, by what they're under and their class.
    pseudo: HashMap<(InstanceId, String), InstanceId>,
}

impl Styles {
    /// A script set `name` on `id`: the property is its own from now on,
    /// so a rule that stops matching leaves it be rather than resetting
    /// it (the player list sets its own colors when hovered and left).
    pub(crate) fn script_wrote(&mut self, id: InstanceId, name: &str) {
        if let Some(names) = self.styled.get_mut(&id) {
            names.remove(name);
        }
    }

    /// Style `id`'s tree again at the next restyle.
    pub(crate) fn restyle_later(&mut self, id: InstanceId) {
        self.pending.insert(id);
    }

    /// Drop what styling keeps about `id`, which is being freed.
    pub(crate) fn forget(&mut self, id: InstanceId) {
        self.properties.remove(&id);
        self.parsed.remove(&id);
        self.links.remove(&id);
        self.pending.remove(&id);
        self.styled.remove(&id);
        self.pseudo
            .retain(|(under, _), made| *under != id && *made != id);
    }

    /// Rules changed: style every linked tree again.
    pub(crate) fn invalidate(&mut self) {
        self.all = true;
    }

    /// Note what the journal says must be styled again: called before each
    /// restyle, and before the journal is trimmed at the end of a frame
    /// (scripts change most things after the restyle).
    pub(crate) fn note_changes(&mut self, dom: &Dom) {
        let is_style = |id: InstanceId| STYLE_CLASSES.iter().any(|c| dom.is_a(id, c));
        for change in dom.journal().since(self.cursor) {
            match change {
                Change::Reparented { id, subtree, .. } => {
                    for &d in subtree {
                        if dom.class_name(d) == "StyleLink" {
                            self.links.insert(d);
                        }
                    }
                    if subtree.iter().any(|&d| is_style(d)) {
                        self.all = true;
                    } else {
                        self.pending.insert(*id);
                    }
                }
                Change::Property { id, .. } | Change::Attribute { id, .. } if is_style(*id) => {
                    self.all = true;
                }
                Change::Property { id, property }
                    if dom.database().property(*property).name == "GuiState" =>
                {
                    self.pending.insert(*id);
                }
                Change::TagAdded { id, .. } | Change::TagRemoved { id, .. } => {
                    self.pending.insert(*id);
                }
                _ => {}
            }
        }
        self.cursor = dom.journal().end();
    }
}

/// One rule, ready to match.
struct Rule {
    id: InstanceId,
    priority: i64,
    chains: Rc<Vec<Chain>>,
}

/// Style what changed since the last call.
pub(crate) fn restyle(lua: &Lua) -> mlua::Result<()> {
    let work = with_world(lua, |w| {
        let s = &mut w.styles;
        let dom = &w.dom;
        s.note_changes(dom);
        if !s.scanned {
            s.scanned = true;
            s.all = true;
            let root = dom.root();
            s.links.extend(
                dom.descendants(root)
                    .into_iter()
                    .filter(|&d| dom.class_name(d) == "StyleLink"),
            );
        }
        if !s.all && s.pending.is_empty() {
            return None;
        }
        s.links
            .retain(|&l| dom.get(l).is_some_and(|_| !dom.is_destroyed(l)));
        let all = std::mem::take(&mut s.all);
        let pending: Vec<InstanceId> = s.pending.drain().collect();
        let links: Vec<(InstanceId, InstanceId)> = s
            .links
            .iter()
            .filter_map(|&l| Some((dom.parent(l)?, dom.ref_property(l, "StyleSheet")?)))
            .collect();
        Some((all, pending, links))
    });
    let Some((all, pending, links)) = work else {
        return Ok(());
    };
    // Confirmed: creator-docs "UI styling" (only one StyleSheet applies to
    // a given tree). Inference: where links nest, the nearest one does, so
    // CoreGui's own link doesn't undo the chat's.
    let scopes: HashSet<InstanceId> = links.iter().map(|&(scope, _)| scope).collect();
    for (scope, sheet) in links {
        let tops: Vec<InstanceId> = if all {
            vec![scope]
        } else {
            with_world(lua, |w| {
                pending
                    .iter()
                    .copied()
                    .filter(|&p| p == scope || w.dom.is_ancestor_of(scope, p))
                    .collect()
            })
        };
        if tops.is_empty() {
            continue;
        }
        let sheets = sheet_chain(lua, sheet);
        let rules = rules_of(lua, &sheets);
        for top in tops {
            let targets = with_world(lua, |w| {
                let mut all = vec![top];
                all.extend(w.dom.descendants(top));
                all.retain(|&id| nearest_scope(&w.dom, &scopes, id) == Some(scope));
                all
            });
            for id in targets {
                style_one(lua, scope, &sheets, &rules, id)?;
            }
        }
    }
    // The journal since the last note is looked at again next time, styling's
    // own writes included: they fire Changed at once, and what scripts do
    // then (React making objects and tagging them) must be styled too. Only
    // pseudo-instances come back from styling itself, and their owners
    // style them.
    Ok(())
}

/// The nearest of `scopes` at or above `id`.
fn nearest_scope(dom: &Dom, scopes: &HashSet<InstanceId>, id: InstanceId) -> Option<InstanceId> {
    let mut at = Some(id);
    while let Some(i) = at {
        if scopes.contains(&i) {
            return Some(i);
        }
        at = dom.parent(i);
    }
    None
}

/// A sheet and those it derives from, nearest first.
fn sheet_chain(lua: &Lua, sheet: InstanceId) -> Vec<InstanceId> {
    with_world(lua, |w| {
        let mut out = vec![sheet];
        let mut i = 0;
        while i < out.len() && out.len() < 32 {
            let derived: Vec<InstanceId> = w
                .dom
                .children(out[i])
                .iter()
                .filter(|&&c| w.dom.class_name(c) == "StyleDerive")
                .filter_map(|&c| w.dom.ref_property(c, "StyleSheet"))
                .filter(|s| !out.contains(s))
                .collect();
            out.extend(derived);
            i += 1;
        }
        out
    })
}

/// The rules of `sheets`, least precedence first: derived sheets' before
/// the sheet's own, each in child order, then by priority.
fn rules_of(lua: &Lua, sheets: &[InstanceId]) -> Vec<Rule> {
    with_world(lua, |w| {
        let mut rules = Vec::new();
        for &sheet in sheets.iter().rev() {
            for &c in w.dom.children(sheet) {
                if w.dom.class_name(c) != "StyleRule" {
                    continue;
                }
                let text = w.dom.string_property(c, "Selector").unwrap_or_default();
                let chains = match w.styles.parsed.get(&c) {
                    Some((t, p)) if *t == text => p.clone(),
                    _ => {
                        let p = Rc::new(selector::parse(&text));
                        w.styles.parsed.insert(c, (text, p.clone()));
                        p
                    }
                };
                let priority = w.dom.number_property(c, "Priority").unwrap_or(0.0) as i64;
                rules.push(Rule {
                    id: c,
                    priority,
                    chains,
                });
            }
        }
        rules.sort_by_key(|r| r.priority);
        rules
    })
}

/// Which rules match `id` itself, and which style a pseudo-instance of
/// it (by class), least precedence first.
fn matching(
    dom: &Dom,
    scope: InstanceId,
    rules: &[Rule],
    id: InstanceId,
) -> (Vec<InstanceId>, Vec<(String, InstanceId)>) {
    let mut own = Vec::new();
    let mut pseudo = Vec::new();
    for rule in rules {
        for chain in rule.chains.iter() {
            if !selector::matches(dom, scope, id, chain) {
                continue;
            }
            match &chain.pseudo {
                Some(class) => pseudo.push((class.clone(), rule.id)),
                None => own.push(rule.id),
            }
            break;
        }
    }
    (own, pseudo)
}

fn style_one(
    lua: &Lua,
    scope: InstanceId,
    sheets: &[InstanceId],
    rules: &[Rule],
    id: InstanceId,
) -> mlua::Result<()> {
    // A pseudo-instance's values come from its owner's rules; styling it as
    // an object of its own would reset them.
    let pseudo_child = with_world(lua, |w| {
        w.dom.parent(id).is_some_and(|owner| {
            let key = (owner, w.dom.class_name(id).to_owned());
            w.styles.pseudo.get(&key) == Some(&id)
        })
    });
    if pseudo_child {
        return Ok(());
    }
    let (own, pseudo) = with_world(lua, |w| {
        if w.dom.get(id).is_none() || !w.dom.is_a(id, "Instance") {
            return (Vec::new(), Vec::new());
        }
        matching(&w.dom, scope, rules, id)
    });
    let values = merged(lua, sheets, &own)?;
    apply(lua, id, values, false);
    let mut wanted: HashMap<String, Vec<InstanceId>> = HashMap::new();
    for (class, rule) in pseudo {
        wanted.entry(class).or_default().push(rule);
    }
    // Pseudo-instances no rule asks for any more go.
    let stale: Vec<(String, InstanceId)> = with_world(lua, |w| {
        w.styles
            .pseudo
            .iter()
            .filter(|((owner, class), _)| *owner == id && !wanted.contains_key(class))
            .map(|((_, class), child)| (class.clone(), *child))
            .collect()
    });
    for (class, child) in stale {
        with_world(lua, |w| w.styles.pseudo.remove(&(id, class)));
        crate::removal::destroy(lua, child).ok();
    }
    for (class, rule_ids) in wanted {
        let existing = with_world(lua, |w| w.styles.pseudo.get(&(id, class.clone())).copied());
        let child = match existing {
            Some(c) => c,
            None => {
                let made = crate::api::methods::mutate(lua, |w| {
                    let c = w.dom.create_by_name(&class)?;
                    w.dom.set_name(c, class.clone())?;
                    w.dom.set_parent(c, Some(id))?;
                    Ok(c)
                });
                let Ok(c) = made else {
                    continue;
                };
                with_world(lua, |w| w.styles.pseudo.insert((id, class.clone()), c));
                c
            }
        };
        let values = merged(lua, sheets, &rule_ids)?;
        apply(lua, child, values, true);
    }
    Ok(())
}

/// The properties `rules` set, later ones winning, tokens resolved.
fn merged(
    lua: &Lua,
    sheets: &[InstanceId],
    rules: &[InstanceId],
) -> mlua::Result<Vec<(String, LuaValue)>> {
    let mut out: Vec<(String, LuaValue)> = Vec::new();
    for &rule in rules {
        let table: Option<Table> = with_world(lua, |w| {
            w.styles
                .properties
                .get(&rule)
                .map(|k| lua.registry_value::<Table>(k))
        })
        .transpose()?;
        let Some(table) = table else {
            continue;
        };
        for pair in table.pairs::<String, LuaValue>() {
            let (name, value) = pair?;
            let Some(value) = resolve(lua, sheets, value)? else {
                continue;
            };
            match out.iter_mut().find(|(n, _)| *n == name) {
                Some(slot) => slot.1 = value,
                None => out.push((name, value)),
            }
        }
    }
    Ok(out)
}

/// A `$Name` token's value from the sheets' attributes; other values as
/// they are. `None`: no sheet has the token.
fn resolve(lua: &Lua, sheets: &[InstanceId], value: LuaValue) -> mlua::Result<Option<LuaValue>> {
    let LuaValue::String(s) = &value else {
        return Ok(Some(value));
    };
    let text = s.to_string_lossy();
    let Some(token) = text.strip_prefix('$') else {
        return Ok(Some(value));
    };
    let found = with_world(lua, |w| {
        sheets.iter().find_map(|&sheet| {
            w.dom
                .get(sheet)
                .and_then(|i| i.attributes.get(token).cloned())
        })
    });
    found
        .map(|v| crate::ctx::convert::to_lua(lua, &v, None))
        .transpose()
}

/// Set the styled values on `id` where it hasn't set them itself (`own`:
/// a pseudo-instance styles made, all of whose properties are styles'),
/// and return properties styles no longer set to their defaults.
/// Values a property can't take are skipped.
fn apply(lua: &Lua, id: InstanceId, values: Vec<(String, LuaValue)>, own: bool) {
    let before = with_world(lua, |w| w.styles.styled.remove(&id).unwrap_or_default());
    let mut now = HashSet::new();
    for (name, value) in values {
        let free = with_world(lua, |w| {
            let Some(p) = w.dom.property_id(id, &name) else {
                return false;
            };
            own || before.contains(&name) || !w.dom.is_set(id, p)
        });
        if !free {
            continue;
        }
        if let Ok(Ok(())) = crate::api::instance::write_property(lua, id, &name, &value) {
            now.insert(name);
        }
    }
    for name in before.difference(&now) {
        let _ = crate::api::methods::mutate(lua, |w| match w.dom.property_id(id, name) {
            Some(p) => w.dom.reset_property(id, p),
            None => Ok(()),
        });
    }
    if !now.is_empty() {
        with_world(lua, |w| w.styles.styled.insert(id, now));
    }
}
