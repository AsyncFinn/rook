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
//! - A wedge's slope is a face; its box's top and front aren't. A wedge
//!   calling MakeJoints joins what's on its slope only through its own
//!   Weld surfaces, TopSurface before FrontSurface, framed at that box
//!   face. A part calling it on a slope sees the wedge's FrontSurface
//!   there. Rotating surfaces on either side of a slope join nothing.
//! - When both parts of a pair call (a model's MakeJoints), the earlier
//!   tries first and the later tries again if that made no joint, so a
//!   slope joins if either side would (Part0 is then the later caller).
//!
//! Every other part is treated as its box (a wedge's side triangles as
//! rectangles). Balls, cylinders and corner wedges are boxes too: in
//! Studio a ball's Hinge makes a Rotate and its Studs a Snap, and a
//! cylinder's caps and curved side join, all framed at the box face.

use rustc_hash::FxHashMap as HashMap;

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

const TOP: usize = 1;
const FRONT: usize = 5;
/// A wedge's slope, numbered after the six box faces.
const SLOPE: usize = 6;
const BOX_FACES: &[usize] = &[0, 1, 2, 3, 4, 5];
const WEDGE_FACES: &[usize] = &[0, 2, 3, 4, SLOPE];

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
    faces: &'static [usize],
}

impl Box {
    fn of(dom: &Dom, id: InstanceId) -> Option<Self> {
        let faces = match part_shape(dom, id) {
            PartShape::Wedge => WEDGE_FACES,
            PartShape::Block | PartShape::CornerWedge | PartShape::Ball | PartShape::Cylinder => {
                BOX_FACES
            }
        };
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
            faces,
        })
    }

    fn has_slope(&self) -> bool {
        self.faces.contains(&SLOPE)
    }

    fn near(&self, other: &Self) -> bool {
        self.min.cmple(other.max).all() && other.min.cmple(self.max).all()
    }

    /// Face `f`'s outward normal, its centre and its two tangent axes with
    /// their half extents, in world space.
    fn face(&self, f: usize) -> (Vec3, Vec3, [(Vec3, f32); 2]) {
        let r = self.cf.rotation;
        if f == SLOPE {
            // From the bottom front edge up to the top back one, through
            // the centre; it faces up and forward.
            let h = self.half;
            let up = Vec3::new(0.0, h.y, h.z);
            return (
                r * Vec3::new(0.0, h.z, -h.y).normalize(),
                self.cf.position,
                [(r.col(0), h.x), (r * up.normalize(), up.length())],
            );
        }
        let axis = f % 3;
        let sign = if f < 3 { 1.0 } else { -1.0 };
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
    let mut rank = HashMap::default();
    for (r, p) in parts.iter().enumerate() {
        if let Some(&i) = index.get(p) {
            rank.entry(i).or_insert(r);
        }
    }
    // The neighbours each caller tries. When both parts of a pair call, the
    // earlier tries first and the later tries again if that made no joint,
    // as in Studio: a wedge's slope joins differently from each side.
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
            (Some(ra), Some(rb)) => {
                let (first, second) = if ra <= rb { (a, b) } else { (b, a) };
                neighbours[first].push(second);
                // Only a slope joins differently from the other side.
                if world[a].has_slope() || world[b].has_slope() {
                    neighbours[second].push(first);
                }
            }
            (Some(_), None) => neighbours[a].push(b),
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
    a.faces
        .iter()
        .flat_map(|&fa| b.faces.iter().map(move |&fb| (fa, fb)))
        .find(|&(fa, fb)| faces_touch(a, fa, b, fb))
}

/// The joint class `me`'s face `fa` makes with `other`'s face `fb`, and
/// Part0 with the face its joint is framed at, then Part1.
fn joint_between<'a>(
    dom: &Dom,
    me: &'a Box,
    fa: usize,
    other: &'a Box,
    fb: usize,
) -> Option<(&'static str, &'a Box, usize, &'a Box)> {
    if fa == SLOPE {
        let f = [TOP, FRONT]
            .into_iter()
            .find(|&f| Surface::of(dom, me.id, FACES[f].0) == Surface::Weld)?;
        return Some(("Weld", me, f, other));
    }
    let mine = Surface::of(dom, me.id, FACES[fa].0);
    let theirs = Surface::of(dom, other.id, FACES[if fb == SLOPE { FRONT } else { fb }].0);
    if fb == SLOPE && (mine.rotating_joint().is_some() || theirs.rotating_joint().is_some()) {
        return None;
    }
    let (class, swapped) = joint_class(mine, theirs)?;
    Some(if swapped {
        (class, other, fb, me)
    } else {
        (class, me, fa, other)
    })
}

fn join(dom: &mut Dom, me: &Box, fa: usize, other: &Box, fb: usize) -> Result<(), DomError> {
    let Some((class, p0, f0, p1)) = joint_between(dom, me, fa, other, fb) else {
        return Ok(());
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
