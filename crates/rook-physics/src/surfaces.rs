//! Legacy surface joints: `MakeJoints`.
//!
//! Rules observed in Studio 0.741 (MakeJoints still works there):
//!
//! - Two faces join when they face each other within 0.01 studs (gap or
//!   overlap), their edges line up (within about half a degree) and they
//!   overlap with some area (touching along an edge isn't enough).
//! - A Hinge, Motor or SteppingMotor surface on either face makes a Rotate,
//!   RotateV or RotateP; the calling part's wins. Otherwise Weld on either
//!   side makes a Weld, then Glue a Glue, then Studs, Inlet or Universal on
//!   both sides a Snap, except Studs to Studs and Inlet to Inlet.
//! - The joint is named after its class, parented to Part0 and framed at
//!   Part0's face centre with Z along the face's outward normal. Part0 is
//!   the part whose rotating surface decided the type, otherwise the
//!   calling part. Parts already joined aren't joined again.
//! - Only parts in Workspace join; they may join parts anywhere in it.
//!
//! Every part is treated as its box, except balls and cylinders, which
//! have no surfaces here (Approximation).

use std::collections::HashMap;

use glam::{Mat3, Vec3};
use rook_dom::{Dom, DomError, InstanceId};
use rook_types::{CFrame, PartShape, Value};

use crate::joints::part_joints;
use crate::shape::{part_shape, workspace_parts};

/// The faces in the order +X, +Y, +Z, -X, -Y, -Z, with each one's surface
/// property and joint frame (columns: X, Y, Z = outward normal).
const FACES: [(&str, [[f32; 3]; 3]); 6] = [
    ("RightSurface", [[0., 0., -1.], [0., 1., 0.], [1., 0., 0.]]),
    ("TopSurface", [[-1., 0., 0.], [0., 0., 1.], [0., 1., 0.]]),
    ("BackSurface", [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]),
    ("LeftSurface", [[0., 0., 1.], [0., 1., 0.], [-1., 0., 0.]]),
    ("BottomSurface", [[1., 0., 0.], [0., 0., 1.], [0., -1., 0.]]),
    ("FrontSurface", [[-1., 0., 0.], [0., 1., 0.], [0., 0., -1.]]),
];

/// Faces closer than this (either way) touch.
const DISTANCE: f32 = 0.01;
/// Edges line up when the sine of the angle between them is below this.
const ALIGNMENT: f32 = 0.01;

/// `Enum.SurfaceType`, the values that matter here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Surface {
    Glue,
    Weld,
    Studs,
    Inlet,
    Universal,
    Hinge,
    Motor,
    SteppingMotor,
    Other,
}

impl Surface {
    fn of(dom: &Dom, part: InstanceId, property: &str) -> Self {
        match dom.property_by_name(part, property) {
            Some(Value::Enum(n)) => match n {
                1 => Self::Glue,
                2 => Self::Weld,
                3 => Self::Studs,
                4 => Self::Inlet,
                5 => Self::Universal,
                6 => Self::Hinge,
                7 => Self::Motor,
                8 => Self::SteppingMotor,
                _ => Self::Other,
            },
            _ => Self::Other,
        }
    }

    fn rotating_joint(self) -> Option<&'static str> {
        match self {
            Self::Hinge => Some("Rotate"),
            Self::Motor => Some("RotateV"),
            Self::SteppingMotor => Some("RotateP"),
            _ => None,
        }
    }

    fn connects(self) -> bool {
        matches!(self, Self::Studs | Self::Inlet | Self::Universal)
    }
}

/// The joint class two touching surfaces make, and whether the other
/// part's surface decided it (making that part Part0).
fn joint_class(mine: Surface, theirs: Surface) -> Option<(&'static str, bool)> {
    if let Some(class) = mine.rotating_joint() {
        return Some((class, false));
    }
    if let Some(class) = theirs.rotating_joint() {
        return Some((class, true));
    }
    let either = |s| mine == s || theirs == s;
    if either(Surface::Weld) {
        Some(("Weld", false))
    } else if either(Surface::Glue) {
        Some(("Glue", false))
    } else if mine.connects() && theirs.connects() && (mine != theirs || mine == Surface::Universal)
    {
        Some(("Snap", false))
    } else {
        None
    }
}

/// A part as a box, for face tests.
struct Box {
    id: InstanceId,
    cf: CFrame,
    half: Vec3,
    min: Vec3,
    max: Vec3,
}

impl Box {
    fn of(dom: &Dom, id: InstanceId) -> Option<Self> {
        if matches!(part_shape(dom, id), PartShape::Ball | PartShape::Cylinder) {
            return None;
        }
        let (Some(Value::CFrame(cf)), Some(Value::Vector3(size))) = (
            dom.property_by_name(id, "CFrame"),
            dom.property_by_name(id, "Size"),
        ) else {
            return None;
        };
        let reach = cf.box_reach(size.0) + Vec3::splat(DISTANCE);
        Some(Self {
            id,
            cf,
            half: size.0 * 0.5,
            min: cf.position - reach,
            max: cf.position + reach,
        })
    }

    fn near(&self, other: &Self) -> bool {
        self.min.cmple(other.max).all() && other.min.cmple(self.max).all()
    }

    /// Face `f`'s outward normal, its centre and its two tangent axes with
    /// their half extents, in world space.
    fn face(&self, f: usize) -> (Vec3, Vec3, [(Vec3, f32); 2]) {
        let axis = f % 3;
        let sign = if f < 3 { 1.0 } else { -1.0 };
        let r = self.cf.rotation;
        let normal = r.col(axis) * sign;
        let centre = self.cf.position + normal * self.half[axis];
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        (
            normal,
            centre,
            [(r.col(a), self.half[a]), (r.col(b), self.half[b])],
        )
    }
}

/// Whether face `fa` of `a` and face `fb` of `b` touch.
fn faces_touch(a: &Box, fa: usize, b: &Box, fb: usize) -> bool {
    let (na, ca, ta) = a.face(fa);
    let (nb, cb, tb) = b.face(fb);
    if na.dot(nb) > -(1.0 - ALIGNMENT * ALIGNMENT / 2.0) {
        return false;
    }
    let d = cb - ca;
    if d.dot(na).abs() >= DISTANCE {
        return false;
    }
    // Each of a's tangent axes must run along one of b's; then the faces
    // overlap if their extents overlap along both.
    ta.iter().all(|&(axis, half)| {
        let Some(&(_, other_half)) = tb.iter().find(|(t, _)| axis.cross(*t).length() < ALIGNMENT)
        else {
            return false;
        };
        let centre = d.dot(axis);
        let overlap = half.min(centre + other_half) - (-half).max(centre - other_half);
        overlap > 1e-4
    })
}

fn face_frame(f: usize) -> Mat3 {
    let [x, y, z] = FACES[f].1;
    Mat3::from_cols(Vec3::from(x), Vec3::from(y), Vec3::from(z))
}

/// `MakeJoints` for each of `parts`, in order.
pub fn make_joints(dom: &mut Dom, parts: &[InstanceId]) -> Result<(), DomError> {
    let world: Vec<Box> = workspace_parts(dom)
        .into_iter()
        .filter_map(|id| Box::of(dom, id))
        .collect();
    let index: HashMap<InstanceId, usize> =
        world.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    // Each caller's place in `parts`; a part listed twice calls once.
    let mut rank = HashMap::new();
    for (r, p) in parts.iter().enumerate() {
        if let Some(&i) = index.get(p) {
            rank.entry(i).or_insert(r);
        }
    }
    // The neighbours each caller tries, the earlier caller of a pair first.
    // A few callers (the usual script call) check the world directly;
    // many (a model, a whole place) sweep it once.
    let pairs = if rank.len() <= 16 {
        let mut pairs = Vec::new();
        for &i in rank.keys() {
            pairs.extend(
                (0..world.len())
                    .filter(|&j| j != i && world[i].near(&world[j]))
                    .map(|j| (i.min(j), i.max(j))),
            );
        }
        pairs.sort_unstable();
        pairs.dedup();
        pairs
    } else {
        near_pairs(&world)
    };
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); world.len()];
    for (a, b) in pairs {
        match (rank.get(&a), rank.get(&b)) {
            (Some(ra), Some(rb)) if rb < ra => neighbours[b].push(a),
            (Some(_), _) => neighbours[a].push(b),
            (None, Some(_)) => neighbours[b].push(a),
            (None, None) => {}
        }
    }
    let mut callers: Vec<(usize, usize)> = rank.into_iter().map(|(i, r)| (r, i)).collect();
    callers.sort_unstable();
    for (_, i) in callers {
        let me = &world[i];
        neighbours[i].sort_unstable();
        for &j in &neighbours[i] {
            let other = &world[j];
            if part_joints(dom, me.id)
                .iter()
                .any(|jt| jt.other(me.id) == other.id)
            {
                continue;
            }
            if let Some((fa, fb)) = touching_faces(me, other) {
                join(dom, me, fa, other, fb)?;
            }
        }
    }
    Ok(())
}

/// Pairs of boxes whose padded bounds overlap (sweep and prune along X).
fn near_pairs(boxes: &[Box]) -> Vec<(usize, usize)> {
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_unstable_by(|a, b| boxes[*a].min.x.total_cmp(&boxes[*b].min.x));
    let mut active: Vec<usize> = Vec::new();
    let mut pairs = Vec::new();
    for i in order {
        active.retain(|&j| boxes[j].max.x >= boxes[i].min.x);
        pairs.extend(
            active
                .iter()
                .filter(|&&j| boxes[i].near(&boxes[j]))
                .map(|&j| (j.min(i), j.max(i))),
        );
        active.push(i);
    }
    pairs
}

/// Whether the place joins touching parts by their surfaces when it loads.
/// `Workspace.ExplicitAutoJoints` (hidden; true in new places) turned this
/// off; places saved before it existed don't store it, and their parts
/// were joined at load, which is why they save almost no joints
/// (Inference: a 2008 place stores one Snap in 1,678 parts).
pub fn joins_on_load(dom: &Dom) -> bool {
    let Some(ws) = dom.find_service("Workspace") else {
        return false;
    };
    match dom.property_id(ws, "ExplicitAutoJoints") {
        Some(p) => match dom.get(ws).and_then(|i| i.properties().get(&p)) {
            Some(v) => *v == Value::Bool(false),
            None => true,
        },
        None => false,
    }
}

fn touching_faces(a: &Box, b: &Box) -> Option<(usize, usize)> {
    (0..6)
        .flat_map(|fa| (0..6).map(move |fb| (fa, fb)))
        .find(|&(fa, fb)| faces_touch(a, fa, b, fb))
}

fn join(dom: &mut Dom, me: &Box, fa: usize, other: &Box, fb: usize) -> Result<(), DomError> {
    let mine = Surface::of(dom, me.id, FACES[fa].0);
    let theirs = Surface::of(dom, other.id, FACES[fb].0);
    let Some((class, swapped)) = joint_class(mine, theirs) else {
        return Ok(());
    };
    let (p0, f0, p1) = if swapped {
        (other, fb, me)
    } else {
        (me, fa, other)
    };
    let frame = face_frame(f0);
    let c0 = CFrame {
        position: frame.z_axis * p0.half[f0 % 3],
        rotation: frame,
    };
    let c1 = p1.cf.inverse() * p0.cf * c0;
    let joint = dom.create_by_name(class)?;
    let set = |dom: &mut Dom, name, v| dom.set_property_by_name(joint, name, v);
    set(dom, "Part0", Value::Ref(Some(p0.id.to_ref())))?;
    set(dom, "Part1", Value::Ref(Some(p1.id.to_ref())))?;
    set(dom, "C0", Value::CFrame(c0))?;
    set(dom, "C1", Value::CFrame(c1))?;
    dom.set_parent(joint, Some(p0.id))
}
