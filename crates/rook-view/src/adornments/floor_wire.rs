//! FloorWire: a wire laid along the floor between two parts, drawn with
//! its `From` part; its moving images are the renderer's
//! ([`rook_render::FloorWire`], read in `effects`).

use glam::Quat;
use rook_types::PartShape;

use super::*;

/// How much bigger than the wire its corner balls are (measured: about
/// 1.25 × `WireRadius`, so they bulge at the corners).
const CORNER: f32 = 1.25;

/// How far the wire keeps off the floor and its parts' faces (measured:
/// 0.2 studs, whatever its radius or the parts' sizes).
const GAP: f32 = 0.2;

/// One FloorWire from a part at `from`: tubes `WireRadius` thick (0.0625
/// by default) in `Color3`, lit, along [`path`], with a ball at each
/// corner. Rook draws nothing without a `To`.
pub(super) fn floor_wire(dom: &Dom, id: InstanceId, from: CFrame) -> Vec<(PartInstance, Pose)> {
    let Some(path) = path(dom, id) else {
        return Vec::new();
    };
    let radius = dom.float_property(id, "WireRadius").unwrap_or(0.0625);
    let wire = PartInstance {
        light: Light::Lit,
        ..piece(dom, id, DrawShape::Adorn(AdornShape::Tube { hole: 0 }))
    };
    if radius <= 0.0 || wire.transparency >= 1.0 {
        return Vec::new();
    }
    // Into the From part's axes.
    let local = |p: Vec3| from.rotation.transpose() * (p - from.position);
    let mut out = Vec::new();
    for (i, leg) in path.windows(2).enumerate() {
        let (p, q) = (local(leg[0]), local(leg[1]));
        let length = p.distance(q);
        let turn = Mat3::from_quat(Quat::from_rotation_arc(Vec3::Z, (q - p) / length));
        let tube = PartInstance {
            size: Vec3::new(2.0 * radius, 2.0 * radius, length),
            ..wire
        };
        out.push((
            tube,
            Pose {
                offset: (p + q) * 0.5,
                turn,
            },
        ));
        if i > 0 {
            let ball = PartInstance {
                shape: DrawShape::Part(PartShape::Ball),
                size: Vec3::splat(2.0 * radius * CORNER),
                ..wire
            };
            out.push((ball, at(p)));
        }
    }
    out
}

/// FloorWire `id`'s corners in the world, from `From` to `To`, without
/// legs of no length. Measured in Studio 0.743 (2026-10-20): it leaves
/// `From`'s center along X towards `To` (along Z when they line up in X),
/// drops `GAP` past `From`'s face to `GAP` above the floor under it (the
/// top of what's below), runs along X, then along Z, and rises `GAP`
/// short of `To`'s face to `To`'s center height; the bits inside the parts
/// aren't drawn. Rook takes faces and floors from the parts' world boxes
/// (Approximation: a turned part's box reaches past it), the floor under
/// where the wire drops (Inference).
pub(crate) fn path(dom: &Dom, id: InstanceId) -> Option<Vec<Vec3>> {
    let (from, to) = (dom.ref_property(id, "From")?, dom.ref_property(id, "To")?);
    let (a, b) = (world_box(dom, from)?, world_box(dom, to)?);
    let (pa, pb) = (a.0, b.0);
    let way = (pb - pa).signum();
    let (dx, dz) = (pb.x != pa.x, pb.z != pa.z);
    let start = match (dx, dz) {
        (true, _) => pa + Vec3::X * way.x * (a.1.x + GAP),
        (false, true) => pa + Vec3::Z * way.z * (a.1.z + GAP),
        (false, false) => pa,
    };
    let end = match (dx, dz) {
        (_, true) => pb - Vec3::Z * way.z * (b.1.z + GAP),
        (true, false) => pb - Vec3::X * way.x * (b.1.x + GAP),
        (false, false) => pb,
    };
    let floor = floor_under(dom, start, &[from, to]).unwrap_or(start.y - GAP) + GAP;
    let corners = [
        start,
        Vec3::new(start.x, floor, start.z),
        Vec3::new(end.x, floor, start.z),
        Vec3::new(end.x, floor, end.z),
        end,
    ];
    let mut path = vec![start];
    for p in &corners[1..] {
        if p.distance(*path.last().unwrap()) > 1e-4 {
            path.push(*p);
        }
    }
    (path.len() > 1).then_some(path)
}

/// Part `id`'s center and the half size of its box in the world.
fn world_box(dom: &Dom, id: InstanceId) -> Option<(Vec3, Vec3)> {
    let size = dom.vector3_property(id, "Size")?;
    let cf = dom.drawn_cframe(id)?;
    let r = cf.rotation;
    let half = Mat3::from_cols(r.x_axis.abs(), r.y_axis.abs(), r.z_axis.abs()) * (size * 0.5);
    Some((cf.position, half))
}

/// The top of the highest Workspace part under `at` (not those in `skip`).
fn floor_under(dom: &Dom, at: Vec3, skip: &[InstanceId]) -> Option<f32> {
    let mut floor: Option<f32> = None;
    for part in workspace_parts(dom) {
        if skip.contains(&part) {
            continue;
        }
        let Some((center, half)) = world_box(dom, part) else {
            continue;
        };
        let (lo, hi) = (center - half, center + half);
        let over = at.x >= lo.x && at.x <= hi.x && at.z >= lo.z && at.z <= hi.z;
        if over && hi.y <= at.y && floor.is_none_or(|f| hi.y > f) {
            floor = Some(hi.y);
        }
    }
    floor
}
