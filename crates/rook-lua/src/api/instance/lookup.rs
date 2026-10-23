//! Member tables: which properties, methods and events a class has, and
//! looking them up.

use super::*;

/// Read and write security of a property, from the member list.
pub(super) fn property_security(owner: &str, name: &str) -> [Security; 2] {
    members()
        .classes
        .get(owner)
        .and_then(|c| c.property_security.get(name))
        .copied()
        .unwrap_or([Security::None; 2])
}

/// Who is running code, which decides the members it reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Identity {
    /// Game scripts: `None` security only.
    Game,
    /// Roblox's own CoreScripts and the modules they require (code that
    /// came compiled), which also reach Plugin, LocalUser and RobloxScript
    /// members (creator-docs SecurityCapabilities: CoreScripts hold
    /// RobloxScript).
    Core,
}

pub(super) fn allowed(sec: Security, identity: Identity) -> bool {
    match identity {
        Identity::Game => sec == Security::None,
        Identity::Core => !matches!(
            sec,
            Security::RobloxSecurity | Security::NotAccessibleSecurity
        ),
    }
}

/// Whether a member the game identity found needs a higher one: it was
/// denied, or is a property it may not use this way (`write`).
pub(super) fn needs_core(member: Option<Member>, write: bool) -> bool {
    match member {
        Some(Member::Denied { .. }) => true,
        Some(Member::Property(p)) => !allowed(p.security[usize::from(write)], Identity::Game),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PropInfo {
    /// The canonical property (aliases resolved).
    pub(super) name: &'static str,
    pub(super) id: PropertyId,
    pub(super) enum_id: Option<EnumId>,
    pub(super) scriptability: Scriptability,
    pub(super) flags: PropertyFlags,
    pub(super) security: [Security; 2],
}

/// What a key names on a class.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Member {
    /// A property scripts can see.
    Property(PropInfo),
    Method {
        owner: &'static str,
        name: &'static str,
        preferred: Option<&'static str>,
    },
    /// An event, by its canonical name.
    Event(&'static str),
    /// A callback (`OnInvoke`): scripts can set it but not read it.
    Callback(&'static str),
    /// A method or event the game identity can't reach.
    Denied {
        verb: &'static str,
        security: Security,
    },
}

/// Every member of a class by name, resolved the way Roblox looks keys up:
/// properties, then methods and events from the class up through its
/// superclasses, then callbacks.
pub(crate) type MemberTable = FxHashMap<&'static str, Member>;

/// Deprecated spellings Roblox still answers that the API dump no longer
/// lists: `(class, alias, member)`. Observed: Roblox's own CoreScripts
/// connect to `RunService.renderStepped` and read `.name`.
const HIDDEN_ALIASES: &[(&str, &str, &str)] = &[
    ("RunService", "renderStepped", "RenderStepped"),
    ("Instance", "name", "Name"),
];

/// Build a class's member table as `identity` sees it. `known` is false
/// for instances of classes the database doesn't have, which get no
/// properties.
pub(super) fn member_table(class: ClassId, known: bool, identity: Identity) -> MemberTable {
    let allowed = |sec| allowed(sec, identity);
    let db = database();
    let mut table = MemberTable::default();
    if known {
        for &pid in &db.class(class).all_properties {
            let name = db.property(pid).name;
            // The most derived declaration of a name wins.
            let Some(p) = db.find_property(class, name) else {
                continue;
            };
            if p.scriptability == Scriptability::None {
                continue;
            }
            // Aliases read and write their canonical property, but keep
            // their own scriptability (`size` is an alias scripts can't use).
            let canonical = db.find_canonical_property(class, name).unwrap_or(p);
            let canonical = capitalized_twin(class, canonical).unwrap_or(canonical);
            table.insert(
                name,
                Member::Property(PropInfo {
                    name: canonical.name,
                    id: canonical.id,
                    enum_id: match canonical.ty {
                        PropertyType::Enum(e) => Some(e),
                        PropertyType::Value(_) => None,
                    },
                    scriptability: p.scriptability,
                    flags: p.flags,
                    security: property_security(db.class(p.owner).name, p.name),
                }),
            );
        }
    }
    let chain = || {
        db.superclasses(class)
            .filter_map(|c| Some((c.name, members().classes.get(c.name)?)))
    };
    for (owner, m) in chain() {
        for f in &m.functions {
            table
                .entry(f.name.as_str())
                .or_insert(if allowed(f.security) {
                    Member::Method {
                        owner,
                        name: f.name.as_str(),
                        preferred: f.preferred.as_deref(),
                    }
                } else {
                    Member::Denied {
                        verb: "call",
                        security: f.security,
                    }
                });
        }
        for e in &m.events {
            let member = if allowed(e.security) {
                // Deprecated lowercase spellings (`childAdded`) are the same signal.
                let lowercase = e.name.starts_with(|c: char| c.is_ascii_lowercase());
                let canonical = m
                    .events
                    .iter()
                    .find(|o| lowercase && o.name != e.name && o.name.eq_ignore_ascii_case(&e.name))
                    .map_or(e.name.as_str(), |o| o.name.as_str());
                Member::Event(canonical)
            } else {
                Member::Denied {
                    verb: "access",
                    security: e.security,
                }
            };
            table.entry(e.name.as_str()).or_insert(member);
        }
    }
    for &(owner, alias, member) in HIDDEN_ALIASES {
        if db.superclasses(class).any(|c| c.name == owner)
            && let Some(&found) = table.get(member)
        {
            table.entry(alias).or_insert(found);
        }
    }
    for (_, m) in chain() {
        for cb in m.callbacks.iter().filter(|cb| allowed(cb.security)) {
            table
                .entry(cb.name.as_str())
                .or_insert(Member::Callback(cb.name.as_str()));
        }
    }
    table
}

/// The capitalized property a deprecated lowercase spelling stands for
/// (`className`, `Mouse.hit`, `Sound.isPlaying`). The API lists both as
/// separate properties; scripts get the same value from either
/// (Inference: the API marks the lowercase one deprecated, and old
/// scripts rely on it).
fn capitalized_twin(
    class: ClassId,
    p: &'static rook_reflection::Property,
) -> Option<&'static rook_reflection::Property> {
    let db = database();
    if !p.name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return None;
    }
    let twin = db
        .class(class)
        .all_properties
        .iter()
        .map(|&id| db.property(id))
        .find(|q| {
            q.name != p.name
                && q.name.starts_with(|c: char| c.is_ascii_uppercase())
                && q.name.eq_ignore_ascii_case(p.name)
        })?;
    db.find_canonical_property(class, twin.name)
}

/// What `key` names on an instance's class for the running code, if
/// anything (`write`: for assigning it), and who that code is.
pub(super) fn find_member(
    lua: &Lua,
    id: InstanceId,
    key: &str,
    write: bool,
) -> (Option<Member>, Identity) {
    let c = ctx(lua);
    let found = find_member_in(&c, id, key, Identity::Game).map(|(_, m)| m);
    if needs_core(found, write) && crate::api::scripts::caller_is_core(lua) {
        let core = find_member_in(&c, id, key, Identity::Core).map(|(_, m)| m);
        return (core, Identity::Core);
    }
    (found, Identity::Game)
}

/// [`find_member`] for one identity, with the context already fetched
/// (fetching it costs as much as a lookup, and `inst.Key` needs it
/// several times). Also gives the member table's own (static) copy of
/// the key.
pub(super) fn find_member_in(
    c: &Ctx,
    id: InstanceId,
    key: &str,
    identity: Identity,
) -> Option<(&'static str, Member)> {
    let (class, known) = c
        .world
        .borrow()
        .dom
        .get(id)
        .map(|i| (i.class, i.unknown_class.is_none()))?;
    let found = |table: &MemberTable| table.get_key_value(key).map(|(k, m)| (*k, *m));
    if let Some(table) = c.member_tables.borrow().get(&(class, known, identity)) {
        return found(table);
    }
    let table = member_table(class, known, identity);
    let result = found(&table);
    c.member_tables
        .borrow_mut()
        .insert((class, known, identity), table);
    result
}

/// Runtime-only property values set by engine Luau code (for example
/// `Tween.TweenInfo`, whose type isn't a stored value type).
pub(crate) fn runtime_properties(lua: &Lua) -> mlua::Result<mlua::Table> {
    let c = ctx(lua);
    if let Some(t) = &*c.runtime_props.borrow() {
        return Ok(t.clone());
    }
    let t = crate::ctx::weak_table(lua, "k")?;
    *c.runtime_props.borrow_mut() = Some(t.clone());
    Ok(t)
}

pub(super) fn runtime_property(
    lua: &Lua,
    c: &Ctx,
    id: InstanceId,
    key: &str,
) -> mlua::Result<Option<LuaValue>> {
    if !c.runtime_prop_ids.borrow().contains(&id) {
        return Ok(None);
    }
    let Some(all) = c.runtime_props.borrow().clone() else {
        return Ok(None);
    };
    match all.raw_get::<Option<mlua::Table>>(push_instance(lua, id)?)? {
        Some(t) => Ok(t.raw_get::<Option<LuaValue>>(key)?),
        None => Ok(None),
    }
}

/// `inst[key]` for scripts; whether it's a method (the same function for
/// every instance of the class, so the caller may cache it); and, for a
/// plain stored property, the member id [`fast_read`] reads it by.
pub(crate) fn get_member(
    lua: &Lua,
    id: InstanceId,
    key: &str,
) -> ApiResult<(LuaValue, bool, Option<u32>)> {
    let c = ctx(lua);
    if let Some(v) = runtime_property(lua, &c, id, key)? {
        return Ok((v, false, None));
    }
    // `className` is the deprecated lowercase name.
    if matches!(key, "ClassName" | "className") {
        let name = with_world(lua, |w| w.dom.class_name(id).to_owned());
        return Ok((LuaValue::String(lua.create_string(name)?), false, None));
    }
    let mut found = find_member_in(&c, id, key, Identity::Game);
    // Members CoreScripts reach aren't cached for the class (as a method
    // or a fast read), or game scripts would reach them too.
    let identity =
        if needs_core(found.map(|(_, m)| m), false) && crate::api::scripts::caller_is_core(lua) {
            found = find_member_in(&c, id, key, Identity::Core);
            Identity::Core
        } else {
            Identity::Game
        };
    let member = found.map(|(_, m)| m);
    let method = matches!(member, Some(Member::Method { .. })) && identity == Identity::Game;
    let mut fast = None;
    let value = match member {
        Some(Member::Property(p)) => {
            if !allowed(p.security[0], identity) {
                return Err(ScriptError(format!(
                    "The current thread cannot access '{key}' (lacking capability {})",
                    p.security[0].capability()
                )));
            }
            if p.flags.write_only {
                return Err(ScriptError(format!("{key} is a write-only member")));
            }
            let (value, plain) = read(lua, &c, id, p.name, Some((p.id, p.enum_id)))?;
            if plain
                && identity == Identity::Game
                && let Some((static_key, _)) = found
            {
                fast = Some(register_fast_read(&c, id, static_key, p));
            }
            Ok(value)
        }
        Some(Member::Method {
            owner,
            name,
            preferred,
        }) => Ok(LuaValue::Function(method_function(
            lua, owner, name, preferred,
        )?)),
        Some(Member::Event(name)) => Ok(mlua::IntoLua::into_lua(
            LSignal {
                instance: id,
                kind: SignalKind::Event(name),
            },
            lua,
        )?),
        Some(Member::Denied { verb, security }) => Err(ScriptError(format!(
            "The current thread cannot {verb} '{key}' (lacking capability {})",
            security.capability()
        ))),
        Some(Member::Callback(_)) => {
            let class = with_world(lua, |w| w.dom.class_name(id).to_owned());
            Err(ScriptError(format!(
                "{key} is a callback member of {class}; you can only set the callback value, get is not available"
            )))
        }
        None => {
            let child = with_world(lua, |w| w.dom.find_first_child(id, key, false));
            match child {
                Some(child) => Ok(LuaValue::UserData(push_instance(lua, child)?)),
                // Members newer than Rook's API snapshot that Roblox's
                // CoreScripts read: nil, counted (see `calls::method_function`).
                None if crate::api::scripts::caller_is_core(lua) => {
                    let class = with_world(lua, |w| w.dom.class_name(id).to_owned());
                    match methods::core::hidden_property(&class, key) {
                        Some(v) => Ok(LuaValue::String(lua.create_string(v)?)),
                        None => {
                            with_world(lua, |w| {
                                w.count_unimplemented(format!("{class}.{key} (unknown)"));
                            });
                            Ok(LuaValue::Nil)
                        }
                    }
                }
                None => Err(not_a_member(lua, id, key)),
            }
        }
    }?;
    Ok((value, method, fast))
}

/// Remember that reading `key` on `id`'s class is a plain read of `p`,
/// and give `key` its member id. Derived properties (`properties::get`)
/// depend only on the class, so what holds for one instance holds for
/// every instance of its class.
pub(super) fn register_fast_read(c: &Ctx, id: InstanceId, key: &'static str, p: PropInfo) -> u32 {
    let member = {
        let mut ids = c.member_ids.borrow_mut();
        let next = ids.len() as u32;
        *ids.entry(key).or_insert(next)
    };
    if let Some(class) = c.world.borrow().dom.get(id).map(|i| i.class) {
        c.fast_reads
            .borrow_mut()
            .insert((class, member), (p.id, p.enum_id));
    }
    member
}

/// `inst.Key` by member id, for keys [`get_member`] found to be plain
/// stored properties of the instance's class. The second result is `None`
/// for a value, 1 when the value is an instance's key (the shim turns it
/// into the instance's userdata without a trip through Rust), and 0 when
/// the read must take the general path: another class, an unknown class,
/// or an instance with runtime-only values.
pub(super) fn fast_read(
    lua: &Lua,
    c: &Ctx,
    id: InstanceId,
    member: u32,
) -> mlua::Result<(LuaValue, Option<u8>)> {
    const SLOW: (LuaValue, Option<u8>) = (LuaValue::Nil, Some(0));
    let value = {
        let w = c.world.borrow();
        let Some(inst) = w.dom.get(id).filter(|i| i.unknown_class.is_none()) else {
            return Ok(SLOW);
        };
        let Some(&(pid, enum_id)) = c.fast_reads.borrow().get(&(inst.class, member)) else {
            return Ok(SLOW);
        };
        if c.runtime_prop_ids.borrow().contains(&id) {
            return Ok(SLOW);
        }
        // The most read property: made from the instance's own name, as
        // `property` would copy it first.
        if pid == w.dom.name_property() {
            return Ok((LuaValue::String(lua.create_string(w.dom.name(id))?), None));
        }
        (w.dom.property(id, pid), enum_id)
    };
    Ok(match value {
        (Some(Value::Ref(Some(r))), _) => (
            LuaValue::LightUserData(key_of(InstanceId::from_ref(r))),
            Some(1),
        ),
        (Some(v), enum_id) => (to_lua(lua, &v, enum_id)?, None),
        (None, _) => (LuaValue::Nil, None),
    })
}
