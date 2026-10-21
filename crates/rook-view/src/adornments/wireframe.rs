//! WireframeHandleAdornment: the lines scripts give it (`AddLine`,
//! `AddLines`, `AddPath`, emptied by `Clear`), which reach the scene as
//! effect calls.

use std::collections::HashMap;

use glam::Quat;
use rook_lua::EffectCall;
use rook_types::PartShape;

use super::handle_adornment::{line_pixels, shaded, shift};
use super::*;

/// Each WireframeHandleAdornment's lines so far, in its CFrame's axes.
pub(crate) type WireLines = HashMap<InstanceId, Vec<[Vec3; 2]>>;

/// Apply scripts' calls on WireframeHandleAdornments to `lines`. Returns
/// the parts whose wireframes changed.
pub(crate) fn note_calls(
    dom: &Dom,
    lines: &mut WireLines,
    calls: &[(InstanceId, EffectCall)],
) -> Vec<InstanceId> {
    let mut changed = Vec::new();
    for &(id, call) in calls {
        if dom.class_name(id) != "WireframeHandleAdornment" {
            continue;
        }
        match call {
            EffectCall::Line(from, to) => lines.entry(id).or_default().push([from, to]),
            EffectCall::Clear => {
                lines.remove(&id);
            }
            _ => continue,
        }
        if let Some(part) = dom.ref_property(id, "Adornee")
            && !changed.contains(&part)
        {
            changed.push(part);
        }
    }
    changed
}

/// One WireframeHandleAdornment on a part of `size`: its lines, placed by
/// its `CFrame` (and `SizeRelativeOffset`) like the other
/// HandleAdornments, its points scaled by `Scale` (Inference).
///
/// Measured in Studio 0.742 (2026-10-20): one pixel across in `Color3`,
/// without light, whatever its `Thickness` (1 and 5 drew alike), hidden
/// by parts in front; it draws nothing until a script adds lines.
pub(super) fn wireframe(
    dom: &Dom,
    id: InstanceId,
    size: Vec3,
    lines: &WireLines,
) -> Vec<(PartInstance, Pose)> {
    let Some(lines) = lines.get(&id) else {
        return Vec::new();
    };
    let cf = dom.cframe_property(id, "CFrame").unwrap_or_default();
    let scale = dom.vector3_property(id, "Scale").unwrap_or(Vec3::ONE);
    let shift = shift(dom, id, size);
    let base = line_pixels(piece(dom, id, DrawShape::Part(PartShape::Block)), 1.0);
    let mut out = Vec::new();
    for &[from, to] in lines {
        let placed = |v: Vec3| cf.position + cf.rotation * (v * scale);
        let (p, q) = (placed(from), placed(to));
        let length = p.distance(q);
        if length <= 0.0 {
            continue;
        }
        let line = PartInstance {
            size: Vec3::new(1.0, 1.0, length),
            ..base
        };
        let pose = Pose {
            offset: shift + (p + q) * 0.5,
            turn: Mat3::from_quat(Quat::from_rotation_arc(Vec3::Z, (q - p) / length)),
        };
        out.extend(shaded(dom, id, line, pose));
    }
    out
}
