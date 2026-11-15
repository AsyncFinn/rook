//! Moving an assembly's parts as scripts and the engine do: a part's
//! velocity, and a new CFrame that carries its welded parts with it.

use super::*;

/// The deprecated `BasePart.Velocity`: the assembly's velocity at the
/// part's position, `AssemblyLinearVelocity + AssemblyAngularVelocity ×
/// (position − center of mass)` (observed: a part 3.56 studs from the
/// center of mass of an assembly spinning at 2 rad/s reads 7.11). An
/// anchored part reads its own AssemblyLinearVelocity, which it keeps.
pub fn part_velocity(dom: &Dom, physics: &crate::Physics, part: InstanceId) -> glam::Vec3 {
    let vector = |id, name| dom.vector3_property(id, name).unwrap_or(glam::Vec3::ZERO);
    let linear = vector(part, "AssemblyLinearVelocity");
    let angular = vector(part, "AssemblyAngularVelocity");
    if angular == glam::Vec3::ZERO {
        return linear;
    }
    let position = |id| {
        dom.cframe_property(id, "CFrame")
            .map_or(glam::Vec3::ZERO, |c| c.position)
    };
    let com = match Assembly::of(dom, part) {
        Some(asm) if !asm.anchored => asm.center_of_mass(dom, physics),
        _ => position(part),
    };
    linear + angular.cross(position(part) - com)
}

/// Set `AssemblyLinearVelocity` or `AssemblyAngularVelocity` (`name`) the
/// way a script does: it's the whole assembly's, so every member reads it
/// back at once (observed, also through `Velocity` on a welded part).
pub fn set_velocity(
    dom: &mut Dom,
    part: InstanceId,
    name: &str,
    value: glam::Vec3,
) -> Result<(), DomError> {
    let members = match Assembly::of(dom, part) {
        Some(asm) => asm.members.iter().map(|m| m.part).collect(),
        None => vec![part],
    };
    // Observed: a NaN or infinite number anywhere writes zero.
    let value = if value.is_finite() {
        value
    } else {
        glam::Vec3::ZERO
    };
    let value = Value::Vector3(rook_types::Vector3(value));
    for m in members {
        dom.set_property_by_name(m, name, value.clone())?;
    }
    Ok(())
}

/// Set a part's CFrame the way a script does: its whole assembly moves
/// with it (observed, even when another part is anchored).
pub fn move_part(dom: &mut Dom, part: InstanceId, cf: CFrame) -> Result<(), DomError> {
    move_parts(dom, [(part, cf)])
}

/// [`move_part`] for several parts moved together (a model's), each move
/// given relative to the same starting poses: a part whose assembly an
/// earlier one already moved is skipped.
///
/// A CFrame with a NaN or infinite number in it is [`CFrame::written`]
/// instead. Parts' CFrames are orthonormalized as they're written, as Roblox's are
/// (creator-docs CFrame:Orthonormalize). Without that, rounding error in a
/// rotation grows threefold with each `SetPrimaryPartCFrame(Get.. * r)`,
/// whose inverse assumes an orthonormal rotation, until parts balloon.
pub fn move_parts(
    dom: &mut Dom,
    moves: impl IntoIterator<Item = (InstanceId, CFrame)>,
) -> Result<(), DomError> {
    let mut moved = FxHashSet::default();
    // Moving parts changes no joint, so the joints found for one part
    // serve the rest: a model moved every frame walks each cluster once
    // a call, not once a part.
    let mut clusters = Clusters::default();
    for (part, cf) in moves {
        let cf = cf.written().orthonormalize();
        if moved.contains(&part) {
            continue;
        }
        // A joint holds its parts by reference: a part nothing refers to
        // is alone in its assembly, which needn't be built to say so.
        if dom.referrers(part).is_empty() {
            write_cframe(dom, part, cf)?;
            continue;
        }
        match Assembly::of_with(dom, part, &mut clusters) {
            Some(asm) if asm.members.len() > 1 => {
                asm.place_around(dom, part, cf)?;
                moved.extend(asm.members.iter().map(|m| m.part));
            }
            _ => write_cframe(dom, part, cf)?,
        }
    }
    Ok(())
}
