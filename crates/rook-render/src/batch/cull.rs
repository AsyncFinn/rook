//! Skipping draws nothing in them can be seen in: each batch keeps a box
//! around its instances in the world, and one per run of 16 of them, and a
//! pass records only the runs whose box reaches into the pass's view (the camera's, or the
//! sun's for the shadow map). The GPU clips away the same triangles, so no
//! pixel changes; big places just record far fewer draws.

use std::ops::Range;

use glam::{Mat4, Vec3, Vec4};

/// How far the main pass's shader moves a vertex from where its instance
/// puts it, at most: grass blades lean and sway up to about 3 studs and
/// sink up to 3.7, and water rises and falls by `WaterWaveSize`
/// (`shaders/terrain.wgsl`), so boxes reach this much further.
pub(crate) const MOTION_STUDS: f32 = 4.0;

/// A box in the world, along its axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// Holds nothing; any view misses it.
    pub const EMPTY: Self = Self {
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };

    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.min.cmpgt(self.max).any()
    }
}

/// What a view keeps: the six planes bounding its clip volume (x and y
/// from -w to w, z from 0 to w), in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn of(view_proj: Mat4) -> Self {
        let [x, y, z, w] = [0, 1, 2, 3].map(|i| view_proj.row(i));
        Self {
            planes: [w + x, w - x, w + y, w - y, z, w - z],
        }
    }

    /// Whether any of `b`, grown by `margin` studs, is inside. A box in
    /// front of all six planes may still miss the view near a corner;
    /// that only costs a draw.
    pub fn sees(&self, b: Aabb, margin: f32) -> bool {
        if b.is_empty() {
            return false;
        }
        let centre = (b.min + b.max) * 0.5;
        let half = (b.max - b.min) * 0.5 + Vec3::splat(margin);
        self.planes.iter().all(|p| {
            let n = p.truncate();
            n.dot(centre) + n.abs().dot(half) + p.w >= 0.0
        })
    }
}

/// Where `position` falls along a Z-order curve through 16-stud cells:
/// entries placed in this order fill a batch's slots near each other, so
/// each run of [`CULL_SLOTS`](crate::batch::CULL_SLOTS) slots gets a small
/// box that views can skip.
pub(crate) fn spatial_key(position: Vec3) -> u64 {
    // 21 bits an axis, centred on the origin (±16.7M studs).
    let cell = |v: f32| ((v / 16.0).floor() as i64 + (1 << 20)).clamp(0, (1 << 21) - 1) as u64;
    let spread = |mut v: u64| {
        v = (v | (v << 32)) & 0x1f00000000ffff;
        v = (v | (v << 16)) & 0x1f0000ff0000ff;
        v = (v | (v << 8)) & 0x100f00f00f00f00f;
        v = (v | (v << 4)) & 0x10c30c30c30c30c3;
        (v | (v << 2)) & 0x1249249249249249
    };
    spread(cell(position.x)) | spread(cell(position.y)) << 1 | spread(cell(position.z)) << 2
}

/// The runs of a batch's `instances` that `view` sees: consecutive
/// `chunks` (a box per `per` instances) it sees, joined, in order.
pub(crate) fn seen_runs<'a>(
    view: &'a Frustum,
    chunks: &'a [Aabb],
    per: u32,
    margin: f32,
    instances: Range<u32>,
) -> impl Iterator<Item = Range<u32>> + 'a {
    let mut c = 0;
    std::iter::from_fn(move || {
        while c < chunks.len() && !view.sees(chunks[c], margin) {
            c += 1;
        }
        if c == chunks.len() {
            return None;
        }
        let first = c;
        while c < chunks.len() && view.sees(chunks[c], margin) {
            c += 1;
        }
        let start = instances.start + first as u32 * per;
        let end = (instances.start + c as u32 * per).min(instances.end);
        (start < end).then_some(start..end)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Camera;

    fn cube(at: Vec3) -> Aabb {
        Aabb {
            min: at - Vec3::ONE,
            max: at + Vec3::ONE,
        }
    }

    /// The camera at the origin looks down -Z: boxes ahead are seen,
    /// boxes behind it or far off to the side aren't, nor an empty one.
    #[test]
    fn the_camera_sees_what_is_ahead() {
        let camera = Camera::look_at(Vec3::ZERO, Vec3::NEG_Z);
        let view = Frustum::of(camera.view_projection(16.0 / 9.0));
        assert!(view.sees(cube(Vec3::new(0.0, 0.0, -10.0)), 0.0));
        assert!(view.sees(cube(Vec3::new(0.0, 0.0, -5000.0)), 0.0));
        assert!(!view.sees(cube(Vec3::new(0.0, 0.0, 10.0)), 0.0));
        assert!(!view.sees(cube(Vec3::new(100.0, 0.0, -10.0)), 0.0));
        assert!(!view.sees(Aabb::EMPTY, MOTION_STUDS));
        // Grown far enough, the box beside the view reaches into it.
        assert!(view.sees(cube(Vec3::new(30.0, 0.0, -10.0)), 25.0));
    }

    /// Seen chunks join into runs; the last run stops at the batch's end.
    #[test]
    fn seen_chunks_join_into_runs() {
        let camera = Camera::look_at(Vec3::ZERO, Vec3::NEG_Z);
        let view = Frustum::of(camera.view_projection(16.0 / 9.0));
        let (ahead, behind) = (cube(Vec3::NEG_Z * 10.0), cube(Vec3::Z * 10.0));
        let chunks = [ahead, behind, ahead, ahead];
        let runs: Vec<_> = seen_runs(&view, &chunks, 16, 0.0, 100..155).collect();
        assert_eq!(runs, [100..116, 132..155]);
    }
}
