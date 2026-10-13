//! The sun's shadow map: one orthographic view from the sun, fitted around
//! what the camera sees within [`SHADOW_DISTANCE`] studs.
//!
//! Roblox's own shadows (ShadowMap and Future technologies, Voxel's soft
//! ones) aren't reproduced: this is one cascade, an Approximation tuned by
//! eye against Studio.

use glam::{Mat3, Mat4, Vec3};

use crate::Camera;

/// The shadow map's width and height in texels.
pub(crate) const SHADOW_SIZE: u32 = 2048;
/// How far from the camera shadows reach, in studs. They fade out over the
/// last fifth.
pub(crate) const SHADOW_DISTANCE: f32 = 300.0;
/// Parts this far further towards the sun than the fitted box (a tall
/// tower behind the camera) still cast into it.
const CASTER_REACH: f32 = 500.0;
/// Receivers look up the map this many texels out along their normal, so a
/// lit face doesn't shadow itself (acne).
const NORMAL_OFFSET_TEXELS: f32 = 1.5;
/// And this far towards the sun, in studs.
const DEPTH_BIAS_STUDS: f32 = 0.05;

/// The sun's view for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ShadowView {
    /// World to the shadow map's clip space (reverse Z: towards the sun is
    /// greater).
    pub view_proj: Mat4,
    /// The receiver's normal offset in studs.
    pub normal_offset: f32,
    /// [`DEPTH_BIAS_STUDS`] in depth units.
    pub depth_bias: f32,
}

impl ShadowView {
    /// The sun's view for `camera` at `aspect`, with `sun` pointing towards
    /// the sun (unit length).
    ///
    /// The box holds the smallest sphere around the camera's view out to
    /// [`SHADOW_DISTANCE`], whose size depends only on the field of view
    /// and aspect, so texels keep their size as the camera turns. Its
    /// centre moves in whole texels across the sun's view, so a texel's
    /// footprint on the ground stays put as the camera moves and shadow
    /// edges don't shimmer.
    pub fn fit(camera: &Camera, aspect: f32, sun: Vec3) -> Self {
        let d = SHADOW_DISTANCE;
        // Tangent of the angle to the view's corners.
        let k = (camera.fov_y.to_radians() / 2.0).tan() * (1.0 + aspect * aspect).sqrt();
        // The sphere's centre on the view axis, equally far from the eye
        // and the corners at distance `d`; past `d`, the corners alone.
        let ahead = (d * (1.0 + k * k) / 2.0).min(d);
        let radius = if ahead < d { ahead } else { d * k };
        // Whole studs, so a float wobble in the fit can't resize texels.
        let radius = radius.ceil();
        let texel = 2.0 * radius / SHADOW_SIZE as f32;

        // The sun's axes; no translation, so the texel grid is fixed in
        // the world and only the window over it moves.
        let forward = -sun;
        let up = if sun.y.abs() > 0.99 { Vec3::X } else { Vec3::Y };
        let right = forward.cross(up).normalize();
        let up = right.cross(forward);
        let view = Mat4::from_mat3(Mat3::from_cols(right, up, sun).transpose());

        let centre = view.transform_point3(camera.position + camera.forward() * ahead);
        let x = (centre.x / texel).floor() * texel;
        let y = (centre.y / texel).floor() * texel;
        // Distance along `forward`, from the plane through the origin.
        let mid = -centre.z;
        let near = mid - radius - CASTER_REACH;
        let far = mid + radius;
        // Reverse Z, as in the main pass: near is depth 1, far 0.
        let proj = glam::camera::rh::proj::directx::orthographic(
            x - radius,
            x + radius,
            y - radius,
            y + radius,
            far,
            near,
        );
        Self {
            view_proj: proj * view,
            normal_offset: texel * NORMAL_OFFSET_TEXELS,
            depth_bias: DEPTH_BIAS_STUDS / (far - near),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texel_of(view: &ShadowView, p: Vec3) -> glam::Vec2 {
        let clip = view.view_proj.project_point3(p);
        let half = SHADOW_SIZE as f32 / 2.0;
        glam::Vec2::new(clip.x * half, clip.y * half)
    }

    /// Moving the camera moves the map by whole texels, so a point in the
    /// world stays at the same place within its texel; turning it keeps
    /// the texel size.
    #[test]
    fn the_map_moves_in_whole_texels() {
        let sun = Vec3::new(0.3, 0.8, 0.5).normalize();
        let a = Camera::look_at(Vec3::new(0.0, 20.0, 40.0), Vec3::ZERO);
        let mut b = a;
        b.position += Vec3::new(3.37, 0.0, -1.91);
        let mut c = a;
        c.rotation = Mat3::from_rotation_y(1.0) * a.rotation;
        let (va, vb, vc) = (
            ShadowView::fit(&a, 16.0 / 9.0, sun),
            ShadowView::fit(&b, 16.0 / 9.0, sun),
            ShadowView::fit(&c, 16.0 / 9.0, sun),
        );
        assert_eq!(va.normal_offset, vb.normal_offset);
        assert_eq!(va.normal_offset, vc.normal_offset);
        let p = Vec3::new(5.3, 1.0, -7.7);
        let (ta, tb) = (texel_of(&va, p), texel_of(&vb, p));
        let shift = ta - tb;
        assert!(
            (shift - shift.round()).abs().max_element() < 1e-2,
            "{shift}"
        );
        // Towards the sun is nearer: greater depth.
        let near = va.view_proj.project_point3(p + sun * 10.0).z;
        assert!(near > va.view_proj.project_point3(p).z);
    }
}
