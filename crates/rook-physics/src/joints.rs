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

use crate::changes::read::{cframe, float};
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

/// `Enum.InputType`: how a motor surface drives its hinge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceInput {
    /// Holds still (observed: an impulse doesn't turn it).
    NoInput,
    /// ParamB.
    Constant,
    /// `ParamA · sin(ParamB · t)`, t counted from when the input became
    /// Sin (observed).
    Sin,
}

/// What drives a RotateV or RotateP: the inputs of Part0's surface that
/// the joint sits on, the face C0's Z axis points out of (Inference: for
/// joints MakeJoints creates, that's the Motor or SteppingMotor surface,
/// as Part0 is the part whose surface decided the joint). Changes apply
/// live (observed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HingeInput {
    /// RotateP (a stepping motor, driven to an angle) rather than RotateV
    /// (driven at a speed).
    pub stepping: bool,
    pub input: SurfaceInput,
    pub param_a: f32,
    pub param_b: f32,
    /// RotateV's torque limit per unit of input ([`rotate_v_strength`]).
    pub strength: f32,
}

impl HingeInput {
    /// The input's value `t` seconds after it became Sin: RotateV's speed
    /// in units of [`ROTATE_V_SPEED`], or RotateP's angle in radians.
    /// `None` holds the hinge where it is.
    pub fn value(&self, t: f64) -> Option<f32> {
        match self.input {
            SurfaceInput::NoInput => None,
            SurfaceInput::Constant => Some(self.param_b),
            SurfaceInput::Sin => Some(self.param_a * (f64::from(self.param_b) * t).sin() as f32),
        }
    }
}

/// RotateV turns at this many radians per second per unit of input
/// (observed: ParamB 0.1, 0.5, 1 and 2 turn at 3.1, 15.5, 31 and 62).
pub const ROTATE_V_SPEED: f32 = 31.0;

/// RotateV's torque limit per unit of input per unit of [`rotate_v_strength`]'s
/// geometric inertia (observed with a 1 and a 2 stud cube: 8.3e5 and
/// 2.7e7 at ParamB 10).
pub const ROTATE_V_TORQUE: f32 = 5.0e5;

/// RotateV's torque limit per unit of input. A rotor gains speed steadily
/// at this torque until it turns at its input's speed, so a heavy one
/// spins up over seconds (observed). The limit comes from the smaller of
/// the two parts, by size alone: the same with either part's density
/// changed, and the same whichever part is the small one (observed).
///
/// Inference: modelled as the part's moment of inertia about the hinge
/// axis at unit density. That is exact for 1 and 2 stud cubes and for
/// plates up to 1 stud thick (the limit grows with thickness, observed
/// from 0.2 to 2 studs), but Roblox's limit grows in steps with size:
/// cubes from 0.5 to 8 studs measured from a quarter to three and a half
/// times this.
pub fn rotate_v_strength(dom: &Dom, j: &Joint) -> f32 {
    let inertia = |part, frame: &str| {
        let size = crate::changes::read::vector(dom, part, "Size");
        let axis = cframe(dom, j.id, frame).rotation.z_axis;
        let volume = size.x * size.y * size.z;
        let sq = size * size;
        let diagonal = glam::Vec3::new(sq.y + sq.z, sq.x + sq.z, sq.x + sq.y) * (volume / 12.0);
        (axis * axis).dot(diagonal)
    };
    ROTATE_V_TORQUE * inertia(j.part0, "C0").min(inertia(j.part1, "C1"))
}

/// RotateP moves its angle this many radians per 240 Hz step toward the
/// input (observed: 2.4 rad/s, in 0.01 rad steps, whatever the distance).
pub const ROTATE_P_STEP: f32 = 0.01;

/// The input driving `j` if it's a RotateV or RotateP.
pub fn hinge_input(dom: &Dom, j: &Joint) -> Option<HingeInput> {
    let stepping = match dom.class_name(j.id) {
        "RotateV" => false,
        "RotateP" => true,
        _ => return None,
    };
    let z = cframe(dom, j.id, "C0").rotation.z_axis;
    let a = z.abs();
    let face = if a.x >= a.y && a.x >= a.z {
        if z.x > 0.0 { "Right" } else { "Left" }
    } else if a.y >= a.z {
        if z.y > 0.0 { "Top" } else { "Bottom" }
    } else if z.z > 0.0 {
        "Back"
    } else {
        "Front"
    };
    let input = match crate::changes::read::enum_value(dom, j.part0, &format!("{face}SurfaceInput"))
    {
        12 => SurfaceInput::Constant,
        13 => SurfaceInput::Sin,
        _ => SurfaceInput::NoInput,
    };
    Some(HingeInput {
        stepping,
        input,
        param_a: float(dom, j.part0, &format!("{face}ParamA")),
        param_b: float(dom, j.part0, &format!("{face}ParamB")),
        strength: rotate_v_strength(dom, j),
    })
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
