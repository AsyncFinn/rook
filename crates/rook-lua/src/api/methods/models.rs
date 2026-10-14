//! Pivots and models.

use super::*;

pub(super) fn parts_of(w: &World, model: InstanceId) -> Vec<InstanceId> {
    w.dom
        .descendants(model)
        .into_iter()
        .filter(|d| w.dom.is_a(*d, "BasePart"))
        .collect()
}

fn primary_part(w: &World, model: InstanceId) -> Option<InstanceId> {
    match w.dom.ref_property(model, "PrimaryPart") {
        Some(id) => w.dom.get(id).is_some().then_some(id),
        None => None,
    }
}

/// `PVInstance:GetPivot` for a part is `CFrame * PivotOffset`.
fn pivot_of(w: &World, id: InstanceId) -> CFrame {
    if w.dom.is_a(id, "BasePart") {
        let offset = match w.dom.property_by_name(id, "PivotOffset") {
            Some(Value::CFrame(c)) => c,
            _ => CFrame::IDENTITY,
        };
        return cframe(w, id) * offset;
    }
    match crate::api::properties::get(w, id, "WorldPivot") {
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

/// Move every part of a model rigidly by `delta` (applied in world space),
/// taking along whatever they're joined to.
fn transform_model(w: &mut World, model: InstanceId, delta: CFrame) -> Result<(), DomError> {
    let moves: Vec<_> = parts_of(w, model)
        .into_iter()
        .map(|part| (part, delta * cframe(w, part)))
        .collect();
    assembly::move_parts(&mut w.dom, moves)?;
    if let Some(Value::OptionalCFrame(Some(p))) = w.dom.property_by_name(model, "WorldPivotData") {
        let pid = w
            .dom
            .property_id(model, "WorldPivotData")
            .ok_or(DomError::Stale)?;
        w.dom
            .set_property(model, pid, Value::OptionalCFrame(Some(delta * p)))?;
    }
    Ok(())
}

pub(super) fn get_pivot(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, LCFrame(with_world(lua, |w| pivot_of(w, this))))
}

pub(super) fn pivot_to(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let target: LCFrame = a.userdata(0, "CFrame")?;
    mutate(lua, |w| {
        let current = pivot_of(w, this);
        let delta = target.0 * current.inverse();
        if w.dom.is_a(this, "BasePart") {
            let c = cframe(w, this);
            set_cframe(w, this, delta * c)
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

pub(super) fn get_primary_part_cframe(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let c = with_world(lua, |w| primary_part(w, this).map(|p| cframe(w, p)))
        .ok_or_else(|| no_primary_part("GetPrimaryPartCFrame"))?;
    Outcome::ret(lua, LCFrame(c))
}

pub(super) fn set_primary_part_cframe(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let target: LCFrame = a.userdata(0, "CFrame")?;
    let primary = with_world(lua, |w| primary_part(w, this))
        .ok_or_else(|| no_primary_part("SetPrimaryPartCFrame"))?;
    mutate(lua, |w| {
        let delta = target.0 * cframe(w, primary).inverse();
        transform_model(w, this, delta)
    })?;
    Outcome::ret(lua, ())
}

/// Deprecated; the primary part's CFrame, or the bounding box centre
/// (Approximation: Roblox's legacy calculation is undocumented).
pub(super) fn get_model_cframe(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let c = with_world(lua, |w| match primary_part(w, this) {
        Some(p) => cframe(w, p),
        None => bounding_box(w, this).0,
    });
    Outcome::ret(lua, LCFrame(c))
}

pub(super) fn translate_by(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let v = a.vector3(0)?;
    mutate(lua, |w| {
        transform_model(w, this, CFrame::from_position(v.0))
    })?;
    Outcome::ret(lua, ())
}

/// Moves the model so its primary part (or bounding box centre) is at the
/// target. Roblox also moves the model up out of obstructions; that needs
/// collision queries (Approximation until M3).
pub(super) fn model_move_to(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let target = a.vector3(0)?;
    mutate(lua, |w| {
        let from = match primary_part(w, this) {
            Some(p) => cframe(w, p).position,
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
        let c = cframe(w, part);
        let size = match w.dom.property_by_name(part, "Size") {
            Some(Value::Vector3(s)) => s.0,
            _ => glam::Vec3::ONE,
        };
        let extent = c.box_reach(size);
        min = min.min(c.position - extent);
        max = max.max(c.position + extent);
    }
    if min.x > max.x {
        return (CFrame::IDENTITY, Vector3::ZERO);
    }
    (CFrame::from_position((min + max) * 0.5), Vector3(max - min))
}

pub(super) fn get_extents_size(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let size = with_world(lua, |w| bounding_box(w, this).1);
    Outcome::ret(lua, vector3_to_lua(size))
}

pub(super) fn get_bounding_box(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let (c, s) = with_world(lua, |w| bounding_box(w, this));
    Outcome::ret(lua, (LCFrame(c), vector3_to_lua(s)))
}

pub(super) fn get_mass(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let mass = with_world(lua, |w| rook_physics::part_mass(&w.dom, this));
    Outcome::ret(lua, f64::from(mass))
}
