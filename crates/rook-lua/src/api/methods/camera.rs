//! Camera rays and projection: `ViewportPointToRay`, `ScreenPointToRay`,
//! `WorldToViewportPoint` and `WorldToScreenPoint`. The engine's mouse uses
//! [`viewport_ray`] too, so Mouse.Hit and scripts' rays agree.
//!
//! Behaviour observed in Studio 0.742: `FieldOfView` is vertical; a ray
//! starts on the near plane (0.5 studs in front of the camera), moved
//! `depth` studs further along its unit direction; a server's camera has
//! a 1 × 1 viewport.

use glam::{Vec2, Vec3};
use mlua::Lua;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Vector3};

use super::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::convert::vector3_to_lua;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::types::LRay;

/// Camera.NearPlaneZ, which Roblox doesn't let scripts change.
const NEAR_PLANE: f32 = 0.5;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Camera", "ViewportPointToRay") => viewport_point_to_ray,
        ("Camera", "ScreenPointToRay") => screen_point_to_ray,
        ("Camera", "WorldToViewportPoint") => world_to_viewport_point,
        ("Camera", "WorldToScreenPoint") => world_to_screen_point,
        _ => return None,
    })
}

/// What projecting through a camera needs.
#[derive(Debug, Clone, Copy)]
pub struct Lens {
    pub cframe: CFrame,
    /// Vertical field of view, in degrees.
    pub fov: f32,
    /// Viewport size in pixels.
    pub viewport: Vec2,
}

impl Lens {
    /// A camera's lens, from its properties.
    pub fn of(dom: &Dom, camera: InstanceId) -> Self {
        let cframe = dom
            .cframe_property(camera, "CFrame")
            .unwrap_or(CFrame::IDENTITY);
        let fov = dom.float_property(camera, "FieldOfView").unwrap_or(70.0);
        let viewport = match dom.vector2_property(camera, "ViewportSize") {
            Some(v) if v.x() > 0.0 && v.y() > 0.0 => v.0,
            _ => Vec2::ONE,
        };
        Self {
            cframe,
            fov,
            viewport,
        }
    }

    fn tan(&self) -> Vec2 {
        let t = (self.fov.to_radians() * 0.5).tan();
        Vec2::new(t * self.viewport.x / self.viewport.y, t)
    }
}

/// The ray through viewport pixel (`x`, `y`): its origin on the near plane
/// and its unit direction (`Camera:ViewportPointToRay` with depth 0).
pub fn viewport_ray(lens: &Lens, x: f32, y: f32) -> (Vec3, Vec3) {
    let ndc = Vec2::new(
        2.0 * x / lens.viewport.x - 1.0,
        1.0 - 2.0 * y / lens.viewport.y,
    );
    let t = ndc * lens.tan();
    let local = Vec3::new(t.x, t.y, -1.0);
    let rot = lens.cframe.rotation;
    let origin = lens.cframe.position + rot * (local * NEAR_PLANE);
    (origin, (rot * local).normalize())
}

/// Where a world point lands in the viewport: pixel x and y, its depth in
/// front of the camera, and whether it's on screen.
fn project(lens: &Lens, point: Vec3) -> (Vec3, bool) {
    let local = lens.cframe.rotation.transpose() * (point - lens.cframe.position);
    let depth = -local.z;
    let t = lens.tan();
    let ndc = Vec2::new(local.x / (depth * t.x), local.y / (depth * t.y));
    let x = (ndc.x + 1.0) * 0.5 * lens.viewport.x;
    let y = (1.0 - ndc.y) * 0.5 * lens.viewport.y;
    let visible = depth > 0.0 && ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0;
    (Vec3::new(x, y, depth), visible)
}

fn point_to_ray(lua: &Lua, this: InstanceId, a: &Args, inset: f32) -> ApiResult<Outcome> {
    let (x, y) = (a.number(0)? as f32, a.number(1)? as f32);
    let depth = a.number_or(2, 0.0)? as f32;
    let lens = with_world(lua, |w| Lens::of(&w.dom, this));
    let (origin, dir) = viewport_ray(&lens, x, y + inset);
    let origin = origin + dir * depth;
    let ray = rbx_types::Ray::new(Vector3(origin).into(), Vector3(dir).into());
    Outcome::ret(lua, LRay(ray))
}

fn viewport_point_to_ray(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    point_to_ray(lua, this, a, 0.0)
}

fn screen_point_to_ray(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    point_to_ray(lua, this, a, super::gui::gui_inset(lua))
}

fn world_to_point(lua: &Lua, this: InstanceId, a: &Args, inset: f32) -> ApiResult<Outcome> {
    let point = a.vector3(0)?.0;
    let lens = with_world(lua, |w| Lens::of(&w.dom, this));
    let (p, visible) = project(&lens, point);
    let p = Vec3::new(p.x, p.y - inset, p.z);
    Outcome::ret(lua, (vector3_to_lua(Vector3(p)), visible))
}

fn world_to_viewport_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    world_to_point(lua, this, a, 0.0)
}

fn world_to_screen_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    world_to_point(lua, this, a, super::gui::gui_inset(lua))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lens(position: Vec3) -> Lens {
        Lens {
            cframe: CFrame::from_position(position),
            fov: 70.0,
            viewport: Vec2::ONE,
        }
    }

    #[test]
    fn rays_start_on_the_near_plane() {
        // Observed on a server camera at (10, 20, 30).
        let l = lens(Vec3::new(10.0, 20.0, 30.0));
        let (o, d) = viewport_ray(&l, 0.5, 0.5);
        assert!((o - Vec3::new(10.0, 20.0, 29.5)).length() < 1e-5);
        assert!((d - Vec3::NEG_Z).length() < 1e-6);
        let (o, d) = viewport_ray(&l, 0.0, 0.0);
        assert!(
            (o - Vec3::new(9.649_897, 20.350_103, 29.5)).length() < 1e-4,
            "{o}"
        );
        assert!(
            (d - Vec3::new(-0.497_543, 0.497_543, -0.710_565)).length() < 1e-5,
            "{d}"
        );
    }

    #[test]
    fn projection_inverts_the_ray() {
        let l = Lens {
            viewport: Vec2::new(800.0, 600.0),
            ..lens(Vec3::ZERO)
        };
        let (o, d) = viewport_ray(&l, 200.0, 450.0);
        let (p, visible) = project(&l, o + d * 40.0);
        assert!(visible);
        assert!(
            (p.x - 200.0).abs() < 1e-2 && (p.y - 450.0).abs() < 1e-2,
            "{p}"
        );
        let (p, _) = project(&l, Vec3::new(0.0, 0.0, -30.0));
        assert!((p - Vec3::new(400.0, 300.0, 30.0)).length() < 1e-3);
        assert!(!project(&l, Vec3::new(0.0, 0.0, 5.0)).1);
    }
}
