//! Scene queries on WorldRoot and BasePart: legacy rays and regions,
//! `Raycast`, `Blockcast`, `Spherecast`, the overlap queries and
//! `GetTouchingParts`. The rules are `rook_physics::query`'s; this file
//! reads arguments and builds results. Messages observed in Studio 0.742.
//!
//! Rays and casts hit Terrain (`rook_physics::terrain`); `ignoreWater`
//! and `RaycastParams.IgnoreWater` pass through its water. The legacy
//! rays' `terrainCellsAreCubes` changes nothing (observed: a ray hits the
//! smooth surface either way).

use glam::Vec3;
use mlua::{Lua, UserData, Value as LuaValue};
use rook_dom::InstanceId;
use rook_physics::query::{MAX_CAST_DISTANCE, MAX_CAST_RADIUS, MAX_CAST_SIDE};
use rook_physics::{CastShape, Filter, Hit, Region};
use rook_signal::SignalKey;
use rook_types::{CFrame, Vector3};

use super::methods::RawMethod;
use crate::ctx::args::{Args, borrow_clone, type_name};
use crate::ctx::convert::vector3_to_lua;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::types::enums::enum_item;
use crate::types::query::{LOverlapParams, LRaycastParams, LRaycastResult, Params};
use crate::types::{LCFrame, LRay, LRegion3};

/// FindPartsInRegion3's default `maxParts` (observed: 20 of 40 parts).
const REGION_MAX_PARTS: f64 = 20.0;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("WorldRoot", "FindPartOnRay" | "findPartOnRay") => find_part_on_ray,
        ("WorldRoot", "FindPartOnRayWithIgnoreList") => find_part_on_ray_ignore,
        ("WorldRoot", "FindPartOnRayWithWhitelist") => find_part_on_ray_whitelist,
        ("WorldRoot", "FindPartsInRegion3" | "findPartsInRegion3") => find_parts_in_region,
        ("WorldRoot", "FindPartsInRegion3WithIgnoreList") => find_parts_in_region_ignore,
        ("WorldRoot", "FindPartsInRegion3WithWhiteList") => find_parts_in_region_whitelist,
        ("WorldRoot", "IsRegion3Empty") => is_region_empty,
        ("WorldRoot", "IsRegion3EmptyWithIgnoreList") => is_region_empty_ignore,
        ("WorldRoot", "Raycast") => raycast,
        ("WorldRoot", "Blockcast") => blockcast,
        ("WorldRoot", "Spherecast") => spherecast,
        ("WorldRoot", "GetPartBoundsInBox") => part_bounds_in_box,
        ("WorldRoot", "GetPartBoundsInRadius") => part_bounds_in_radius,
        ("WorldRoot", "GetPartsInPart") => parts_in_part,
        ("BasePart", "GetTouchingParts") => touching_parts,
        ("BasePart", "GetNoCollisionConstraints") => no_collision_constraints,
        _ => return super::camera::native(owner, name),
    })
}

/// A datatype argument, with Roblox's message for the wrong type
/// (`Unable to cast double to Ray`).
fn datatype<T: UserData + Clone + 'static>(a: &Args, i: usize, name: &str) -> ApiResult<T> {
    let v = a.get(i);
    if let LuaValue::UserData(u) = v
        && let Some(t) = borrow_clone::<T>(u)
    {
        return Ok(t);
    }
    Err(cast_error(a, i, name))
}

fn cast_error(a: &Args, i: usize, name: &str) -> ScriptError {
    ScriptError(match a.get(i) {
        LuaValue::Nil => format!("Argument {} missing or nil", i + 1),
        LuaValue::Integer(_) | LuaValue::Number(_) => format!("Unable to cast double to {name}"),
        v => format!("Unable to cast {} to {name}", type_name(v)),
    })
}

fn vector(a: &Args, i: usize) -> ApiResult<Vec3> {
    match a.get(i) {
        LuaValue::Vector(v) => Ok(crate::ctx::convert::from_lua_vector(*v).0),
        _ => Err(cast_error(a, i, "Vector3")),
    }
}

fn cframe(a: &Args, i: usize) -> ApiResult<CFrame> {
    Ok(datatype::<LCFrame>(a, i, "CoordinateFrame")?.0)
}

/// An instance list argument (`Argument 2 missing or nil` when absent).
fn instance_list(a: &Args, i: usize) -> ApiResult<Vec<InstanceId>> {
    if matches!(a.get(i), LuaValue::Nil) {
        return Err(cast_error(a, i, "Array"));
    }
    a.instances(i)
}

// Legacy rays

/// `Enum.Material.Air`, which a missed legacy ray reports.
fn air() -> u32 {
    rook_reflection::database()
        .enum_by_name("Material")
        .and_then(|e| rook_reflection::enum_item_value(e.id, "Air"))
        .unwrap_or(1792)
}

/// The legacy ray's four results: the part (or nil), where the ray hit or
/// ended, the surface normal (zero on a miss) and the material (Air on a
/// miss), all observed.
fn ray_results(lua: &Lua, a: &Args, filter: Filter) -> ApiResult<Outcome> {
    let ray = datatype::<LRay>(a, 0, "Ray")?.0;
    let origin = Vector3::from(ray.origin).0;
    let dir = Vector3::from(ray.direction).0;
    let hit = with_world(lua, |w| w.physics.raycast(&mut w.dom, origin, dir, &filter));
    let (part, position, normal, material) = match hit {
        Some(h) => (
            LuaValue::UserData(super::instance::push_instance(lua, h.part)?),
            h.position,
            h.normal,
            h.material,
        ),
        None => (LuaValue::Nil, origin + dir, Vec3::ZERO, air()),
    };
    Outcome::ret(
        lua,
        (
            part,
            vector3_to_lua(Vector3(position)),
            vector3_to_lua(Vector3(normal)),
            enum_item(lua, "Material", material)?,
        ),
    )
}

/// A legacy ray's filter, with its `ignoreWater` argument at `water`.
fn legacy_ray(a: &Args, instances: Vec<InstanceId>, include: bool, water: usize) -> Filter {
    Filter {
        ignore_water: a.truthy(water),
        ..Filter::legacy_ray(instances, include)
    }
}

fn find_part_on_ray(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let ignore = a.opt_instance(1)?.into_iter().collect();
    ray_results(lua, &a, legacy_ray(&a, ignore, false, 3))
}

fn find_part_on_ray_ignore(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    datatype::<LRay>(&a, 0, "Ray")?;
    let ignore = instance_list(&a, 1)?;
    ray_results(lua, &a, legacy_ray(&a, ignore, false, 3))
}

fn find_part_on_ray_whitelist(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    datatype::<LRay>(&a, 0, "Ray")?;
    let only = instance_list(&a, 1)?;
    ray_results(lua, &a, legacy_ray(&a, only, true, 2))
}

// Regions

/// Parts whose `Size` box meets `region`.
fn in_bounds(lua: &Lua, region: Region, filter: &Filter, max: usize) -> Vec<InstanceId> {
    with_world(lua, |w| {
        w.physics.parts_in_bounds(&mut w.dom, region, filter, max)
    })
}

/// Parts in a Region3, at most `max` (0 or less: all; observed). An
/// inverted region finds nothing (observed).
fn region_parts(lua: &Lua, a: &Args, filter: Filter, max: f64) -> ApiResult<Vec<InstanceId>> {
    let region = datatype::<LRegion3>(a, 0, "Region3")?.0;
    let (min, max_corner) = (Vector3::from(region.min).0, Vector3::from(region.max).0);
    if min.cmpgt(max_corner).any() {
        return Ok(Vec::new());
    }
    let region = Region::Box {
        cframe: CFrame::from_position((min + max_corner) * 0.5),
        size: max_corner - min,
    };
    let limit = if max > 0.0 { max as usize } else { 0 };
    Ok(in_bounds(lua, region, &filter, limit))
}

fn ret_parts(lua: &Lua, parts: Vec<InstanceId>) -> ApiResult<Outcome> {
    super::instance::ret_instance_array(lua, &parts)
}

fn find_parts_in_region(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let ignore = a.opt_instance(1)?.into_iter().collect();
    let max = a.number_or(2, REGION_MAX_PARTS)?;
    ret_parts(
        lua,
        region_parts(lua, &a, Filter::legacy_region(ignore, false), max)?,
    )
}

fn find_parts_in_region_ignore(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    datatype::<LRegion3>(&a, 0, "Region3")?;
    let ignore = instance_list(&a, 1)?;
    let max = a.number_or(2, REGION_MAX_PARTS)?;
    ret_parts(
        lua,
        region_parts(lua, &a, Filter::legacy_region(ignore, false), max)?,
    )
}

fn find_parts_in_region_whitelist(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    datatype::<LRegion3>(&a, 0, "Region3")?;
    let only = instance_list(&a, 1)?;
    let max = a.number_or(2, REGION_MAX_PARTS)?;
    ret_parts(
        lua,
        region_parts(lua, &a, Filter::legacy_region(only, true), max)?,
    )
}

/// Whether a Region3 holds no part outside `ignore` and no terrain voxel
/// (observed: terrain counts even when ignored).
fn region_empty(lua: &Lua, a: &Args, ignore: Vec<InstanceId>) -> ApiResult<Outcome> {
    let parts = region_parts(lua, a, Filter::legacy_region(ignore, false), 1.0)?;
    let region = datatype::<LRegion3>(a, 0, "Region3")?.0;
    let (min, max) = (Vector3::from(region.min).0, Vector3::from(region.max).0);
    let empty =
        parts.is_empty() && !with_world(lua, |w| w.physics.terrain_voxels_in(&mut w.dom, min, max));
    Outcome::ret(lua, empty)
}

fn is_region_empty(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let ignore = a.opt_instance(1)?.into_iter().collect();
    region_empty(lua, &a, ignore)
}

fn is_region_empty_ignore(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    datatype::<LRegion3>(&a, 0, "Region3")?;
    let ignore = instance_list(&a, 1)?;
    region_empty(lua, &a, ignore)
}

// Raycast and shape casts

fn raycast_params(a: &Args, i: usize) -> ApiResult<Params> {
    match a.get(i) {
        LuaValue::Nil => Ok(Params::default()),
        _ => Ok(datatype::<LRaycastParams>(a, i, "RaycastParams")?.0),
    }
}

fn overlap_params(a: &Args, i: usize) -> ApiResult<Params> {
    match a.get(i) {
        LuaValue::Nil => Ok(Params::default()),
        _ => Ok(datatype::<LOverlapParams>(a, i, "OverlapParams")?.0),
    }
}

fn ret_hit(lua: &Lua, hit: Option<Hit>) -> ApiResult<Outcome> {
    match hit {
        Some(h) => Outcome::ret(lua, LRaycastResult(h)),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

fn raycast(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (origin, dir) = (vector(&a, 0)?, vector(&a, 1)?);
    let filter = raycast_params(&a, 2)?.filter();
    let hit = with_world(lua, |w| w.physics.raycast(&mut w.dom, origin, dir, &filter));
    ret_hit(lua, hit)
}

fn cast(lua: &Lua, shape: CastShape, cf: CFrame, dir: Vec3, params: &Params) -> ApiResult<Outcome> {
    let distance = dir.length();
    if distance > MAX_CAST_DISTANCE {
        return Err(ScriptError(format!(
            "Attempt to shapecast with distance {distance:.6}. The maximum distance is {MAX_CAST_DISTANCE}."
        )));
    }
    let filter = params.filter();
    let hit = with_world(lua, |w| {
        w.physics.shapecast(&mut w.dom, shape, cf, dir, &filter)
    });
    ret_hit(lua, hit)
}

fn blockcast(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (cf, size, dir) = (cframe(&a, 0)?, vector(&a, 1)?, vector(&a, 2)?);
    let params = raycast_params(&a, 3)?;
    let side = size.abs().max_element();
    if side > MAX_CAST_SIDE {
        return Err(ScriptError(format!(
            "Attempt to blockcast with side length {side:.6}. The maximum side length is {MAX_CAST_SIDE:.6}."
        )));
    }
    cast(lua, CastShape::Block(size.abs()), cf, dir, &params)
}

fn spherecast(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (position, radius, dir) = (vector(&a, 0)?, a.number(1)? as f32, vector(&a, 2)?);
    let params = raycast_params(&a, 3)?;
    if radius > MAX_CAST_RADIUS {
        return Err(ScriptError(format!(
            "Attempt to spherecast with radius {radius:.6}. The maximum radius is {MAX_CAST_RADIUS:.6}."
        )));
    }
    let cf = CFrame::from_position(position);
    cast(lua, CastShape::Sphere(radius), cf, dir, &params)
}

// Overlaps

fn part_bounds_in_box(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (cf, size) = (cframe(&a, 0)?, vector(&a, 1)?);
    let params = overlap_params(&a, 2)?;
    let region = Region::Box { cframe: cf, size };
    let parts = in_bounds(lua, region, &params.filter(), params.max());
    ret_parts(lua, parts)
}

fn part_bounds_in_radius(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let (position, radius) = (vector(&a, 0)?, a.number(1)? as f32);
    let params = overlap_params(&a, 2)?;
    // Observed: a negative radius finds nothing.
    if radius < 0.0 {
        return ret_parts(lua, Vec::new());
    }
    let region = Region::Sphere {
        center: position,
        radius,
    };
    let parts = in_bounds(lua, region, &params.filter(), params.max());
    ret_parts(lua, parts)
}

fn parts_in_part(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let part = a.instance(0)?;
    if !with_world(lua, |w| w.dom.is_a(part, "BasePart")) {
        return Err(ScriptError("Part parameter must be BasePart".into()));
    }
    let params = overlap_params(&a, 1)?;
    let filter = params.filter();
    let parts = with_world(lua, |w| {
        w.physics
            .parts_in_part(&mut w.dom, part, &filter, params.max())
    });
    ret_parts(lua, parts)
}

fn touching_parts(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let parts = with_world(lua, |w| {
        let listened = w
            .signals
            .has_connections(&SignalKey::event(this, "Touched"));
        w.physics.touching_parts(&mut w.dom, this, listened)
    });
    ret_parts(lua, parts)
}

/// NoCollisionConstraints whose Part0 or Part1 is this part.
fn no_collision_constraints(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let found = with_world(lua, |w| {
        let db = w.dom.database();
        let mut found: Vec<InstanceId> = w
            .dom
            .referrers(this)
            .iter()
            .filter(|(holder, p)| {
                w.dom.class_name(*holder) == "NoCollisionConstraint"
                    && matches!(db.property(*p).name, "Part0" | "Part1")
            })
            .map(|(holder, _)| *holder)
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    });
    ret_parts(lua, found)
}
