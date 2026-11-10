//! FloorWire: a wire laid along the floor between two parts, drawn with
//! its `From` part; its moving images are the renderer's
//! ([`rook_render::FloorWire`], read in `effects`). Each wire's floor is
//! kept in [`Floors`].

use glam::Quat;
use rook_physics::Physics;
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
pub(super) fn floor_wire(
    dom: &Dom,
    id: InstanceId,
    from: CFrame,
    floors: &Floors,
) -> Vec<(PartInstance, Pose)> {
    let Some(path) = path(dom, id, floors) else {
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
/// where the wire drops (Inference), as [`Floors`] keeps it.
pub(crate) fn path(dom: &Dom, id: InstanceId, floors: &Floors) -> Option<Vec<Vec3>> {
    let (start, end) = ends(dom, id)?;
    let floor = floors.height(id, start).unwrap_or(start.y - GAP) + GAP;
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

/// Where FloorWire `id` leaves `From` (where it drops to the floor) and
/// reaches `To`, as [`path`] describes.
fn ends(dom: &Dom, id: InstanceId) -> Option<(Vec3, Vec3)> {
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
    Some((start, end))
}

/// Part `id`'s center and the half size of its box in the world, where
/// it's drawn.
fn world_box(dom: &Dom, id: InstanceId) -> Option<(Vec3, Vec3)> {
    let size = dom.vector3_property(id, "Size")?;
    let cf = dom.drawn_cframe(id)?;
    Some((cf.position, cf.box_reach(size)))
}

/// Every FloorWire's floor: the top of the highest Workspace part box
/// under where its wire drops (not its own two parts), found through
/// physics' part boxes. Kept by change, so a frame costs what changed: a
/// wire looks again only when it changed, one of its parts did, or a part
/// changed that was in, or now reaches into, the column under its drop.
#[derive(Debug, Default)]
pub(crate) struct Floors {
    wires: FxHashMap<InstanceId, Floor>,
}

#[derive(Debug, PartialEq)]
struct Floor {
    /// Where the wire drops, as [`ends`] had it.
    drop: Vec3,
    /// The floor's top, if anything is under the drop.
    height: Option<f32>,
    /// The parts whose boxes meet the column under the drop.
    column: Vec<InstanceId>,
}

impl Floors {
    /// Find every FloorWire's floor (a scene being built).
    pub(crate) fn build(&mut self, dom: &mut Dom, physics: &mut Physics) {
        profiling::scope!("Floors::build");
        self.wires.clear();
        let wires: Vec<InstanceId> = (dom.descendants(dom.root()).into_iter())
            .filter(|&i| dom.class_name(i) == "FloorWire")
            .collect();
        if wires.is_empty() {
            return;
        }
        physics.sync(dom);
        for wire in wires {
            if let Some(floor) = find(dom, physics, wire) {
                self.wires.insert(wire, floor);
            }
        }
    }

    /// Bring the floors up to date with a frame's changes. Returns the
    /// wires whose floor changed, which draw again.
    pub(crate) fn update(
        &mut self,
        dom: &mut Dom,
        physics: &mut Physics,
        changes: &RenderChanges,
    ) -> Vec<InstanceId> {
        for id in &changes.collected {
            self.wires.remove(id);
        }
        let d = &*dom;
        let mut look: FxHashSet<InstanceId> = (changes.effects.iter())
            .chain(&changes.effects_placed)
            .filter(|&&i| d.class_name(i) == "FloorWire")
            .copied()
            .collect();
        if !self.wires.is_empty() {
            profiling::scope!("Floors::touched");
            let touched: Vec<InstanceId> = (changes.moved.iter())
                .chain(&changes.parts)
                .chain(&changes.placed)
                .chain(&changes.collected)
                .copied()
                .collect();
            let boxes: Vec<Option<(Vec3, Vec3)>> = touched
                .iter()
                .map(|&p| rook_physics::part_box(d, p))
                .collect();
            for (&wire, floor) in &self.wires {
                let ends = [d.ref_property(wire, "From"), d.ref_property(wire, "To")];
                let hit = touched.iter().zip(&boxes).any(|(p, b)| {
                    ends.contains(&Some(*p))
                        || floor.column.contains(p)
                        || b.is_some_and(|b| under(b, floor.drop))
                });
                if hit {
                    look.insert(wire);
                }
            }
        }
        if look.is_empty() {
            return Vec::new();
        }
        profiling::scope!("Floors::update");
        physics.sync(dom);
        let dom = &*dom;
        let mut changed = Vec::new();
        for wire in look {
            match find(dom, physics, wire) {
                Some(floor) => {
                    if self.wires.get(&wire) != Some(&floor) {
                        changed.push(wire);
                    }
                    self.wires.insert(wire, floor);
                }
                None => {
                    if self.wires.remove(&wire).is_some() {
                        changed.push(wire);
                    }
                }
            }
        }
        changed.sort_unstable();
        changed
    }

    /// FloorWire `wire`'s floor top, for the wire dropping at `drop`.
    fn height(&self, wire: InstanceId, drop: Vec3) -> Option<f32> {
        let floor = self.wires.get(&wire);
        assert!(
            floor.is_some_and(|f| f.drop == drop),
            "FloorWire {wire:?}'s floor wasn't found again after its drop moved"
        );
        floor?.height
    }
}

/// FloorWire `wire`'s floor as `dom` has it now (physics synced with it).
fn find(dom: &Dom, physics: &mut Physics, wire: InstanceId) -> Option<Floor> {
    let (drop, _) = ends(dom, wire)?;
    let skip = [dom.ref_property(wire, "From"), dom.ref_property(wire, "To")];
    let column = physics.parts_by_box(dom, Vec3::new(drop.x, f32::MIN, drop.z), drop);
    let height = (column.iter())
        .filter(|&&p| !skip.contains(&Some(p)))
        .filter_map(|&p| rook_physics::part_box(dom, p))
        .map(|(_, top)| top.y)
        .filter(|&y| y <= drop.y)
        .reduce(f32::max);
    Some(Floor {
        drop,
        height,
        column,
    })
}

fn under((lo, hi): (Vec3, Vec3), drop: Vec3) -> bool {
    lo.x <= drop.x && drop.x <= hi.x && lo.z <= drop.z && drop.z <= hi.z && lo.y <= drop.y
}
