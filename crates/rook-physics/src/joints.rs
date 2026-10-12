//! Joints between parts, read straight from the DOM.
//!
//! Everything here works on the DOM alone, without the Rapier world, so
//! scripts can query and move assemblies synchronously (Rook's Luau API
//! calls it), and [`crate::Physics`] builds its bodies from the same rules.
//!
//! Facts below were observed in Studio 0.741 unless marked otherwise.

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
    /// Rotate, RotateP and RotateV: free rotation about C0's Z axis (the
    /// surface normal for joints MakeJoints creates). RotateP and RotateV
    /// don't drive their rotation yet (M3 step 4).
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
    if dom.is_destroyed(id) || dom.property_by_name(id, "Enabled") == Some(Value::Bool(false)) {
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

fn cframe(dom: &Dom, id: InstanceId, name: &str) -> CFrame {
    match dom.property_by_name(id, name) {
        Some(Value::CFrame(c)) => c,
        _ => CFrame::IDENTITY,
    }
}

fn float(dom: &Dom, id: InstanceId, name: &str) -> f32 {
    match dom.property_by_name(id, name) {
        Some(Value::Float32(f)) => f,
        _ => 0.0,
    }
}

/// What sits between `C0` and `C1`: `Part0 * C0 * middle = Part1 * C1`.
fn middle(dom: &Dom, j: &Joint) -> CFrame {
    let angle = || CFrame::angles(0.0, 0.0, float(dom, j.id, "CurrentAngle"));
    match j.kind {
        JointKind::Motor => angle(),
        JointKind::Motor6D => cframe(dom, j.id, "Transform") * angle(),
        _ => CFrame::IDENTITY,
    }
}

/// Part1's pose relative to Part0 as the joint holds it.
pub fn offset(dom: &Dom, j: &Joint) -> CFrame {
    if j.kind == JointKind::WeldConstraint {
        // Inference: the captured offset is the parts' relative pose, which
        // stays true while they move together; setting one part's Position
        // re-captures it, which reading it live also does.
        return cframe(dom, j.part0, "CFrame").inverse() * cframe(dom, j.part1, "CFrame");
    }
    cframe(dom, j.id, "C0") * middle(dom, j) * cframe(dom, j.id, "C1").inverse()
}

/// The hinge frames on Part0 and Part1 (their Z axes are the hinge axis).
pub fn hinge_frames(dom: &Dom, j: &Joint) -> (CFrame, CFrame) {
    (cframe(dom, j.id, "C0"), cframe(dom, j.id, "C1"))
}

/// After `part` moved on its own (its Position or Orientation was set),
/// rewrite its side of each attached joint so the joint holds it where it
/// now is: Roblox moves only that part and updates C0 or C1.
pub fn reseat(dom: &mut Dom, part: InstanceId) -> Result<(), DomError> {
    for j in part_joints(dom, part) {
        if j.kind == JointKind::WeldConstraint {
            continue;
        }
        let here = cframe(dom, part, "CFrame").inverse();
        let m = middle(dom, &j);
        let (name, value) = if part == j.part1 {
            let c0 = cframe(dom, j.id, "C0");
            ("C1", here * cframe(dom, j.part0, "CFrame") * c0 * m)
        } else {
            let c1 = cframe(dom, j.id, "C1");
            (
                "C0",
                here * cframe(dom, j.part1, "CFrame") * c1 * m.inverse(),
            )
        };
        dom.set_property_by_name(j.id, name, Value::CFrame(value))?;
    }
    Ok(())
}

/// Advance a Motor's CurrentAngle toward DesiredAngle over `dt` seconds.
/// Returns whether it moved. MaxVelocity is radians per 60 Hz frame
/// (observed: 0.1 per frame).
pub(crate) fn step_motor(dom: &mut Dom, j: &Joint, dt: f64) -> bool {
    if !matches!(j.kind, JointKind::Motor | JointKind::Motor6D) {
        return false;
    }
    let current = float(dom, j.id, "CurrentAngle");
    let desired = float(dom, j.id, "DesiredAngle");
    if current == desired {
        return false;
    }
    let max = float(dom, j.id, "MaxVelocity").abs() * (dt * 60.0) as f32;
    let next = current + (desired - current).clamp(-max, max);
    let _ = dom.set_property_by_name(j.id, "CurrentAngle", Value::Float32(next));
    next != current
}
