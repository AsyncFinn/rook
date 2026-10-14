//! Players and Humanoids.

use super::*;

fn players_of(w: &World, this: InstanceId) -> Vec<InstanceId> {
    w.dom
        .children(this)
        .iter()
        .copied()
        .filter(|c| w.dom.is_a(*c, "Player"))
        .collect()
}

pub(super) fn get_players(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    ret_instances(lua, with_world(lua, |w| players_of(w, this)))
}

pub(super) fn player_from_character(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
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

pub(super) fn player_by_user_id(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let user_id = a.number(0)? as i64;
    let found = with_world(lua, |w| {
        players_of(w, this)
            .into_iter()
            .find(|p| w.dom.property_by_name(*p, "UserId") == Some(Value::Int64(user_id)))
    });
    ret_instance(lua, found)
}

/// Health goes down unless the character has a ForceField (creator-docs
/// Humanoid:TakeDamage). Death handling arrives with characters.
pub(super) fn take_damage(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
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
        let pid = w.dom.property_id(this, "Health").ok_or(DomError::Stale)?;
        w.dom
            .set_property(this, pid, Value::Float32((health - amount).max(0.0) as f32))
    })?;
    Outcome::ret(lua, ())
}
