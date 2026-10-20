//! Joints between parts, read straight from the DOM.
//!
//! Everything here works on the DOM alone, without the Rapier world, so
//! scripts can query and move assemblies synchronously (Rook's Luau API
//! calls it), and [`crate::Physics`] builds its bodies from the same rules.
//!
//! Facts below were observed in Studio 0.741 unless marked otherwise.

pub(crate) mod constraints;
pub(crate) mod drives;
pub(crate) mod movers;
pub mod surfaces;

use rook_dom::{Dom, DomError, InstanceId};
use rook_types::{CFrame, Value};

use crate::shape::is_part;

/// How a joint relates its two parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointKind {
    /// `Part0 * C0 = Part1 * C1`: Weld, ManualWeld, Snap, Glue, ManualGlue.
    Fixed,
    /// `Part0 * C0 * Rz(CurrentAngle) = Part1 * C1`. CurrentAngle moves
    /// toward DesiredAngle by MaxVelocity radians per 60 Hz frame.
    Motor,
    /// A Motor with `Transform` applied before the angle.
    Motor6D,
    /// Keeps the parts' relative pose from when it became active.
    WeldConstraint,
    /// Rotate, RotateP and RotateV: rotation about C0's Z axis (the
    /// surface normal for joints MakeJoints creates). Rotate turns freely;
    /// RotateV and RotateP are driven ([`hinge_drive`]).
    Hinge,
}

impl JointKind {
    pub fn of(class: &str) -> Option<Self> {
        Some(match class {
            "Weld" | "ManualWeld" | "Snap" | "Glue" | "ManualGlue" => Self::Fixed,
            "Motor" => Self::Motor,
            "Motor6D" => Self::Motor6D,
            "WeldConstraint" => Self::WeldConstraint,
            "Rotate" | "RotateP" | "RotateV" => Self::Hinge,
            _ => return None,
        })
    }

    /// Whether the joint makes its parts one assembly.
    pub fn is_rigid(self) -> bool {
        self != Self::Hinge
    }
}

/// An active joint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Joint {
    pub id: InstanceId,
    pub kind: JointKind,
    pub part0: InstanceId,
    pub part1: InstanceId,
}

impl Joint {
    /// The part on the other side from `part`.
    pub fn other(&self, part: InstanceId) -> InstanceId {
        if part == self.part0 {
            self.part1
        } else {
            self.part0
        }
    }
}

/// A class Rook treats as a joint, active or not.
pub fn is_joint(dom: &Dom, id: InstanceId) -> bool {
    JointKind::of(dom.class_name(id)).is_some()
}

/// `Part0` and `Part1`, whether or not the joint is active.
pub fn endpoints(dom: &Dom, id: InstanceId) -> [Option<InstanceId>; 2] {
    [dom.ref_property(id, "Part0"), dom.ref_property(id, "Part1")]
}

/// The joint `id` if it's active: enabled, joining two different parts
/// that are both in Workspace, and itself in Workspace. JointInstances in
/// JointsService are active too; WeldConstraints there aren't.
pub fn active_joint(dom: &Dom, id: InstanceId) -> Option<Joint> {
    let kind = JointKind::of(dom.class_name(id))?;
    if dom.is_destroyed(id) || dom.bool_property(id, "Enabled") == Some(false) {
        return None;
    }
    let [Some(part0), Some(part1)] = endpoints(dom, id) else {
        return None;
    };
    let ws = dom.find_service("Workspace")?;
    let in_ws = |i| dom.is_ancestor_of(ws, i);
    if part0 == part1
        || !in_ws(part0)
        || !in_ws(part1)
        || !is_part(dom, part0)
        || !is_part(dom, part1)
    {
        return None;
    }
    let placed = in_ws(id)
        || (kind != JointKind::WeldConstraint
            && dom
                .find_service("JointsService")
                .is_some_and(|js| dom.is_ancestor_of(js, id)));
    placed.then_some(Joint {
        id,
        kind,
        part0,
        part1,
    })
}

/// Active joints attached to `part`, in the order they were attached.
pub fn part_joints(dom: &Dom, part: InstanceId) -> Vec<Joint> {
    let mut out: Vec<Joint> = Vec::new();
    for &(holder, _) in dom.referrers(part) {
        if out.iter().any(|j| j.id == holder) {
            continue;
        }
        if let Some(j) = active_joint(dom, holder) {
            out.push(j);
        }
    }
    out
}

/// What sits between `C0` and `C1`: `Part0 * C0 * middle = Part1 * C1`.
fn middle(dom: &Dom, j: &Joint) -> CFrame {
    let angle = || {
        CFrame::angles(
            0.0,
            0.0,
            dom.float_property(j.id, "CurrentAngle").unwrap_or(0.0),
        )
    };
    match j.kind {
        JointKind::Motor => angle(),
        JointKind::Motor6D => {
            dom.cframe_property(j.id, "Transform")
                .unwrap_or(CFrame::IDENTITY)
                * angle()
        }
        _ => CFrame::IDENTITY,
    }
}

/// Part1's pose relative to Part0 as the joint holds it.
pub fn offset(dom: &Dom, j: &Joint) -> CFrame {
    if j.kind == JointKind::WeldConstraint {
        // Inference: the captured offset is the parts' relative pose, which
        // stays true while they move together; setting one part's Position
        // re-captures it, which reading it live also does.
        return dom
            .cframe_property(j.part0, "CFrame")
            .unwrap_or(CFrame::IDENTITY)
            .inverse()
            * dom
                .cframe_property(j.part1, "CFrame")
                .unwrap_or(CFrame::IDENTITY);
    }
    dom.cframe_property(j.id, "C0").unwrap_or(CFrame::IDENTITY)
        * middle(dom, j)
        * dom
            .cframe_property(j.id, "C1")
            .unwrap_or(CFrame::IDENTITY)
            .inverse()
}

/// The hinge frames on Part0 and Part1 (their Z axes are the hinge axis).
pub fn hinge_frames(dom: &Dom, j: &Joint) -> (CFrame, CFrame) {
    (
        dom.cframe_property(j.id, "C0").unwrap_or(CFrame::IDENTITY),
        dom.cframe_property(j.id, "C1").unwrap_or(CFrame::IDENTITY),
    )
}

/// After `part` moved on its own (its Position or Orientation was set),
/// rewrite its side of each attached joint so the joint holds it where it
/// now is: Roblox moves only that part and updates C0 or C1.
pub fn reseat(dom: &mut Dom, part: InstanceId) -> Result<(), DomError> {
    for j in part_joints(dom, part) {
        if j.kind == JointKind::WeldConstraint {
            continue;
        }
        let here = dom
            .cframe_property(part, "CFrame")
            .unwrap_or(CFrame::IDENTITY)
            .inverse();
        let m = middle(dom, &j);
        let (name, value) = if part == j.part1 {
            let c0 = dom.cframe_property(j.id, "C0").unwrap_or(CFrame::IDENTITY);
            (
                "C1",
                here * dom
                    .cframe_property(j.part0, "CFrame")
                    .unwrap_or(CFrame::IDENTITY)
                    * c0
                    * m,
            )
        } else {
            let c1 = dom.cframe_property(j.id, "C1").unwrap_or(CFrame::IDENTITY);
            (
                "C0",
                here * dom
                    .cframe_property(j.part1, "CFrame")
                    .unwrap_or(CFrame::IDENTITY)
                    * c1
                    * m.inverse(),
            )
        };
        dom.set_property_by_name(j.id, name, Value::CFrame(value))?;
    }
    Ok(())
}
