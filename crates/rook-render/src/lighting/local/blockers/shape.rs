//! A blocker's shape: a part's box in the world, and the voxels it covers.

use super::*;

/// A part as the voxels see it: a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::lighting::local) struct Blocker {
    pub(in crate::lighting::local) center: Vec3,
    /// The box's axes (unit columns).
    pub(in crate::lighting::local) axes: Mat3,
    /// Half the box along each axis, at least half a voxel, so a thin roof
    /// or wall still blocks a layer of voxels (in Studio a 0.2-stud roof
    /// blocks the sky and the sun as a thick one does).
    pub(in crate::lighting::local) half: Vec3,
    pub(in crate::lighting::local) min: Vec3,
    pub(in crate::lighting::local) max: Vec3,
    /// The box as it is, axis-aligned (for a rotated part, around it).
    pub(in crate::lighting::local) true_min: Vec3,
    pub(in crate::lighting::local) true_max: Vec3,
}

impl Blocker {
    /// A box centered at `center` along `axes`, `half` its size each way.
    pub(in crate::lighting::local) fn of_box(center: Vec3, axes: Mat3, true_half: Vec3) -> Self {
        let half = true_half.max(Vec3::splat(VOXEL * 0.5));
        let extent = rook_types::box_reach(axes, half);
        let true_extent = rook_types::box_reach(axes, true_half);
        Self {
            center,
            axes,
            half,
            min: center - extent,
            max: center + extent,
            true_min: center - true_extent,
            true_max: center + true_extent,
        }
    }

    /// The voxels whose centers it may hold.
    pub(in crate::lighting::local) fn centers(&self) -> Span {
        let lo = ((self.min - VOXEL * 0.5) / VOXEL).ceil().as_ivec3();
        let hi = ((self.max - VOXEL * 0.5) / VOXEL).floor().as_ivec3();
        (lo, hi)
    }

    /// Whether its axes are the world's, in some order.
    pub(in crate::lighting::local) fn aligned(&self) -> bool {
        [self.axes.x_axis, self.axes.y_axis, self.axes.z_axis]
            .iter()
            .all(|a| a.abs().max_element() > 0.9999)
    }

    /// The voxels it fills whole, if it is aligned with the world (their
    /// centers are inside it too); `None` if none are.
    pub(super) fn whole(&self) -> Option<Span> {
        if !self.aligned() {
            return None;
        }
        let lo = (self.true_min / VOXEL).ceil().as_ivec3();
        let hi = (self.true_max / VOXEL).floor().as_ivec3() - 1;
        (!lo.cmpgt(hi).any()).then_some((lo, hi))
    }
}

/// A mesh's box in its own space (center, half size); `None` for terrain
/// or a mesh with no vertex that is a number.
pub(super) fn mesh_bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
    if mesh.terrain.is_some() {
        return None;
    }
    let (min, max) = crate::mesh::bounds_of(mesh.positions.iter().copied())?;
    Some(((min + max) * 0.5, (max - min) * 0.5))
}

impl Blockers {
    /// Which parts block: opaque ones that cast shadows (measured: a roof
    /// with `CastShadow` off blocks nothing, nor does such a post block
    /// lights with `Shadows`; a roof at Transparency 0.5 blocks only a
    /// tenth or so), as boxes; wedges, balls, cylinders and unions block
    /// their whole box (Approximation).
    pub(super) fn blocker(&mut self, part: &PartInstance, scene: &Scene) -> Option<Blocker> {
        // A ViewportFrame's parts are in a world of their own.
        if part.space != Space::Workspace
            || !part.cast_shadow
            || part.over
            || part.transparency >= 0.5
        {
            return None;
        }
        if !matches!(part.layer, Layer::Base) {
            return None;
        }
        let (center, half) = match part.shape {
            DrawShape::Part(_) => (Vec3::ZERO, part.size * 0.5),
            DrawShape::File(hash) => {
                let (center, half) = match self.bounds.get(&hash) {
                    Some(bounds) => (*bounds)?,
                    // Not loaded yet: the part comes back when it is.
                    None => {
                        let bounds = mesh_bounds(scene.meshes.get(&hash)?);
                        self.bounds.insert(hash, bounds);
                        bounds?
                    }
                };
                (center * part.size, half * part.size.abs())
            }
            DrawShape::Head | DrawShape::Truss(_) | DrawShape::Adorn(_) => return None,
        };
        let axes = part.rotation;
        let b = Blocker::of_box(part.position + axes * center, axes, half);
        // Voxel spans are i32s, grown and shrunk by one; a part parked
        // past `REACH` (a float's range away, say) is in no grid's voxels.
        let within = |v: Vec3| v.abs().cmplt(Vec3::splat(REACH)).all();
        (within(b.min) && within(b.max)).then_some(b)
    }
}
