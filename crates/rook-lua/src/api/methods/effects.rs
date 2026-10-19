//! ParticleEmitter's, Trail's and Beam's methods: each only notes the call
//! for the renderer (`World::effect_calls`), which owns the particles,
//! trail segments and texture scrolling (docs/behavior/effects.md).

use super::*;
use crate::world::EffectCall;

fn call(lua: &Lua, this: InstanceId, call: EffectCall) -> ApiResult<Outcome> {
    with_world(lua, |w| w.effect_calls.push((this, call)));
    Outcome::ret(lua, ())
}

/// `Emit(count)`, 16 when not given (creator-docs ParticleEmitter).
pub(super) fn emit(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let count = a.number_or(0, 16.0)?;
    // Fractions round down; negative counts emit nothing (Inference).
    let count = count.clamp(0.0, f64::from(u32::MAX)) as u32;
    call(lua, this, EffectCall::Emit(count))
}

pub(super) fn clear(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    call(lua, this, EffectCall::Clear)
}

pub(super) fn set_texture_offset(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let offset = a.number_or(0, 0.0)? as f32;
    call(lua, this, EffectCall::TextureOffset(offset))
}
