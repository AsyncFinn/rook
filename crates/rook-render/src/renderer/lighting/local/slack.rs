//! How far a blocker may move before anything it marks in the voxels could
//! change, so a part that barely moves (one physics keeps nudging, or one
//! creeping a little each frame) isn't marked again every frame. This is a
//! physics broadphase's fattened bounds (Box2D's `b2_aabbMargin`): a body
//! is inserted again only once it leaves them. Here the bounds are exact.
//! The voxels are a lattice fixed in the world, so what a blocker marks
//! depends on its pose alone, and a blocker kept at the pose it was marked
//! at while it moves within its slack marks exactly what it would where it
//! is now.

use glam::{IVec3, Vec3};

use super::VOXEL;
use super::blockers::{Blocker, FULL, SAMPLE_REACH, center, overlap, samples};

/// How far a blocker may move from the pose it was marked at, in studs,
/// two ways; nothing at all by default.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Slack {
    /// The most any point within `reach` of its center may move: the
    /// voxel centers it holds or nearly holds, and for a rotated box the
    /// points sampled across the voxels it fills part of.
    rigid: f32,
    reach: f32,
    /// For a box aligned with the world, the most its faces may move: its
    /// fills are worked out from them.
    faces: f32,
}

/// Around a box's faces, how far an aligned box's slack looks for faces
/// that could change a voxel's fill; it may move no further.
const WINDOW: f32 = 0.25;

impl Slack {
    /// The slack of `b`, at the pose it is marked at.
    pub(super) fn of(b: &Blocker) -> Self {
        let reach =
            (b.max - b.center).max(b.true_max - b.center).length() + 2.0 * VOXEL * 3f32.sqrt();
        // Both poses' sums in f32 may each be off by about this much.
        let error = 4.0 * f32::EPSILON * (b.center.length() + 2.0 * reach) + 1e-5;
        let to_local = b.axes.transpose();
        // Voxel centers: those one beyond the box's reach are more than a
        // voxel outside it, too far to come in.
        let (lo, hi) = b.centers();
        let mut rigid = VOXEL;
        each(lo - 1, hi + 1, |k| {
            rigid = rigid.min(margin(to_local * (center(k) - b.center), b.half));
        });
        let mut faces = f32::INFINITY;
        if b.aligned() {
            faces = faces_slack(b, error);
        } else {
            // The sampled points of voxels past those it touches are at
            // least half a stud outside it.
            rigid = rigid.min(0.5);
            let points = samples(to_local);
            let (lo, hi) = b.touched();
            each(lo, hi, |k| {
                let local = to_local * (center(k) - b.center);
                let wholly = local.abs();
                let m = if (wholly - SAMPLE_REACH).cmpgt(b.true_half).any() {
                    (wholly - SAMPLE_REACH - b.true_half).max_element()
                } else if (wholly + SAMPLE_REACH).cmple(b.true_half).all() {
                    (b.true_half - wholly - SAMPLE_REACH).min_element()
                } else {
                    (points.iter())
                        .fold(f32::INFINITY, |m, &o| m.min(margin(local + o, b.true_half)))
                };
                rigid = rigid.min(m);
            });
        }
        Self {
            rigid: (rigid - 2.0 * error).max(0.0),
            reach,
            faces: faces.max(0.0),
        }
    }

    /// Whether `to` marks exactly what `from`, the pose this slack is
    /// `from`'s, marks.
    pub(super) fn holds(&self, from: &Blocker, to: &Blocker) -> bool {
        if from.half != to.half || from.true_half != to.true_half || from.aligned() != to.aligned()
        {
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
        if moved >= self.rigid {
            return false;
        }
        !from.aligned() || {
            let low = (to.true_min - from.true_min).abs();
            let high = (to.true_max - from.true_max).abs();
            low.max(high).max_element() < self.faces
        }
    }
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

/// An aligned box's fills are each voxel's overlap with it, a byte in
/// 255ths: how far its faces may move before one rounds differently.
fn faces_slack(b: &Blocker, error: f32) -> f32 {
    let mut slack = WINDOW;
    // Voxels a voxel or more past its faces stay empty while it moves
    // less than the window.
    let (lo, hi) = b.touched();
    each(lo - 1, hi + 1, |k| {
        let at = center(k);
        let over = overlap(b, at);
        let filled = over.x * over.y * over.z / (VOXEL * VOXEL * VOXEL) * f32::from(FULL);
        // How far the fill is from rounding to another byte.
        let gap = 0.5 - (filled - filled.round()).abs() - 1e-3;
        // Each overlap changes by at most twice the faces' move, and only
        // along axes where a face is within the window of the voxel.
        let near = |face: f32, axis: usize| {
            face > at[axis] - VOXEL * 0.5 - WINDOW && face < at[axis] + VOXEL * 0.5 + WINDOW
        };
        let grown = (over + 2.0 * WINDOW).min(Vec3::splat(VOXEL));
        let mut rate = 0.0;
        for axis in 0..3 {
            if near(b.true_min[axis], axis) || near(b.true_max[axis], axis) {
                let rest = grown[(axis + 1) % 3] * grown[(axis + 2) % 3];
                rate += 2.0 * rest / (VOXEL * VOXEL * VOXEL) * f32::from(FULL);
            }
        }
        if rate > 0.0 {
            slack = slack.min(gap / rate - 2.0 * error);
        }
    });
    slack
}

/// Each voxel from `lo` to `hi`, both included.
fn each(lo: IVec3, hi: IVec3, mut f: impl FnMut(IVec3)) {
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                f(IVec3::new(x, y, z));
            }
        }
    }
}
