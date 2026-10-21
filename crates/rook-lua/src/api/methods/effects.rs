//! ParticleEmitter's, Trail's, Beam's and WireframeHandleAdornment's
//! methods: each only notes the call for the renderer
//! (`World::effect_calls`), which owns the particles, trail segments,
//! texture scrolling (docs/behavior/effects/classic.md) and wireframe
//! lines (docs/behavior/adornments.md).

use super::*;
use crate::world::EffectCall;

fn call(lua: &Lua, this: InstanceId, call: EffectCall) -> ApiResult<Outcome> {
    with_world(lua, |w| w.effect_calls.push((this, call)));
    Outcome::ret(lua, ())
}

/// `Emit(count)`, 16 when not given (creator-docs ParticleEmitter).
pub(super) fn emit(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let count = a.number_or(0, 16.0)?;
    // Fractions round down; negative counts emit nothing (Inference).
    let count = count.clamp(0.0, f64::from(u32::MAX)) as u32;
    call(lua, this, EffectCall::Emit(count))
}

pub(super) fn clear(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    call(lua, this, EffectCall::Clear)
}

pub(super) fn set_texture_offset(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let offset = a.number_or(0, 0.0)? as f32;
    call(lua, this, EffectCall::TextureOffset(offset))
}

fn lines(lua: &Lua, this: InstanceId, lines: Vec<(glam::Vec3, glam::Vec3)>) -> ApiResult<Outcome> {
    with_world(lua, |w| {
        let calls = lines
            .into_iter()
            .map(|(a, b)| (this, EffectCall::Line(a, b)));
        w.effect_calls.extend(calls);
    });
    Outcome::ret(lua, ())
}

/// The Vector3s in array argument `i`.
fn points(a: &Args, i: usize) -> ApiResult<Vec<glam::Vec3>> {
    let LuaValue::Table(list) = a.get(i) else {
        return Err(a.wrong(i, "Array"));
    };
    let mut out = Vec::new();
    for value in list.sequence_values::<LuaValue>() {
        match value? {
            LuaValue::Vector(v) => out.push(crate::ctx::convert::from_lua_vector(v).0),
            _ => return Err(a.wrong(i, "Array of Vector3")),
        }
    }
    Ok(out)
}

pub(super) fn add_line(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (from, to) = (a.vector3(0)?.0, a.vector3(1)?.0);
    lines(lua, this, vec![(from, to)])
}

/// `AddLines(points)`: a line between each pair; an odd last point is
/// left out (Inference).
pub(super) fn add_lines(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let points = points(a, 0)?;
    lines(
        lua,
        this,
        points
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[a, b]| (a, b))
            .collect(),
    )
}

/// `AddPath(points, loop)`: a line between each point and the next, and
/// with `loop` from the last back to the first.
pub(super) fn add_path(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let points = points(a, 0)?;
    let mut out: Vec<_> = points.windows(2).map(|p| (p[0], p[1])).collect();
    if a.truthy(1)
        && let (Some(&first), Some(&last)) = (points.first(), points.last())
        && points.len() > 2
    {
        out.push((last, first));
    }
    lines(lua, this, out)
}
