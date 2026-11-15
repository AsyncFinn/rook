//! What moves a camera from a script: `Interpolate`, the legacy orbit
//! steps (`PanUnits`, `TiltUnits`), `ZoomToExtents`, and the Focus a
//! Scriptable camera keeps (docs/behavior/characters/camera.md#scripted-cameras).

use glam::{Vec2, Vec3};
use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::{CFrame, Value};

use super::{RawMethod, mutate};
use crate::ctx::args::{Args, type_name};
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::signal::SignalKey;
use crate::threads::fire;
use crate::types::LCFrame;
use crate::world::World;

/// How far in front of a Scriptable camera its Focus sits (Observed).
const FOCUS_AHEAD: f32 = 20.0;

/// `PanUnits` turns by this many degrees a unit, from a heading rounded
/// to a whole number of them (Observed).
const PAN_STEP: f32 = 45.0;
/// `TiltUnits` tilts by this many degrees a unit, from a pitch rounded to
/// a whole number of them, to at most [`MAX_TILT`] up or down (Observed).
const TILT_STEP: f32 = 10.0;
const MAX_TILT: f32 = 81.0;
/// `ZoomToExtents` fits a box at least this many studs in radius
/// (Observed: a box of no size).
const MIN_EXTENT: f32 = 0.5;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Camera", "Interpolate") => interpolate,
        ("Camera", "PanUnits") => pan_units,
        ("Camera", "TiltUnits") => tilt_units,
        ("Camera", "ZoomToExtents") => zoom_to_extents,
        // Deprecated: both read 0 whatever the camera does (Observed).
        ("Camera", "GetPanSpeed" | "GetTiltSpeed") => |lua, _, _| Outcome::ret(lua, 0.0),
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

// Orbiting the Focus

/// An `int` argument: a number rounded to the nearest whole one, halves
/// away from zero (Observed: 1.7 and 1.5 count as 2).
fn int(a: &Args, i: usize) -> ApiResult<i64> {
    match a.get(i) {
        mlua::Value::Nil => Err(a.missing(i)),
        v => crate::ctx::args::number_of(v)
            .map(|n| n.round() as i64)
            .ok_or_else(|| ScriptError(format!("Unable to cast {} to int", type_name(v)))),
    }
}

/// Where a camera is around its Focus: its look vector and its distance
/// from the Focus.
struct Orbit {
    focus: Vec3,
    look: Vec3,
    distance: f32,
}

impl Orbit {
    fn of(w: &World, camera: InstanceId) -> Self {
        let cf = w
            .dom
            .cframe_property(camera, "CFrame")
            .unwrap_or(CFrame::IDENTITY);
        let focus = w
            .dom
            .cframe_property(camera, "Focus")
            .map_or(Vec3::ZERO, |f| f.position);
        Self {
            focus,
            look: -cf.rotation.z_axis,
            distance: (cf.position - focus).length(),
        }
    }

    /// The look's heading in degrees, clockwise from -Z seen from above.
    fn heading(&self) -> f32 {
        (-self.look.x).atan2(-self.look.z).to_degrees()
    }

    fn pitch(&self) -> f32 {
        self.look.y.clamp(-1.0, 1.0).asin().to_degrees()
    }

    /// The camera looking along `look` at the Focus from its distance, as
    /// `CFrame.lookAt` turns it: no roll. One on its Focus goes a stud out
    /// (Observed).
    fn cframe(&self, look: Vec3) -> CFrame {
        let distance = if self.distance > 0.0 {
            self.distance
        } else {
            1.0
        };
        CFrame::look_at(self.focus - look * distance, self.focus, Vec3::Y)
    }
}

/// A look `degrees` clockwise from -Z seen from above, `across` long
/// level and `up` high.
fn heading_look(degrees: f32, across: f32, up: f32) -> Vec3 {
    let (sin, cos) = degrees.to_radians().sin_cos();
    Vec3::new(-sin * across, up, -cos * across)
}

fn set_cframe(lua: &Lua, camera: InstanceId, cf: CFrame) -> ApiResult<()> {
    mutate(lua, |w| {
        w.dom
            .set_property_by_name(camera, "CFrame", Value::CFrame(cf))
    })
}

/// `Camera:PanUnits(units)`: orbits the Focus, the look's heading rounded
/// to a multiple of 45° then turned 45° clockwise (seen from above) a
/// unit, its pitch and the distance kept, then aimed at the Focus by
/// `CFrame.lookAt`, so the roll goes and a camera looking straight down
/// comes out as `lookAt` turns one (all observed).
fn pan_units(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let units = int(a, 0)?;
    let orbit = with_world(lua, |w| Orbit::of(w, this));
    let heading = (orbit.heading() / PAN_STEP).round() * PAN_STEP - PAN_STEP * units as f32;
    let level = Vec2::new(orbit.look.x, orbit.look.z).length();
    let look = heading_look(heading, level, orbit.look.y);
    set_cframe(lua, this, orbit.cframe(look))?;
    Outcome::ret(lua, ())
}

/// `Camera:TiltUnits(units)`: orbits the Focus, the look's pitch rounded
/// to a multiple of 10° then raised 10° a unit and kept within 81° of
/// level, its heading and the distance kept, aimed by `CFrame.lookAt`.
/// True unless the pitch asked for is the one it had (all observed:
/// false only for 0 units on a whole multiple of 10°, true at the limit).
fn tilt_units(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let units = int(a, 0)?;
    let orbit = with_world(lua, |w| Orbit::of(w, this));
    let pitch = orbit.pitch();
    let target = (pitch / TILT_STEP).round() * TILT_STEP + TILT_STEP * units as f32;
    let (up, across) = target.clamp(-MAX_TILT, MAX_TILT).to_radians().sin_cos();
    let look = heading_look(orbit.heading(), across, up);
    set_cframe(lua, this, orbit.cframe(look))?;
    Outcome::ret(lua, target != pitch)
}

/// `Camera:ZoomToExtents(boxCFrame, boxSize)`: Focus goes to the box's
/// center (unturned) and the camera, turned as it was, backs off it until
/// the sphere around the box's world-axis box fills the narrower of the
/// field of view's two half angles (all observed).
fn zoom_to_extents(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let at = a.userdata::<LCFrame>(0, "CFrame")?.0;
    let size = a.vector3(1)?.0.abs();
    let lens = with_world(lua, |w| super::rays::Lens::of(&w.dom, this));
    let radius = at.box_reach(size).length().max(MIN_EXTENT);
    let tan = (lens.fov.to_radians() * 0.5).tan();
    let half = tan.min(tan * lens.viewport.x / lens.viewport.y).atan();
    let distance = radius / half.sin();
    let cf = CFrame {
        position: at.position + lens.cframe.rotation.z_axis * distance,
        rotation: lens.cframe.rotation,
    };
    mutate(lua, |w| {
        w.dom.set_property_by_name(
            this,
            "Focus",
            Value::CFrame(CFrame::from_position(at.position)),
        )?;
        w.dom
            .set_property_by_name(this, "CFrame", Value::CFrame(cf))
    })?;
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
