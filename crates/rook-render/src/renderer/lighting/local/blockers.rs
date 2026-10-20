//! What blocks light in the voxels: the scene's opaque parts as boxes,
//! marked into the light volume's voxels so the fill pass can carry the
//! sky's diffuse light (`EnvironmentDiffuseScale`) down around them and
//! keep lights with `Shadows` out from behind them
//! (`shaders/light_volume.wgsl`; docs/behavior/materials.md, "The sky's
//! diffuse light"; docs/behavior/lights.md, "Shadows").

use std::time::{Duration, Instant};

use glam::Mat3;

use super::*;

/// How soon after marking the voxels parts that moved may mark them
/// again (a moving part would otherwise mark them every frame).
const REMARK: Duration = Duration::from_millis(500);

/// A part as the voxels see it: a box.
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
pub(super) struct Blockers {
    /// By index into `Scene::parts`.
    blockers: Vec<Option<Blocker>>,
    /// Each mesh file's box in its own space (centre, half size); `None`
    /// for terrain, which doesn't block yet.
    bounds: HashMap<MeshHash, Option<(Vec3, Vec3)>>,
    /// A blocker changed since the voxels were marked.
    changed: bool,
    marked_at: Option<Instant>,
    /// The voxels as last marked, kept so parts that change only unmark
    /// their old box and mark their new one.
    grid: Option<Grid>,
    /// Blockers changed since the grid was marked: (before, after).
    moved: Vec<(Option<Blocker>, Option<Blocker>)>,
    /// Parts changed while nothing used the blockers, taken when
    /// something does; `stale` marks those listed.
    stale: Vec<usize>,
    is_stale: Vec<bool>,
}

/// The marked voxels of a volume.
#[derive(Debug)]
struct Grid {
    origin: Vec3,
    dims: [u32; 3],
    /// Blockers covering each voxel.
    counts: Vec<u32>,
    /// A bit per voxel covered at all, as the fill pass reads it.
    bits: Vec<u32>,
}

impl Grid {
    fn new(origin: Vec3, dims: [u32; 3]) -> Self {
        let voxels = (dims[0] * dims[1] * dims[2]) as usize;
        Self {
            origin,
            dims,
            counts: vec![0; voxels],
            bits: vec![0; voxels.div_ceil(32)],
        }
    }

    /// Count `b` in (`add`) or out of the voxels whose centres it holds.
    fn apply(&mut self, b: &Blocker, add: bool) {
        let [nx, ny, nz] = self.dims;
        let origin = self.origin;
        let last = Vec3::new(nx as f32 - 1.0, ny as f32 - 1.0, nz as f32 - 1.0);
        let lo = ((b.min - origin) / VOXEL).ceil().max(Vec3::ZERO);
        let hi = ((b.max - origin) / VOXEL).floor().min(last);
        if lo.cmpgt(hi).any() {
            return;
        }
        let to_local = b.axes.transpose();
        for y in lo.y as u32..=hi.y as u32 {
            for z in lo.z as u32..=hi.z as u32 {
                for x in lo.x as u32..=hi.x as u32 {
                    let at = origin + Vec3::new(x as f32, y as f32, z as f32) * VOXEL;
                    let local = to_local * (at - b.centre);
                    if !local.abs().cmple(b.half).all() {
                        continue;
                    }
                    let i = (x + nx * (z + nz * y)) as usize;
                    let (word, bit) = (i / 32, 1 << (i % 32));
                    if add {
                        self.counts[i] += 1;
                        self.bits[word] |= bit;
                    } else {
                        self.counts[i] -= 1;
                        if self.counts[i] == 0 {
                            self.bits[word] &= !bit;
                        }
                    }
                }
            }
        }
    }
}

impl Blockers {
    pub(super) fn clear(&mut self) {
        self.blockers.clear();
        self.changed = true;
        self.grid = None;
        self.moved.clear();
        self.stale.clear();
        self.is_stale.clear();
    }

    /// Take the parts at `changed` (indices into `scene.parts`) as they
    /// are now, or, while the blockers aren't `used`, note them for later.
    pub(super) fn update(&mut self, scene: &Scene, changed: &[usize], used: bool) {
        if self.blockers.len() < scene.parts.len() {
            self.blockers.resize(scene.parts.len(), None);
            self.is_stale.resize(scene.parts.len(), false);
        }
        if !used {
            for &i in changed {
                if !std::mem::replace(&mut self.is_stale[i], true) {
                    self.stale.push(i);
                }
            }
            return;
        }
        let stale = std::mem::take(&mut self.stale);
        for &i in stale.iter().chain(changed) {
            self.is_stale[i] = false;
            let blocker = self.blocker(&scene.parts[i], scene);
            if self.blockers[i] != blocker {
                if self.grid.is_some() {
                    self.moved.push((self.blockers[i], blocker));
                }
                self.blockers[i] = blocker;
                self.changed = true;
            }
        }
        // More changes than parts (or nothing marking the voxels for a
        // while): marking afresh is cheaper.
        if self.moved.len() > self.blockers.len() {
            self.grid = None;
            self.moved.clear();
        }
    }

    /// Whether the voxels should be marked again for parts that changed.
    pub(super) fn due(&self) -> bool {
        self.changed && self.marked_at.is_none_or(|at| at.elapsed() >= REMARK)
    }

    /// Which parts block: opaque ones that cast shadows (measured: a roof
    /// with `CastShadow` off blocks nothing, nor does such a post block
    /// lights with `Shadows`; a roof at Transparency 0.5 blocks only a
    /// tenth or so), as boxes; wedges, balls, cylinders and unions block
    /// their whole box (Approximation).
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
            DrawShape::Head | DrawShape::Truss(_) | DrawShape::Adorn(_) => return None,
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
    /// inside a blocker. The same volume as last time only takes the
    /// blockers that changed since; a new one is marked from scratch.
    pub(super) fn mark(&mut self, origin: Vec3, dims: [u32; 3]) -> &[u32] {
        profiling::scope!("Blockers::mark");
        self.changed = false;
        self.marked_at = Some(Instant::now());
        let moved = std::mem::take(&mut self.moved);
        match &mut self.grid {
            Some(grid) if grid.origin == origin && grid.dims == dims => {
                for (before, after) in &moved {
                    if let Some(b) = before {
                        grid.apply(b, false);
                    }
                    if let Some(b) = after {
                        grid.apply(b, true);
                    }
                }
            }
            grid => {
                let mut fresh = Grid::new(origin, dims);
                for b in self.blockers.iter().flatten() {
                    fresh.apply(b, true);
                }
                *grid = Some(fresh);
            }
        }
        &self.grid.as_ref().expect("just marked").bits
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
