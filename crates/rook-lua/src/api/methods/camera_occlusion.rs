//! What stands between a camera and what it looks at:
//! `GetLargestCutoffDistance`, which legacy camera scripts move the camera
//! in by, and `GetPartsObscuringTarget`. Both see only the camera's own
//! world: one outside Workspace finds nothing (Observed in Studio 0.747).

use glam::{Vec2, Vec3};
use mlua::{Lua, Value as LuaValue};
use rook_dom::{Dom, InstanceId};
use rook_physics::Filter;
use rustc_hash::FxHashSet;

use super::RawMethod;
use super::camera::{Lens, near_plane};
use crate::ctx::args::{Args, type_name};
use crate::ctx::convert::from_lua_vector;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};

/// Parts more transparent than this don't cut the view off (Observed:
/// 0.95 does, 0.951 doesn't).
const SEE_THROUGH: f32 = 0.95;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Camera", "GetLargestCutoffDistance") => largest_cutoff_distance,
        ("Camera", "GetPartsObscuringTarget") => parts_obscuring_target,
        _ => return None,
    })
}

fn in_workspace(dom: &Dom, camera: InstanceId) -> bool {
    dom.find_service("Workspace")
        .is_some_and(|ws| dom.is_ancestor_of(ws, camera))
}

/// An ignore list: left off, `Argument 2 missing or nil`; anything but
/// a table, explicit nil too, `Expected table argument, got nil` (observed).
fn ignore_list(a: &Args, i: usize) -> ApiResult<Vec<InstanceId>> {
    if i >= a.len() {
        return Err(a.missing(i));
    }
    if !matches!(a.get(i), LuaValue::Table(_)) {
        return Err(ScriptError(format!(
            "Expected table argument, got {}",
            type_name(a.get(i))
        )));
    }
    a.instances(i)
}

/// `Camera:GetLargestCutoffDistance(ignoreList)`: how far in the camera
/// must move to see past what's between it and its Focus. Five rays run
/// parallel to the line from the Focus to the camera: through the near
/// plane's center and its four corners (by the field of view and the
/// viewport's shape, along the camera's own axes), each from that offset
/// off the Focus to that point of the near plane. Each ray that hits a
/// part cuts off the stretch from there to the near plane; the longest
/// is returned, 0 if none hit (all observed). Parts with CanCollide off
/// or more than 0.95 transparent are passed through, as are those in
/// groups that don't collide with Default; Terrain cuts off too.
fn largest_cutoff_distance(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let filter = Filter {
        respect_can_collide: true,
        max_transparency: Some(SEE_THROUGH),
        ..Filter::legacy_ray(ignore_list(a, 0)?, false)
    };
    let cutoff = with_world(lua, |w| {
        if !in_workspace(&w.dom, this) {
            return 0.0;
        }
        let lens = Lens::of(&w.dom, this);
        let near = near_plane(w, this);
        let focus = w
            .dom
            .cframe_property(this, "Focus")
            .map_or(Vec3::ZERO, |f| f.position);
        let rot = lens.cframe.rotation;
        let tan = (lens.fov.to_radians() * 0.5).tan();
        let corner = Vec2::new(tan * lens.viewport.x / lens.viewport.y, tan) * near;
        let offsets = [
            Vec2::ZERO,
            corner,
            Vec2::new(-corner.x, corner.y),
            -corner,
            Vec2::new(corner.x, -corner.y),
        ];
        offsets
            .into_iter()
            .filter_map(|o| {
                let offset = rot * Vec3::new(o.x, o.y, 0.0);
                let to = lens.cframe.position + offset - rot.z_axis * near;
                let from = focus + offset;
                let hit = w.physics.raycast(&mut w.dom, from, to - from, &filter)?;
                Some((to - from).length() - hit.distance)
            })
            .fold(0.0_f32, f32::max)
    });
    Outcome::ret(lua, f64::from(cutoff))
}

/// `Camera:GetPartsObscuringTarget(castPoints, ignoreList)`: the parts
/// a ray from the camera to each point passes before it, point by point
/// in order, each ray's nearest first and each part once. Entries that
/// aren't Vector3s are skipped; the rays have no length limit, act as
/// the Default group and see parts whatever their CanCollide and
/// Transparency, but not Terrain (all observed).
fn parts_obscuring_target(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let points: Vec<Vec3> = match a.get(0) {
        LuaValue::Table(t) => t
            .sequence_values::<LuaValue>()
            .filter_map(|v| match v {
                Ok(LuaValue::Vector(v)) => Some(Ok(from_lua_vector(v).0)),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<_, _>>()?,
        LuaValue::Nil => return Err(a.missing(0)),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            return Err(ScriptError("Unable to cast double to Array".into()));
        }
        v => {
            return Err(ScriptError(format!(
                "Unable to cast {} to Array",
                type_name(v)
            )));
        }
    };
    let filter = Filter::legacy_ray(ignore_list(a, 1)?, false);
    let parts = with_world(lua, |w| {
        if !in_workspace(&w.dom, this) {
            return Vec::new();
        }
        let from = w
            .dom
            .cframe_property(this, "CFrame")
            .map_or(Vec3::ZERO, |c| c.position);
        let mut seen = FxHashSet::default();
        let mut parts = Vec::new();
        for point in points {
            let along = (w.physics).parts_along(&mut w.dom, from, point - from, &filter);
            parts.extend(along.into_iter().filter(|p| seen.insert(*p)));
        }
        parts
    });
    crate::api::instance::ret_instance_array(lua, &parts)
}
