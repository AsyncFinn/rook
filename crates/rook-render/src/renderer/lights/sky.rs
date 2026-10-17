//! What blocks the sky's diffuse light (`EnvironmentDiffuseScale`): the
//! scene's opaque parts as boxes, marked into the light volume's voxels so
//! the fill pass can carry the sky's light down around them
//! (`shaders/light_volume.wgsl`; docs/behavior/materials.md, "The sky's
//! diffuse light").

use std::time::{Duration, Instant};

use glam::Mat3;

use super::*;

/// How soon after marking the voxels parts that moved may mark them
/// again (a moving part would otherwise mark them every frame).
const REMARK: Duration = Duration::from_millis(500);

/// A part as the sky's light sees it: a box.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Blocker {
    centre: Vec3,
    /// The box's axes (unit columns).
    axes: Mat3,
    /// Half the box along each axis, at least half a voxel, so a thin roof
    /// or wall still fills a layer of voxels (a 1-stud roof blocks fully
    /// in Studio).
    half: Vec3,
    min: Vec3,
    max: Vec3,
}

#[derive(Debug, Default)]
pub(super) struct SkyBlockers {
    /// By index into `Scene::parts`.
    blockers: Vec<Option<Blocker>>,
    /// Each mesh file's box in its own space (centre, half size); `None`
    /// for terrain, which doesn't block yet.
    bounds: HashMap<MeshHash, Option<(Vec3, Vec3)>>,
    /// A blocker changed since the voxels were marked.
    changed: bool,
    marked_at: Option<Instant>,
}

impl SkyBlockers {
    pub(super) fn clear(&mut self) {
        self.blockers.clear();
        self.changed = true;
    }

    /// Take the parts at `changed` (indices into `scene.parts`) as they
    /// are now.
    pub(super) fn update(&mut self, scene: &Scene, changed: &[usize]) {
        if self.blockers.len() < scene.parts.len() {
            self.blockers.resize(scene.parts.len(), None);
        }
        for &i in changed {
            let blocker = self.blocker(&scene.parts[i], scene);
            if self.blockers[i] != blocker {
                self.blockers[i] = blocker;
                self.changed = true;
            }
        }
    }

    /// Whether the voxels should be marked again for parts that changed.
    pub(super) fn due(&self) -> bool {
        self.changed && self.marked_at.is_none_or(|at| at.elapsed() >= REMARK)
    }

    /// Which parts block: opaque ones that cast shadows (measured: a roof
    /// with `CastShadow` off blocks nothing, one at Transparency 0.5 only
    /// a tenth or so), as boxes; wedges, balls, cylinders and unions
    /// block their whole box (Approximation).
    fn blocker(&mut self, part: &PartInstance, scene: &Scene) -> Option<Blocker> {
        if !part.cast_shadow || part.over || part.transparency >= 0.5 {
            return None;
        }
        if !matches!(part.layer, Layer::Base) {
            return None;
        }
        let (centre, half) = match part.shape {
            DrawShape::Part(_) => (Vec3::ZERO, part.size * 0.5),
            DrawShape::File(hash) => {
                let (centre, half) = match self.bounds.get(&hash) {
                    Some(bounds) => (*bounds)?,
                    // Not loaded yet: the part comes back when it is.
                    None => {
                        let bounds = mesh_bounds(scene.meshes.get(&hash)?);
                        self.bounds.insert(hash, bounds);
                        bounds?
                    }
                };
                (centre * part.size, half * part.size.abs())
            }
            DrawShape::Head | DrawShape::Truss(_) => return None,
        };
        let axes = part.rotation;
        let half = half.max(Vec3::splat(VOXEL * 0.5));
        let centre = part.position + axes * centre;
        let abs = Mat3::from_cols(axes.x_axis.abs(), axes.y_axis.abs(), axes.z_axis.abs());
        let extent = abs * half;
        Some(Blocker {
            centre,
            axes,
            half,
            min: centre - extent,
            max: centre + extent,
        })
    }

    /// One bit per voxel of the volume at `origin` (the centre of voxel
    /// (0, 0, 0)), `x + nx * (z + nz * y)`: set where a voxel's centre is
    /// inside a blocker.
    pub(super) fn mark(&mut self, origin: Vec3, dims: [u32; 3]) -> Vec<u32> {
        profiling::scope!("SkyBlockers::mark");
        self.changed = false;
        self.marked_at = Some(Instant::now());
        let [nx, ny, nz] = dims;
        let mut bits = vec![0u32; (nx * ny * nz).div_ceil(32) as usize];
        let last = Vec3::new(nx as f32 - 1.0, ny as f32 - 1.0, nz as f32 - 1.0);
        for b in self.blockers.iter().flatten() {
            let lo = ((b.min - origin) / VOXEL).ceil().max(Vec3::ZERO);
            let hi = ((b.max - origin) / VOXEL).floor().min(last);
            if lo.cmpgt(hi).any() {
                continue;
            }
            let to_local = b.axes.transpose();
            for y in lo.y as u32..=hi.y as u32 {
                for z in lo.z as u32..=hi.z as u32 {
                    for x in lo.x as u32..=hi.x as u32 {
                        let at = origin + Vec3::new(x as f32, y as f32, z as f32) * VOXEL;
                        let local = to_local * (at - b.centre);
                        if local.abs().cmple(b.half).all() {
                            let i = x + nx * (z + nz * y);
                            bits[(i / 32) as usize] |= 1 << (i % 32);
                        }
                    }
                }
            }
        }
        bits
    }
}

/// A mesh's box in its own space (centre, half size); `None` for terrain.
fn mesh_bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
    if mesh.terrain.is_some() {
        return None;
    }
    let (min, max) = mesh.positions.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), &p| (lo.min(Vec3::from(p)), hi.max(Vec3::from(p))),
    );
    if min.cmpgt(max).any() {
        return None;
    }
    Some(((min + max) * 0.5, (max - min) * 0.5))
}
