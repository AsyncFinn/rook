//! `Camera:Interpolate`, and the Focus a Scriptable camera keeps
//! (docs/behavior/characters/camera.md#scripted-cameras).

use glam::Vec3;
use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::{CFrame, Value};

use super::{RawMethod, mutate};
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::signal::SignalKey;
use crate::threads::fire;
use crate::types::LCFrame;
use crate::world::World;

/// How far in front of a Scriptable camera its Focus sits (Observed).
const FOCUS_AHEAD: f32 = 20.0;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Camera", "Interpolate") => interpolate,
        _ => return None,
    })
}

/// A camera on its way to where `Interpolate` sent it.
#[derive(Debug, Clone)]
pub(crate) struct Interpolation {
    from: Vec3,
    to: CFrame,
    focus_from: Vec3,
    focus_to: Vec3,
    start: f64,
    duration: f64,
    /// The CFrame last set; a script setting another ends the move.
    wrote: CFrame,
}

impl Interpolation {
    fn ends(&self) -> f64 {
        self.start + self.duration
    }
}

/// `Camera:Interpolate(endPos, endFocus, duration)`: from where it is, the
/// position moves straight to `endPos`'s and the camera looks at a focus
/// moving straight to `endFocus`'s, both at a steady pace (Observed), and
/// ends on `endPos` itself. A new call starts over from where the camera
/// is; only a Scriptable camera moves this way.
fn interpolate(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let to = a.userdata::<LCFrame>(0, "CFrame")?.0;
    let focus_to = a.userdata::<LCFrame>(1, "CFrame")?.0.position;
    let duration = a.number(2)?;
    if duration <= 0.0 {
        return Err(ScriptError("Interpolation time must be positive.".into()));
    }
    let scriptable = with_world(lua, |w| {
        w.dom.enum_item_property(this, "CameraType") == Some("Scriptable")
    });
    if !scriptable {
        return Err(ScriptError(
            "Attempted to use interpolation with a camera mode other than scriptable.".into(),
        ));
    }
    with_world(lua, |w| {
        let from = w
            .dom
            .cframe_property(this, "CFrame")
            .unwrap_or(CFrame::IDENTITY);
        let focus_from = w
            .dom
            .cframe_property(this, "Focus")
            .map_or(from.position, |f| f.position);
        let start = w.scheduler.now();
        w.interpolations.insert(
            this,
            Interpolation {
                from: from.position,
                to,
                focus_from,
                focus_to,
                start,
                duration,
                wrote: from,
            },
        );
    });
    Outcome::ret(lua, ())
}

/// Each client frame, before drawing: interpolating cameras move, firing
/// `InterpolationFinished` when they arrive, and the current camera's
/// Focus, when Scriptable, goes 20 studs in front of it (Observed: even
/// after a script set it).
pub(crate) fn step(lua: &Lua) {
    let Ok(finished) = mutate(lua, |w| {
        let now = w.scheduler.now();
        let mut finished = Vec::new();
        let cameras: Vec<InstanceId> = w.interpolations.keys().copied().collect();
        for camera in cameras {
            let moving = advance(w, camera, now)?;
            let arrived = w
                .interpolations
                .get(&camera)
                .is_some_and(|i| i.ends() <= now);
            if !moving || arrived {
                w.interpolations.remove(&camera);
            }
            if moving && arrived {
                finished.push(camera);
            }
        }
        keep_focus(w)?;
        Ok(finished)
    }) else {
        return;
    };
    for camera in finished {
        fire(lua, &SignalKey::event(camera, "InterpolationFinished"), &[]);
    }
}

/// Move `camera` along its interpolation; false if a script moved it
/// meanwhile, which ends it (Observed: it stays where written).
fn advance(w: &mut World, camera: InstanceId, now: f64) -> Result<bool, rook_dom::DomError> {
    let Some(i) = w.interpolations.get(&camera) else {
        return Ok(false);
    };
    if w.dom.cframe_property(camera, "CFrame") != Some(i.wrote) {
        return Ok(false);
    }
    let s = ((now - i.start) / i.duration).clamp(0.0, 1.0) as f32;
    let cf = if s >= 1.0 {
        i.to
    } else {
        let at = i.from.lerp(i.to.position, s);
        let target = i.focus_from.lerp(i.focus_to, s);
        if (target - at).length_squared() > 1e-8 {
            CFrame::look_at(at, target, Vec3::Y)
        } else {
            CFrame {
                position: at,
                rotation: i.to.rotation,
            }
        }
    };
    if let Some(i) = w.interpolations.get_mut(&camera) {
        i.wrote = cf;
    }
    w.dom
        .set_property_by_name(camera, "CFrame", Value::CFrame(cf))?;
    Ok(true)
}

fn keep_focus(w: &mut World) -> Result<(), rook_dom::DomError> {
    let camera = w
        .dom
        .find_service("Workspace")
        .and_then(|ws| w.dom.ref_property(ws, "CurrentCamera"))
        .filter(|&c| w.dom.enum_item_property(c, "CameraType") == Some("Scriptable"));
    let Some(camera) = camera else {
        return Ok(());
    };
    let cf = w
        .dom
        .cframe_property(camera, "CFrame")
        .unwrap_or(CFrame::IDENTITY);
    let ahead = CFrame {
        position: cf.position + cf.rotation * Vec3::new(0.0, 0.0, -FOCUS_AHEAD),
        rotation: cf.rotation,
    };
    if w.dom.cframe_property(camera, "Focus") != Some(ahead) {
        w.dom
            .set_property_by_name(camera, "Focus", Value::CFrame(ahead))?;
    }
    Ok(())
}
