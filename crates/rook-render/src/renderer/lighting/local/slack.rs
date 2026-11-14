//! How far a blocker may move before anything it marks in the voxels could
//! change, so a part that barely moves (one physics keeps nudging, or one
//! creeping a little each frame) isn't marked again every frame. This is a
//! physics broadphase's fattened bounds (Box2D's `b2_aabbMargin`): a body
//! is inserted again only once it leaves them. Here the bounds are exact.
//! The voxels are a lattice fixed in the world, so what a blocker marks
//! depends on its pose alone (the voxel centers its box holds), and a
//! blocker kept at the pose it was marked at while it moves within its
//! slack marks exactly what it would where it is now. Only the voxels the
//! volume holds are looked at, so a part far bigger than the volume costs
//! what it covers of it; a slack holds only while the volume stays put.

use glam::{IVec3, Vec3};

use super::VOXEL;
use super::blockers::{Blocker, Span, center, clip};

/// How far a blocker may move from the pose it was marked at, in studs;
/// nothing at all by default.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Slack {
    /// The most any point within `reach` of its center may move: the
    /// voxel centers it holds or nearly holds.
    rigid: f32,
    reach: f32,
    /// The voxels it was worked out over: the volume's then.
    within: Span,
}

impl Slack {
    /// The slack of `b`, at the pose it is marked at, over the voxels
    /// `within`.
    pub(super) fn of(b: &Blocker, within: Span) -> Self {
        let reach = reach(b);
        // Both poses' sums in f32 may each be off by about this much.
        let error = 4.0 * f32::EPSILON * (b.center.length() + 2.0 * reach) + 1e-5;
        let to_local = b.axes.transpose();
        // Voxel centers: those one beyond the box's reach are more than a
        // voxel outside it, too far to come in.
        let (lo, hi) = b.centers();
        let mut rigid = VOXEL;
        each((lo - 1, hi + 1), within, |k| {
            rigid = rigid.min(margin(to_local * (center(k) - b.center), b.half));
        });
        Self {
            rigid: (rigid - 2.0 * error).max(0.0),
            reach,
            within,
        }
    }

    /// Whether a part that just moved `from` to `to` may be held next
    /// time: whether that move fits in the most slack any part has (a
    /// voxel). One moving faster, whirled by a tornado or thrown, is
    /// marked again at each move anyway, and working its slack out would
    /// cost about what marking it does.
    pub(super) fn worth(from: &Blocker, to: &Blocker) -> bool {
        let most = Slack {
            rigid: VOXEL,
            reach: reach(to),
            within: (IVec3::ZERO, IVec3::ZERO),
        };
        most.fits(from, to)
    }

    /// Whether `to` marks exactly what `from`, the pose this slack is
    /// `from`'s, marks in the voxels `within`.
    pub(super) fn holds(&self, from: &Blocker, to: &Blocker, within: Span) -> bool {
        self.within == within && self.fits(from, to)
    }

    /// Whether `from` to `to` is within this slack, whatever voxels it
    /// was worked out over.
    fn fits(&self, from: &Blocker, to: &Blocker) -> bool {
        if from.half != to.half {
            return false;
        }
        // A point `reach` from the center moves by at most the center's
        // move and the turn's.
        let turned = to.axes - from.axes;
        let turn = (turned.x_axis.length_squared()
            + turned.y_axis.length_squared()
            + turned.z_axis.length_squared())
        .sqrt();
        let moved = (to.center - from.center).length() + turn * self.reach;
        moved < self.rigid
    }
}

/// How far from `b`'s center the points a slack watches can be.
fn reach(b: &Blocker) -> f32 {
    (b.max - b.center).length() + 2.0 * VOXEL * 3f32.sqrt()
}

/// How far a point at `local` (in a box's space) is from crossing the
/// faces of a box `half` its size each way, in or out, along the box's
/// axes.
fn margin(local: Vec3, half: Vec3) -> f32 {
    let inside = half - local.abs();
    if inside.cmpge(Vec3::ZERO).all() {
        inside.min_element()
    } else {
        (-inside).max_element()
    }
}

/// Each voxel of `span` (both ends included) among those `within`.
fn each(span: Span, within: Span, mut f: impl FnMut(IVec3)) {
    let Some((lo, hi)) = clip(span, within) else {
        return;
    };
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                f(IVec3::new(x, y, z));
            }
        }
    }
}
