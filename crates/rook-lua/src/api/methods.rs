//! Native implementations of Roblox member functions.
//!
//! `native(owner, name)` maps a member (by the class that declares it in
//! the API dump) to its implementation. Members without one raise
//! "not implemented in Rook yet" and are counted. A short list of members
//! whose effect only matters to systems Rook doesn't have yet (joints,
//! network ownership, preloading) are *inert*: they do nothing, return nil
//! and are counted separately, so scripts keep running.

use mlua::{Function, Lua, MultiValue, Value as LuaValue};
use rook_dom::{DomError, InstanceId};
use rook_reflection::database;
use rook_scheduler::Queue;
use rook_signal::{SignalKey, SignalKind};
use rook_types::{CFrame, Value, Vector3};

use super::instance::push_instance;
use super::signal::LSignal;
use crate::args::Args;
use crate::convert::{to_lua, vector3_to_lua};
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::threads::{dispatch, fire};
use crate::types::LCFrame;
use crate::world::{Job, World};

pub(crate) type RawMethod = fn(&Lua, InstanceId, Args) -> ApiResult<Outcome>;

/// Members that are accepted and ignored for now (see module docs).
const INERT: &[(&str, &str)] = &[
    ("BasePart", "MakeJoints"),
    ("BasePart", "BreakJoints"),
    ("BasePart", "SetNetworkOwner"),
    ("BasePart", "SetNetworkOwnershipAuto"),
    ("Model", "MakeJoints"),
    ("Model", "BreakJoints"),
    ("ContentProvider", "Preload"),
    ("ContentProvider", "PreloadAsync"),
    // FunctionalTest reports results to Roblox's internal test harness.
    ("FunctionalTest", "Passed"),
    ("FunctionalTest", "Failed"),
    ("FunctionalTest", "Error"),
    ("FunctionalTest", "Warn"),
];

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    if INERT.contains(&(owner, name)) {
        return Some(inert);
    }
    Some(match (owner, name) {
        ("Object", "IsA") => is_a,
        ("Object", "GetPropertyChangedSignal") => property_changed_signal,
        ("Instance", "FindFirstChild") => find_first_child,
        ("Instance", "FindFirstChildOfClass") => find_first_child_of_class,
        ("Instance", "FindFirstChildWhichIsA") => find_first_child_which_is_a,
        ("Instance", "FindFirstAncestor") => find_first_ancestor,
        ("Instance", "FindFirstAncestorOfClass") => find_first_ancestor_of_class,
        ("Instance", "FindFirstAncestorWhichIsA") => find_first_ancestor_which_is_a,
        ("Instance", "FindFirstDescendant") => find_first_descendant,
        ("Instance", "GetChildren" | "children") => get_children,
        ("Instance", "GetDescendants") => get_descendants,
        ("Instance", "IsDescendantOf") => is_descendant_of,
        ("Instance", "IsAncestorOf") => is_ancestor_of,
        ("Instance", "Destroy") => destroy,
        ("Instance", "Remove") => remove,
        ("Instance", "ClearAllChildren") => clear_all_children,
        ("Instance", "Clone") => clone,
        ("Instance", "GetFullName") => get_full_name,
        ("Instance", "GetAttribute") => get_attribute,
        ("Instance", "SetAttribute") => set_attribute,
        ("Instance", "GetAttributes") => get_attributes,
        ("Instance", "GetAttributeChangedSignal") => attribute_changed_signal,
        ("Instance", "AddTag") => add_tag,
        ("Instance", "RemoveTag") => remove_tag,
        ("Instance", "HasTag") => has_tag,
        ("Instance", "GetTags") => get_tags,
        ("Instance", "GetActor") => nil,
        ("ServiceProvider", "GetService" | "service") => get_service,
        ("ServiceProvider", "FindService") => find_service,
        ("DataModel", "IsLoaded") => true_,
        ("DataModel", "BindToClose") => bind_to_close,
        ("Debris", "AddItem" | "addItem") => debris_add_item,
        ("RunService", "IsServer" | "IsRunning") => true_,
        ("RunService", "IsClient" | "IsStudio" | "IsRunMode") => false_,
        ("BindableEvent", "Fire") => bindable_fire,
        ("BindableFunction", "Invoke") => bindable_invoke,
        ("Players", "GetPlayers" | "getPlayers" | "players") => get_players,
        ("Players", "GetPlayerFromCharacter" | "playerFromCharacter") => player_from_character,
        ("Players", "GetPlayerByUserId") => player_by_user_id,
        ("Humanoid", "TakeDamage") => take_damage,
        ("PVInstance", "GetPivot") => get_pivot,
        ("PVInstance", "PivotTo") => pivot_to,
        ("Model", "GetPrimaryPartCFrame") => get_primary_part_cframe,
        ("Model", "SetPrimaryPartCFrame") => set_primary_part_cframe,
        ("Model", "GetModelCFrame") => get_model_cframe,
        ("Model", "TranslateBy") => translate_by,
        ("Model", "MoveTo" | "move" | "moveTo") => model_move_to,
        ("Model", "GetExtentsSize" | "GetModelSize") => get_extents_size,
        ("Model", "GetBoundingBox") => get_bounding_box,
        ("BasePart", "GetMass") => get_mass,
        ("Lighting", "GetMinutesAfterMidnight") => get_minutes_after_midnight,
        ("Lighting", "SetMinutesAfterMidnight") => set_minutes_after_midnight,
        ("Sound", "Play" | "Resume") => sound_play,
        ("Sound", "Stop") => sound_stop,
        ("Sound", "Pause") => sound_pause,
        _ => return super::services::native(owner, name),
    })
}

/// Methods written in Luau because they yield in ways a single Rust call
/// can't express. Each source evaluates to the method function; it gets
/// the standard library functions it uses as chunk arguments, so scripts
/// replacing globals can't break it.
pub(crate) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    match (owner, name) {
        // Confirmed: creator-docs Instance:WaitForChild (returns nil after
        // timeOut; without one, warns about a possible infinite yield after
        // 5 seconds). Warning text is Inference.
        ("Instance", "WaitForChild") => Some(
            r#"
            local std = ...
            local task, coroutine, warn, error, type = std.task, std.coroutine, std.warn, std.error, std.type
            return function(self, name, timeOut)
                if type(name) ~= "string" then
                    error("Argument 1 missing or nil", 2)
                end
                local found = self:FindFirstChild(name)
                if found then
                    return found
                end
                local thread = coroutine.running()
                local done = false
                local conn, warnTask, timeoutTask
                local function finish(value)
                    if done then return end
                    done = true
                    conn:Disconnect()
                    if warnTask then task.cancel(warnTask) end
                    if timeoutTask then task.cancel(timeoutTask) end
                    task.spawn(thread, value)
                end
                conn = self.ChildAdded:Connect(function(child)
                    if child.Name == name then
                        finish(child)
                    end
                end)
                if timeOut then
                    timeoutTask = task.delay(timeOut, finish, nil)
                else
                    warnTask = task.delay(5, function()
                        warnTask = nil
                        if not done then
                            warn(("Infinite yield possible on '%s:WaitForChild(\"%s\")'"):format(self:GetFullName(), name))
                        end
                    end)
                end
                return coroutine.yield()
            end
            "#,
        ),
        _ => super::services::luau_source(owner, name),
    }
}

// ---- helpers ------------------------------------------------------------------

/// Run a DOM mutation and fire the signals it caused.
pub(crate) fn mutate<R>(
    lua: &Lua,
    f: impl FnOnce(&mut World) -> Result<R, DomError>,
) -> ApiResult<R> {
    let start = with_world(lua, |w| w.dom.journal().end());
    let result = with_world(lua, f);
    dispatch(lua, start);
    result.map_err(ScriptError::from)
}

fn ret_instance(lua: &Lua, id: Option<InstanceId>) -> ApiResult<Outcome> {
    match id {
        Some(id) => Outcome::ret(lua, push_instance(lua, id)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

fn ret_instances(lua: &Lua, ids: Vec<InstanceId>) -> ApiResult<Outcome> {
    let t = lua.create_table_with_capacity(ids.len(), 0)?;
    for (i, id) in ids.into_iter().enumerate() {
        t.raw_set(i + 1, push_instance(lua, id)?)?;
    }
    Outcome::ret(lua, t)
}

fn inert(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, ())
}

fn nil(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, LuaValue::Nil)
}

fn true_(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, true)
}

fn false_(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, false)
}

// ---- Object / Instance ------------------------------------------------------------

fn is_a(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    Outcome::ret(lua, with_world(lua, |w| w.dom.is_a(this, &class)))
}

fn property_changed_signal(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let pid = with_world(lua, |w| {
        let class = w.dom.get(this)?.class;
        w.dom
            .database()
            .find_canonical_property(class, &name)
            .map(|p| p.id)
    })
    .ok_or_else(|| ScriptError(format!("{name} is not a valid property name.")))?;
    Outcome::ret(
        lua,
        LSignal {
            instance: this,
            kind: SignalKind::Property(pid),
        },
    )
}

fn find_first_child(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let recursive = a.truthy(1);
    ret_instance(
        lua,
        with_world(lua, |w| w.dom.find_first_child(this, &name, recursive)),
    )
}

fn find_child_by(
    lua: &Lua,
    this: InstanceId,
    recursive: bool,
    pred: impl Fn(&World, InstanceId) -> bool,
) -> ApiResult<Outcome> {
    let found = with_world(lua, |w| {
        let candidates = if recursive {
            w.dom.descendants(this)
        } else {
            w.dom.children(this).to_vec()
        };
        candidates.into_iter().find(|c| pred(w, *c))
    });
    ret_instance(lua, found)
}

fn find_first_child_of_class(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    find_child_by(lua, this, false, |w, c| w.dom.class_name(c) == class)
}

fn find_first_child_which_is_a(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    find_child_by(lua, this, a.truthy(1), |w, c| w.dom.is_a(c, &class))
}

fn find_first_descendant(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    find_child_by(lua, this, true, |w, c| w.dom.name(c) == name)
}

fn find_ancestor_by(
    lua: &Lua,
    this: InstanceId,
    pred: impl Fn(&World, InstanceId) -> bool,
) -> ApiResult<Outcome> {
    let found = with_world(lua, |w| {
        let ancestors: Vec<_> = w.dom.ancestors(this).collect();
        ancestors.into_iter().find(|a| pred(w, *a))
    });
    ret_instance(lua, found)
}

fn find_first_ancestor(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    find_ancestor_by(lua, this, |w, c| w.dom.name(c) == name)
}

fn find_first_ancestor_of_class(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    find_ancestor_by(lua, this, |w, c| w.dom.class_name(c) == class)
}

fn find_first_ancestor_which_is_a(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let class = a.string(0)?;
    find_ancestor_by(lua, this, |w, c| w.dom.is_a(c, &class))
}

fn get_children(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    ret_instances(lua, with_world(lua, |w| w.dom.children(this).to_vec()))
}

fn get_descendants(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    ret_instances(lua, with_world(lua, |w| w.dom.descendants(this)))
}

fn is_descendant_of(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let other = a.instance(0)?;
    Outcome::ret(lua, with_world(lua, |w| w.dom.is_ancestor_of(other, this)))
}

fn is_ancestor_of(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let other = a.instance(0)?;
    Outcome::ret(lua, with_world(lua, |w| w.dom.is_ancestor_of(this, other)))
}

fn destroy(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    mutate(lua, |w| w.dom.destroy(this))?;
    Outcome::ret(lua, ())
}

/// Deprecated `Remove`: Parent to nil, then the same for each descendant
/// (creator-docs Instance:Remove). Unlike Destroy nothing is locked.
fn remove(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    fn rec(w: &mut World, id: InstanceId) -> Result<(), DomError> {
        w.dom.set_parent(id, None)?;
        for child in w.dom.children(id).to_vec() {
            rec(w, child)?;
        }
        Ok(())
    }
    mutate(lua, |w| rec(w, this))?;
    Outcome::ret(lua, ())
}

fn clear_all_children(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    mutate(lua, |w| w.dom.clear_all_children(this))?;
    Outcome::ret(lua, ())
}

fn clone(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let copy = mutate(lua, |w| Ok(w.dom.clone_instance(this)))?;
    ret_instance(lua, copy)
}

fn get_full_name(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, with_world(lua, |w| w.dom.full_name(this)))
}

// ---- attributes and tags -------------------------------------------------------------

fn get_attribute(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let value = with_world(lua, |w| w.dom.attribute(this, &name).cloned());
    match value {
        Some(v) => Outcome::ret(lua, to_lua(lua, &v, None)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

/// Attribute names: up to 100 characters of letters, digits and
/// underscores, not starting with `RBX` (Confirmed: creator-docs
/// Instance:SetAttribute).
fn valid_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with("RBX")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn attribute_value(lua: &Lua, v: &LuaValue) -> ApiResult<Option<Value>> {
    use crate::convert::datatype_value;
    Ok(match v {
        LuaValue::Nil => None,
        LuaValue::Boolean(b) => Some(Value::Bool(*b)),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            Some(Value::Float64(crate::args::number_of(v).unwrap_or(0.0)))
        }
        LuaValue::String(s) => Some(Value::String(s.to_string_lossy())),
        LuaValue::Vector(v) => Some(Value::Vector3(Vector3::new(v.x(), v.y(), v.z()))),
        LuaValue::UserData(u) => {
            if let Ok(item) = u.borrow::<crate::types::enums::LEnumItem>() {
                return Ok(Some(Value::EnumItem {
                    enum_type: database().enum_info(item.enum_id).name.to_owned(),
                    value: item.value,
                }));
            }
            match datatype_value(u) {
                Some(
                    v @ (Value::UDim(_)
                    | Value::UDim2(_)
                    | Value::BrickColor(_)
                    | Value::Color3(_)
                    | Value::Vector2(_)
                    | Value::CFrame(_)
                    | Value::NumberSequence(_)
                    | Value::ColorSequence(_)
                    | Value::NumberRange(_)
                    | Value::Rect(_)
                    | Value::Font(_)),
                ) => Some(v),
                _ => {
                    return Err(ScriptError(format!(
                        "{} is not a supported attribute type",
                        crate::args::type_name(&LuaValue::UserData(u.clone()))
                    )));
                }
            }
        }
        other => {
            let _ = lua;
            return Err(ScriptError(format!(
                "{} is not a supported attribute type",
                crate::args::type_name(other)
            )));
        }
    })
}

fn set_attribute(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    if !valid_attribute_name(&name) {
        return Err(ScriptError(format!(
            "Attribute name \"{name}\" is not valid"
        )));
    }
    let value = attribute_value(lua, a.get(1))?;
    mutate(lua, |w| w.dom.set_attribute(this, &name, value))?;
    Outcome::ret(lua, ())
}

fn get_attributes(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let attrs: Vec<(String, Value)> = with_world(lua, |w| {
        w.dom
            .get(this)
            .map(|i| {
                i.attributes
                    .iter()
                    .map(|(k, v)| (k.to_owned(), v.clone()))
                    .collect()
            })
            .unwrap_or_default()
    });
    let t = lua.create_table()?;
    for (k, v) in attrs {
        t.raw_set(k, to_lua(lua, &v, None)?)?;
    }
    Outcome::ret(lua, t)
}

fn attribute_changed_signal(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    Outcome::ret(
        lua,
        LSignal {
            instance: this,
            kind: SignalKind::Attribute(name),
        },
    )
}

fn add_tag(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    mutate(lua, |w| w.dom.add_tag(this, &tag))?;
    Outcome::ret(lua, ())
}

fn remove_tag(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    mutate(lua, |w| w.dom.remove_tag(this, &tag))?;
    Outcome::ret(lua, ())
}

fn has_tag(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let tag = a.string(0)?;
    Outcome::ret(lua, with_world(lua, |w| w.dom.has_tag(this, &tag)))
}

fn get_tags(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let tags = with_world(lua, |w| {
        w.dom.get(this).map(|i| i.tags.clone()).unwrap_or_default()
    });
    Outcome::ret(lua, lua.create_sequence_from(tags)?)
}

// ---- services --------------------------------------------------------------------------

fn get_service(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let _ = this;
    let id = mutate(lua, |w| w.dom.get_service(&name))?;
    ret_instance(lua, Some(id))
}

fn find_service(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    ret_instance(lua, with_world(lua, |w| w.dom.find_service(&name)))
}

/// Stored for the engine to call at shutdown.
fn bind_to_close(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let f = a.function(0)?;
    let list: mlua::Table =
        match lua.named_registry_value::<Option<mlua::Table>>("rook.bind_to_close")? {
            Some(t) => t,
            None => {
                let t = lua.create_table()?;
                lua.set_named_registry_value("rook.bind_to_close", t.clone())?;
                t
            }
        };
    list.raw_push(f)?;
    Outcome::ret(lua, ())
}

fn debris_add_item(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let item = a.instance(0)?;
    // Default lifetime 10 seconds (creator-docs Debris:AddItem).
    let lifetime = a.number_or(1, 10.0)?;
    with_world(lua, |w| {
        w.scheduler
            .schedule(Queue::Task, lifetime, Job::Destroy(item));
    });
    Outcome::ret(lua, ())
}

// ---- bindables --------------------------------------------------------------------------

fn bindable_fire(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let args = a.into_vec();
    fire(lua, &SignalKey::event(this, "Event"), args);
    Outcome::ret(lua, ())
}

fn bindable_invoke(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let callback = get_callback(lua, this, "OnInvoke")?.ok_or_else(|| {
        // Roblox waits for a callback to be set; Rook reports it instead
        // until callback queues exist.
        ScriptError("BindableFunction:Invoke has no OnInvoke callback".into())
    })?;
    Ok(Outcome::Call(callback, MultiValue::from_vec(a.into_vec())))
}

fn callbacks_table(lua: &Lua) -> mlua::Result<mlua::Table> {
    match lua.named_registry_value::<Option<mlua::Table>>("rook.callbacks")? {
        Some(t) => Ok(t),
        None => {
            let t = crate::ctx::weak_table(lua, "k")?;
            lua.set_named_registry_value("rook.callbacks", t.clone())?;
            Ok(t)
        }
    }
}

/// Store a callback member (`OnInvoke`, `OnServerInvoke`, ...).
pub(crate) fn set_callback(
    lua: &Lua,
    id: InstanceId,
    name: &str,
    f: Option<Function>,
) -> mlua::Result<()> {
    let ud = push_instance(lua, id)?;
    let all = callbacks_table(lua)?;
    let t: mlua::Table = match all.raw_get::<Option<mlua::Table>>(ud.clone())? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            all.raw_set(ud, t.clone())?;
            t
        }
    };
    t.raw_set(name, f)
}

pub(crate) fn get_callback(
    lua: &Lua,
    id: InstanceId,
    name: &str,
) -> mlua::Result<Option<Function>> {
    let ud = push_instance(lua, id)?;
    match callbacks_table(lua)?.raw_get::<Option<mlua::Table>>(ud)? {
        Some(t) => t.raw_get(name),
        None => Ok(None),
    }
}

// ---- players ------------------------------------------------------------------------------

fn players_of(w: &World, this: InstanceId) -> Vec<InstanceId> {
    w.dom
        .children(this)
        .iter()
        .copied()
        .filter(|c| w.dom.is_a(*c, "Player"))
        .collect()
}

fn get_players(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    ret_instances(lua, with_world(lua, |w| players_of(w, this)))
}

fn player_from_character(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let character = a.opt_instance(0)?;
    let found = character.and_then(|c| {
        with_world(lua, |w| {
            players_of(w, this).into_iter().find(|p| {
                w.dom.property_by_name(*p, "Character") == Some(Value::Ref(Some(c.to_ref())))
            })
        })
    });
    ret_instance(lua, found)
}

fn player_by_user_id(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let user_id = a.number(0)? as i64;
    let found = with_world(lua, |w| {
        players_of(w, this)
            .into_iter()
            .find(|p| w.dom.property_by_name(*p, "UserId") == Some(Value::Int64(user_id)))
    });
    ret_instance(lua, found)
}

// ---- Humanoid -----------------------------------------------------------------------------

/// Health goes down unless the character has a ForceField (creator-docs
/// Humanoid:TakeDamage). Death handling arrives with characters (M4).
fn take_damage(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let amount = a.number(0)?;
    mutate(lua, |w| {
        let protected = w.dom.parent(this).is_some_and(|p| {
            w.dom
                .children(p)
                .iter()
                .any(|c| w.dom.is_a(*c, "ForceField"))
        });
        if protected {
            return Ok(());
        }
        let health = match w.dom.property_by_name(this, "Health") {
            Some(Value::Float32(h)) => f64::from(h),
            _ => 0.0,
        };
        let class = w.dom.get(this).ok_or(DomError::Stale)?.class;
        let pid = w
            .dom
            .database()
            .find_property(class, "Health")
            .expect("Humanoid.Health")
            .id;
        w.dom
            .set_property(this, pid, Value::Float32((health - amount).max(0.0) as f32))
    })?;
    Outcome::ret(lua, ())
}

// ---- pivots and models ----------------------------------------------------------------------

fn part_cframe(w: &World, id: InstanceId) -> CFrame {
    match w.dom.property_by_name(id, "CFrame") {
        Some(Value::CFrame(c)) => c,
        _ => CFrame::IDENTITY,
    }
}

fn set_part_cframe(w: &mut World, id: InstanceId, c: CFrame) -> Result<(), DomError> {
    let class = w.dom.get(id).ok_or(DomError::Stale)?.class;
    let pid = w
        .dom
        .database()
        .find_property(class, "CFrame")
        .expect("CFrame")
        .id;
    w.dom.set_property(id, pid, Value::CFrame(c))
}

fn parts_of(w: &World, model: InstanceId) -> Vec<InstanceId> {
    w.dom
        .descendants(model)
        .into_iter()
        .filter(|d| w.dom.is_a(*d, "BasePart"))
        .collect()
}

fn primary_part(w: &World, model: InstanceId) -> Option<InstanceId> {
    match w.dom.property_by_name(model, "PrimaryPart") {
        Some(Value::Ref(Some(r))) => {
            let id = InstanceId::from_ref(r);
            w.dom.get(id).is_some().then_some(id)
        }
        _ => None,
    }
}

/// `PVInstance:GetPivot` for a part is `CFrame * PivotOffset`.
fn pivot_of(w: &World, id: InstanceId) -> CFrame {
    if w.dom.is_a(id, "BasePart") {
        let offset = match w.dom.property_by_name(id, "PivotOffset") {
            Some(Value::CFrame(c)) => c,
            _ => CFrame::IDENTITY,
        };
        return part_cframe(w, id) * offset;
    }
    match super::properties::get(w, id, "WorldPivot") {
        Some(Value::CFrame(c))
            if primary_part(w, id).is_some()
                || matches!(
                    w.dom.property_by_name(id, "WorldPivotData"),
                    Some(Value::OptionalCFrame(Some(_)))
                ) =>
        {
            c
        }
        _ => bounding_box(w, id).0,
    }
}

/// Move every part of a model rigidly by `delta` (applied in world space).
fn transform_model(w: &mut World, model: InstanceId, delta: CFrame) -> Result<(), DomError> {
    for part in parts_of(w, model) {
        let c = part_cframe(w, part);
        set_part_cframe(w, part, delta * c)?;
    }
    if let Some(Value::OptionalCFrame(Some(p))) = w.dom.property_by_name(model, "WorldPivotData") {
        let class = w.dom.get(model).ok_or(DomError::Stale)?.class;
        let pid = w
            .dom
            .database()
            .find_property(class, "WorldPivotData")
            .expect("pivot")
            .id;
        w.dom
            .set_property(model, pid, Value::OptionalCFrame(Some(delta * p)))?;
    }
    Ok(())
}

fn get_pivot(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, LCFrame(with_world(lua, |w| pivot_of(w, this))))
}

fn pivot_to(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let target: LCFrame = a.userdata(0, "CFrame")?;
    mutate(lua, |w| {
        let current = pivot_of(w, this);
        let delta = target.0 * current.inverse();
        if w.dom.is_a(this, "BasePart") {
            let c = part_cframe(w, this);
            set_part_cframe(w, this, delta * c)
        } else {
            transform_model(w, this, delta)
        }
    })?;
    Outcome::ret(lua, ())
}

fn no_primary_part(method: &str) -> ScriptError {
    ScriptError(format!(
        "Model:{method}() failed because no PrimaryPart has been set, or the PrimaryPart no longer exists. Please set Model.PrimaryPart before using this."
    ))
}

fn get_primary_part_cframe(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let c = with_world(lua, |w| primary_part(w, this).map(|p| part_cframe(w, p)))
        .ok_or_else(|| no_primary_part("GetPrimaryPartCFrame"))?;
    Outcome::ret(lua, LCFrame(c))
}

fn set_primary_part_cframe(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let target: LCFrame = a.userdata(0, "CFrame")?;
    let primary = with_world(lua, |w| primary_part(w, this))
        .ok_or_else(|| no_primary_part("SetPrimaryPartCFrame"))?;
    mutate(lua, |w| {
        let delta = target.0 * part_cframe(w, primary).inverse();
        transform_model(w, this, delta)
    })?;
    Outcome::ret(lua, ())
}

/// Deprecated; the primary part's CFrame, or the bounding box centre
/// (Approximation: Roblox's legacy calculation is undocumented).
fn get_model_cframe(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let c = with_world(lua, |w| match primary_part(w, this) {
        Some(p) => part_cframe(w, p),
        None => bounding_box(w, this).0,
    });
    Outcome::ret(lua, LCFrame(c))
}

fn translate_by(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let v = a.vector3(0)?;
    mutate(lua, |w| {
        transform_model(w, this, CFrame::from_position(v.0))
    })?;
    Outcome::ret(lua, ())
}

/// Moves the model so its primary part (or bounding box centre) is at the
/// target. Roblox also moves the model up out of obstructions; that needs
/// collision queries (Approximation until M3).
fn model_move_to(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let target = a.vector3(0)?;
    mutate(lua, |w| {
        let from = match primary_part(w, this) {
            Some(p) => part_cframe(w, p).position,
            None => bounding_box(w, this).0.position,
        };
        transform_model(w, this, CFrame::from_position(target.0 - from))
    })?;
    Outcome::ret(lua, ())
}

/// World-axis-aligned bounds of a model's parts, as (centre, size).
/// Roblox aligns the box to the primary part's orientation; this is an
/// Approximation for models without one rotated off-axis.
fn bounding_box(w: &World, model: InstanceId) -> (CFrame, Vector3) {
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
    for part in parts_of(w, model) {
        let c = part_cframe(w, part);
        let size = match w.dom.property_by_name(part, "Size") {
            Some(Value::Vector3(s)) => s.0,
            _ => glam::Vec3::ONE,
        };
        let half = size * 0.5;
        let r = c.rotation;
        let extent = r.x_axis.abs() * half.x + r.y_axis.abs() * half.y + r.z_axis.abs() * half.z;
        min = min.min(c.position - extent);
        max = max.max(c.position + extent);
    }
    if min.x > max.x {
        return (CFrame::IDENTITY, Vector3::ZERO);
    }
    (CFrame::from_position((min + max) * 0.5), Vector3(max - min))
}

fn get_extents_size(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let size = with_world(lua, |w| bounding_box(w, this).1);
    Outcome::ret(lua, vector3_to_lua(size))
}

fn get_bounding_box(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let (c, s) = with_world(lua, |w| bounding_box(w, this));
    Outcome::ret(lua, (LCFrame(c), vector3_to_lua(s)))
}

/// Volume times density. Plastic's density, 0.7, is used for every
/// material until the material table lands with physics (Approximation).
fn get_mass(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let size = with_world(lua, |w| match w.dom.property_by_name(this, "Size") {
        Some(Value::Vector3(s)) => s,
        _ => Vector3::ONE,
    });
    Outcome::ret(lua, f64::from(size.x() * size.y() * size.z()) * 0.7)
}

// ---- Lighting ------------------------------------------------------------------------------------

fn get_minutes_after_midnight(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let hours = with_world(lua, |w| super::properties::lighting_hours(w, this));
    Outcome::ret(lua, hours * 60.0)
}

fn set_minutes_after_midnight(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let minutes = a.number(0)?;
    mutate(lua, |w| {
        super::properties::set_lighting_hours(w, this, minutes / 60.0)
    })?;
    Outcome::ret(lua, ())
}

// ---- Sound (state only until audio, M6) --------------------------------------------------------

fn set_bool(w: &mut World, id: InstanceId, name: &str, v: bool) -> Result<(), DomError> {
    let class = w.dom.get(id).ok_or(DomError::Stale)?.class;
    match w.dom.database().find_canonical_property(class, name) {
        Some(p) => w.dom.set_property(id, p.id, Value::Bool(v)),
        None => Ok(()),
    }
}

fn sound_play(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    mutate(lua, |w| set_bool(w, this, "Playing", true))?;
    fire(lua, &SignalKey::event(this, "Played"), vec![]);
    Outcome::ret(lua, ())
}

fn sound_stop(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    mutate(lua, |w| {
        set_bool(w, this, "Playing", false)?;
        let class = w.dom.get(this).ok_or(DomError::Stale)?.class;
        if let Some(p) = w
            .dom
            .database()
            .find_canonical_property(class, "TimePosition")
        {
            w.dom.set_property(this, p.id, Value::Float64(0.0))?;
        }
        Ok(())
    })?;
    fire(lua, &SignalKey::event(this, "Stopped"), vec![]);
    Outcome::ret(lua, ())
}

fn sound_pause(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    mutate(lua, |w| set_bool(w, this, "Playing", false))?;
    fire(lua, &SignalKey::event(this, "Paused"), vec![]);
    Outcome::ret(lua, ())
}

/// Whether a member is inert, for reports.
pub(crate) fn is_inert(owner: &str, name: &str) -> bool {
    INERT.contains(&(owner, name))
}
