//! BasePart placement: CFrame reads and writes, and the Position,
//! Orientation and Rotation views of it.

use std::sync::LazyLock;

use rook_dom::{Dom, DomError, InstanceId};
use rook_physics::assembly::{self};
use rook_physics::joints;
use rook_reflection::PropertyId;
use rook_types::{CFrame, Value, Vector3};

use crate::runtime::world::World;
pub(crate) fn cframe(w: &World, id: InstanceId) -> CFrame {
    w.dom
        .cframe_property(id, "CFrame")
        .unwrap_or(CFrame::IDENTITY)
}

/// [`cframe`] of a BasePart, by id.
pub(crate) fn part_cframe(w: &World, part: InstanceId) -> CFrame {
    (w.dom)
        .read_property(part, part_ids().cframe, Value::as_cframe)
        .unwrap_or(CFrame::IDENTITY)
}

/// BasePart's class and the properties model moves read of every part,
/// resolved once: those walk every part of a model, often every frame.
pub(crate) struct PartIds {
    pub(crate) class: rook_reflection::ClassId,
    pub(crate) cframe: PropertyId,
    pub(crate) size: PropertyId,
}

pub(crate) fn part_ids() -> &'static PartIds {
    static IDS: LazyLock<PartIds> = LazyLock::new(|| {
        let db = rook_reflection::database();
        let class = db.class_by_name("BasePart").expect("BasePart").id;
        let id = |name| db.find_canonical_property(class, name).expect(name).id;
        PartIds {
            class,
            cframe: id("CFrame"),
            size: id("Size"),
        }
    });
    &IDS
}

/// `CollisionFidelity`: Studio works it out from the saved geometry, and a
/// value in the file means nothing (observed in Studio 0.746).
pub(super) fn saved_fidelity(dom: &Dom, id: InstanceId) -> u32 {
    const BOX: u32 = 1;
    rook_physics::saved_blob(dom, id)
        .and_then(|bytes| rook_content::decode::mesh::union::physics::fidelity(&bytes).ok())
        .unwrap_or(BOX)
}

/// Set a part's CFrame as a script does: its assembly moves with it.
pub(crate) fn set_cframe(w: &mut World, id: InstanceId, c: CFrame) -> Result<(), DomError> {
    assembly::move_part(&mut w.dom, id, c)
}

/// Move one part on its own, as setting Position, Orientation or Rotation
/// does: its joints are rewritten to hold it where it now is (observed).
pub(super) fn set_cframe_alone(w: &mut World, id: InstanceId, c: CFrame) -> Result<(), DomError> {
    let pid = w.dom.property_id(id, "CFrame").ok_or(DomError::Stale)?;
    w.dom
        .set_property(id, pid, Value::CFrame(c.written().orthonormalize()))?;
    joints::reseat(&mut w.dom, id)
}

/// In degrees, with no negative zeros: a new part's `Orientation` and
/// `Rotation` read `0, 0, 0` where `CFrame:ToOrientation()` gives `-0`
/// (observed, Studio 0.744).
pub(super) fn degrees(v: (f32, f32, f32)) -> Vector3 {
    let d = |a: f32| a.to_degrees() + 0.0;
    Vector3::new(d(v.0), d(v.1), d(v.2))
}

/// `Orientation`: degrees, applied in Y, X, Z order (Confirmed: creator-docs
/// BasePart.Orientation).
pub(super) fn orientation(c: &CFrame) -> Vector3 {
    degrees(c.to_orientation())
}

/// `Rotation`: degrees, the XYZ Euler angles (Confirmed: creator-docs
/// BasePart.Rotation).
pub(super) fn rotation(c: &CFrame) -> Vector3 {
    degrees(c.to_euler_angles_xyz())
}
