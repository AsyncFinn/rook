//! RaycastParams, OverlapParams and RaycastResult. Fields, defaults,
//! coercions, messages and `tostring` forms observed in Studio 0.742.

use mlua::{
    AnyUserData, Function, IntoLuaMulti, Lua, MetaMethod, MultiValue, UserData, UserDataFields,
    UserDataMethods, UserDataRef, Value as LuaValue,
};
use rook_dom::InstanceId;
use rook_physics::contacts::groups;
use rook_physics::{Filter, Hit};

use super::enums::{LEnumItem, enum_item};
use super::fmt_f32;
use crate::api::instance::{LuaInstance, push_instance};
use crate::ctx::args::number_of;
use crate::ctx::convert::vector3_to_lua;
use crate::ctx::{ApiResult, ScriptError, api_ret, with_world};

fn err(msg: impl Into<String>) -> ScriptError {
    ScriptError(msg.into())
}

/// A metatable function that runs `f` on the userdata it's called on.
/// It goes through the engine function shim, so errors reach `pcall` as
/// plain strings, as Roblox's do (mlua's own metamethods raise error
/// objects with a traceback).
fn shim<T, R>(
    lua: &Lua,
    f: fn(&Lua, &AnyUserData, MultiValue) -> ApiResult<R>,
) -> mlua::Result<Function>
where
    T: UserData + 'static,
    R: IntoLuaMulti + 'static,
{
    api_ret(lua, move |lua, args: MultiValue| {
        let mut args = args.into_iter();
        match args.next() {
            Some(LuaValue::UserData(ud)) if ud.is::<T>() => f(lua, &ud, args.collect()),
            _ => Err(err("Expected ':' not '.' calling member function")),
        }
    })
}

/// The member name an `__index` or `__newindex` was called with.
fn key(args: &MultiValue) -> String {
    match args.front() {
        Some(LuaValue::String(s)) => s.to_string_lossy(),
        Some(LuaValue::Integer(n)) => n.to_string(),
        Some(LuaValue::Number(n)) => crate::ctx::args::format_number(*n),
        _ => String::new(),
    }
}

/// `Enum.RaycastFilterType`: Exclude is 0, Include 1 (Blacklist and
/// Whitelist are their old names).
const INCLUDE: u32 = 1;

/// What RaycastParams and OverlapParams hold. Raycast params ignore
/// `max_parts` and `tolerance`; overlap params ignore `ignore_water`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Params {
    pub(crate) filter_type: u32,
    pub(crate) instances: Vec<InstanceId>,
    pub(crate) ignore_water: bool,
    pub(crate) collision_group: String,
    pub(crate) respect_can_collide: bool,
    pub(crate) brute_force: bool,
    pub(crate) max_parts: i32,
    pub(crate) tolerance: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            filter_type: 0,
            instances: Vec::new(),
            ignore_water: false,
            collision_group: groups::DEFAULT.to_owned(),
            respect_can_collide: false,
            brute_force: false,
            max_parts: 0,
            tolerance: 0.0,
        }
    }
}

impl Params {
    pub(crate) fn filter(&self) -> Filter {
        Filter {
            instances: self.instances.clone(),
            include: self.filter_type == INCLUDE,
            group: Some(self.collision_group.clone()),
            respect_can_collide: self.respect_can_collide,
            ignore_water: self.ignore_water,
            skip_invisible_ghosts: false,
        }
    }

    /// `MaxParts` as a limit; 0 or less means none.
    pub(crate) fn max(&self) -> usize {
        self.max_parts.max(0) as usize
    }
}

/// Instances in a table, nested tables flattened (observed), or one
/// instance (`AddToFilter` takes either).
fn collect(v: &LuaValue, out: &mut Vec<InstanceId>, depth: u32) -> ApiResult<()> {
    match v {
        LuaValue::UserData(u) if u.is::<LuaInstance>() => {
            out.push(u.borrow::<LuaInstance>()?.0);
            Ok(())
        }
        LuaValue::Table(t) if depth < 64 => {
            for v in t.sequence_values::<LuaValue>() {
                collect(&v?, out, depth + 1)?;
            }
            Ok(())
        }
        _ => Err(err(
            "Invalid value type; table must only contain Instances and other tables",
        )),
    }
}

/// `RaycastParams` (`OVERLAP` false) or `OverlapParams`.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct LParams<const OVERLAP: bool>(pub(crate) Params);

pub(crate) type LRaycastParams = LParams<false>;
pub(crate) type LOverlapParams = LParams<true>;

impl<const OVERLAP: bool> LParams<OVERLAP> {
    const NAME: &'static str = if OVERLAP {
        "OverlapParams"
    } else {
        "RaycastParams"
    };

    fn invalid(key: &str) -> ScriptError {
        err(format!("{key} is not a valid member of {}", Self::NAME))
    }

    fn get(&self, lua: &Lua, key: &str) -> ApiResult<LuaValue> {
        let p = &self.0;
        Ok(match key {
            "FilterType" => enum_item(lua, "RaycastFilterType", p.filter_type)?,
            "FilterDescendantsInstances" => {
                // A copy: changing the table read doesn't change the params.
                let t = lua.create_table()?;
                for (i, id) in p.instances.iter().enumerate() {
                    t.raw_set(i + 1, push_instance(lua, *id)?)?;
                }
                LuaValue::Table(t)
            }
            "CollisionGroup" => LuaValue::String(lua.create_string(&p.collision_group)?),
            "RespectCanCollide" => LuaValue::Boolean(p.respect_can_collide),
            "BruteForceAllSlow" => LuaValue::Boolean(p.brute_force),
            "IgnoreWater" if !OVERLAP => LuaValue::Boolean(p.ignore_water),
            "MaxParts" if OVERLAP => LuaValue::Integer(p.max_parts.into()),
            "Tolerance" if OVERLAP => LuaValue::Number(p.tolerance.into()),
            "AddToFilter" => {
                let name = format!("rook.{}.AddToFilter", Self::NAME);
                match lua.named_registry_value::<LuaValue>(&name)? {
                    f @ LuaValue::Function(_) => f,
                    _ => {
                        let f = shim::<Self, ()>(lua, Self::add_to_filter)?;
                        lua.set_named_registry_value(&name, &f)?;
                        LuaValue::Function(f)
                    }
                }
            }
            _ => return Err(Self::invalid(key)),
        })
    }

    fn add_to_filter(_: &Lua, ud: &AnyUserData, args: MultiValue) -> ApiResult<()> {
        let v = args.front().cloned().unwrap_or(LuaValue::Nil);
        if !matches!(v, LuaValue::Table(_) | LuaValue::UserData(_)) {
            return Err(err("Expected a table or Instance"));
        }
        let mut list = Vec::new();
        collect(&v, &mut list, 0)?;
        ud.borrow_mut::<Self>()?.0.instances.extend(list);
        Ok(())
    }

    fn set(&mut self, key: &str, v: LuaValue) -> ApiResult<()> {
        let p = &mut self.0;
        let truthy = !matches!(v, LuaValue::Nil | LuaValue::Boolean(false));
        match key {
            "FilterType" => {
                p.filter_type = match &v {
                    LuaValue::UserData(u) => u
                        .borrow::<LEnumItem>()
                        .ok()
                        .filter(|i| {
                            rook_reflection::database().enum_info(i.enum_id).name
                                == "RaycastFilterType"
                        })
                        .map(|i| i.value),
                    _ => None,
                }
                .ok_or_else(|| err("Invalid value for enum 'RaycastFilterType'"))?;
            }
            "FilterDescendantsInstances" => {
                if !matches!(v, LuaValue::Table(_)) {
                    return Err(err("Invalid value type; expected a table"));
                }
                let mut list = Vec::new();
                collect(&v, &mut list, 0)?;
                p.instances = list;
            }
            "CollisionGroup" => {
                p.collision_group = match &v {
                    LuaValue::String(s) => s.to_string_lossy(),
                    LuaValue::Integer(_) | LuaValue::Number(_) => {
                        crate::ctx::args::format_number(number_of(&v).unwrap_or(0.0))
                    }
                    _ => return Err(err("Invalid value type; expected a string")),
                };
            }
            "RespectCanCollide" => p.respect_can_collide = truthy,
            "BruteForceAllSlow" => p.brute_force = truthy,
            "IgnoreWater" if !OVERLAP => p.ignore_water = truthy,
            "MaxParts" if OVERLAP => {
                p.max_parts = number_of(&v).unwrap_or(0.0) as i32;
            }
            "Tolerance" if OVERLAP => {
                let t = number_of(&v).unwrap_or(0.0) as f32;
                if !(0.0..=0.05).contains(&t) {
                    return Err(err(format!(
                        "Tolerance outside bounds: Expected 0 - 0.050000, got {t:.6}"
                    )));
                }
                p.tolerance = t;
            }
            _ => return Err(Self::invalid(key)),
        }
        Ok(())
    }

    fn describe(&self, lua: &Lua) -> String {
        let p = &self.0;
        let list = match p.instances.as_slice() {
            [] => String::new(),
            [one] => with_world(lua, |w| w.dom.name(*one).to_owned()),
            many => format!("{} instances...", many.len()),
        };
        let common = format!(
            "BruteForceAllSlow={}, RespectCanCollide={}, CollisionGroup={}, FilterDescendantsInstances={{{list}}}",
            p.brute_force, p.respect_can_collide, p.collision_group
        );
        if OVERLAP {
            format!(
                "OverlapParams{{MaxParts={}, Tolerance={}, {common}}}",
                p.max_parts,
                fmt_f32(p.tolerance)
            )
        } else {
            format!("RaycastParams{{IgnoreWater={}, {common}}}", p.ignore_water)
        }
    }
}

impl<const OVERLAP: bool> UserData for LParams<OVERLAP> {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", Self::NAME);
        f.add_meta_field_with("__index", |lua| {
            shim::<Self, LuaValue>(lua, |lua, ud, args| {
                ud.borrow::<Self>()?.get(lua, &key(&args))
            })
        });
        f.add_meta_field_with("__newindex", |lua| {
            shim::<Self, ()>(lua, |_, ud, args| {
                let v = args.get(1).cloned().unwrap_or(LuaValue::Nil);
                ud.borrow_mut::<Self>()?.set(&key(&args), v)
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Type, |_, _, ()| Ok(Self::NAME));
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<Self>| Ok(t.0 == o.0));
        m.add_meta_method(MetaMethod::ToString, |lua, t, ()| Ok(t.describe(lua)));
    }
}

/// `RaycastResult`, from `Raycast`, `Blockcast` and `Spherecast`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LRaycastResult(pub(crate) Hit);

const RESULT_MEMBERS: [&str; 5] = ["Instance", "Position", "Normal", "Distance", "Material"];

fn material_name(value: u32) -> &'static str {
    let db = rook_reflection::database();
    db.enum_by_name("Material")
        .and_then(|e| e.items.iter().find(|(_, v)| *v == value))
        .map_or("", |(n, _)| n)
}

impl LRaycastResult {
    fn get(&self, lua: &Lua, key: &str) -> ApiResult<LuaValue> {
        let h = self.0;
        Ok(match key {
            "Instance" => LuaValue::UserData(push_instance(lua, h.part)?),
            "Position" => vector3_to_lua(rook_types::Vector3(h.position)),
            "Normal" => vector3_to_lua(rook_types::Vector3(h.normal)),
            "Distance" => LuaValue::Number(h.distance.into()),
            "Material" => enum_item(lua, "Material", h.material)?,
            _ => {
                return Err(err(format!(
                    "'{key}' is not a valid member of RaycastResult"
                )));
            }
        })
    }
}

impl UserData for LRaycastResult {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "RaycastResult");
        f.add_meta_field_with("__index", |lua| {
            shim::<Self, LuaValue>(lua, |lua, ud, args| {
                ud.borrow::<Self>()?.get(lua, &key(&args))
            })
        });
        f.add_meta_field_with("__newindex", |lua| {
            shim::<Self, ()>(lua, |_, _, args| {
                let key = key(&args);
                Err(err(if RESULT_MEMBERS.contains(&key.as_str()) {
                    format!("{key} member of RaycastResult is read-only and cannot be assigned to")
                } else {
                    format!("'{key}' is not a valid member of RaycastResult")
                }))
            })
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::ToString, |lua, t, ()| {
            let h = t.0;
            let name = with_world(lua, |w| w.dom.name(h.part).to_owned());
            let v = |v: glam::Vec3| format!("{}, {}, {}", fmt_f32(v.x), fmt_f32(v.y), fmt_f32(v.z));
            Ok(format!(
                "RaycastResult{{{name} @ {}; normal = {}; material = {}}}",
                v(h.position),
                v(h.normal),
                material_name(h.material)
            ))
        });
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // Observed: `new` ignores any arguments.
    let raycast = lua.create_table()?;
    raycast.set(
        "new",
        lua.create_function(|_, _: MultiValue| Ok(LRaycastParams::default()))?,
    )?;
    g.set("RaycastParams", raycast)?;
    let overlap = lua.create_table()?;
    overlap.set(
        "new",
        lua.create_function(|_, _: MultiValue| Ok(LOverlapParams::default()))?,
    )?;
    g.set("OverlapParams", overlap)?;
    Ok(())
}
