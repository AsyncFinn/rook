//! Assemblies: parts joined by rigid joints, which move as one body.
//!
//! Like [`crate::joints`], this reads the DOM alone. The root part stays
//! where it is and the others are placed from it through the joints, so
//! a joint that becomes active snaps its non-root side into place.
//!
//! Anchored parts split assemblies (observed in Studio 0.741): each
//! anchored part is the root of its own assembly, so two anchored parts
//! welded together don't move together. Unanchored parts joined to each
//! other form a cluster; a cluster joined to anchored parts belongs to the
//! assembly of one of them, the one whose joint is newest in Studio.

use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

use rook_dom::{Dom, DomError, InstanceId};
use rook_types::{CFrame, Value};

use crate::joints::{Joint, part_joints};
use crate::shape::is_part;

/// One part of an assembly.
#[derive(Debug, Clone, Copy)]
pub struct Member {
    pub part: InstanceId,
    /// The part's pose relative to the root.
    pub offset: CFrame,
    /// The joint placing it, and the index of the member on its other side
    /// (`None` for the root).
    link: Option<(Joint, usize)>,
}

/// Parts joined by rigid joints.
#[derive(Debug, Clone)]
pub struct Assembly {
    /// Root first, then the others in breadth-first order from it.
    pub members: Vec<Member>,
    /// Whether the root is Anchored, which anchors the whole assembly.
    pub anchored: bool,
    /// Non-rigid joints (hinges) attached to a member.
    pub hinges: Vec<Joint>,
}

/// Unanchored parts joined to each other, and the anchored parts they're
/// joined to.
#[derive(Debug)]
struct Cluster {
    parts: Vec<InstanceId>,
    /// (anchored part, joint to it).
    anchors: Vec<(InstanceId, Joint)>,
}

impl Cluster {
    fn from(dom: &Dom, start: InstanceId) -> Self {
        let mut parts = vec![start];
        let mut seen = HashSet::from([start]);
        let mut anchors = Vec::new();
        let mut next = 0;
        while next < parts.len() {
            let p = parts[next];
            for j in part_joints(dom, p) {
                if !j.kind.is_rigid() {
                    continue;
                }
                let other = j.other(p);
                if is_anchored(dom, other) {
                    anchors.push((other, j));
                } else if seen.insert(other) {
                    parts.push(other);
                }
            }
            next += 1;
        }
        Self { parts, anchors }
    }

    /// The anchored part whose assembly the cluster joins: the newest
    /// joint's, taking the highest id as the newest (Approximation).
    fn anchor(&self) -> Option<InstanceId> {
        self.anchors
            .iter()
            .max_by_key(|(_, j)| j.id)
            .map(|(a, _)| *a)
    }
}

/// Clusters already walked, by every part in them. A cluster is the same
/// whichever of its parts the walk starts from (its anchor is the newest
/// joint whatever order they're found in), so finding the assemblies of
/// many parts at once (a rebuild) walks each cluster once instead of once
/// per part or per joint into it. Valid while joints and `Anchored` don't
/// change.
#[derive(Debug, Default)]
pub struct Clusters {
    of: HashMap<InstanceId, Rc<Cluster>>,
}

impl Clusters {
    fn get(&mut self, dom: &Dom, part: InstanceId) -> Rc<Cluster> {
        if let Some(c) = self.of.get(&part) {
            return c.clone();
        }
        let cluster = Rc::new(Cluster::from(dom, part));
        for &p in &cluster.parts {
            self.of.insert(p, cluster.clone());
        }
        cluster
    }
}

impl Assembly {
    /// The assembly `part` belongs to. Parts outside Workspace have none.
    pub fn of(dom: &Dom, part: InstanceId) -> Option<Self> {
        Self::of_with(dom, part, &mut Clusters::default())
    }

    /// [`Assembly::of`], reusing clusters found for earlier parts.
    pub fn of_with(dom: &Dom, part: InstanceId, clusters: &mut Clusters) -> Option<Self> {
        let ws = dom.find_service("Workspace")?;
        if !is_part(dom, part) || !dom.is_ancestor_of(ws, part) {
            return None;
        }
        let (root, parts) = if is_anchored(dom, part) {
            (part, anchored_members(dom, part, clusters))
        } else {
            let cluster = clusters.get(dom, part);
            match cluster.anchor() {
                Some(anchor) => (anchor, anchored_members(dom, anchor, clusters)),
                None => (root_of(dom, &cluster.parts), cluster.parts.clone()),
            }
        };
        Some(Self::build(dom, root, &parts))
    }

    /// Lay out `parts` from `root` through the rigid joints between them.
    fn build(dom: &Dom, root: InstanceId, parts: &[InstanceId]) -> Self {
        let inside: HashSet<InstanceId> = parts.iter().copied().collect();
        let mut members = vec![Member {
            part: root,
            offset: CFrame::IDENTITY,
            link: None,
        }];
        let mut placed = HashMap::from([(root, 0)]);
        let mut queue = VecDeque::from([root]);
        let mut hinges: Vec<Joint> = Vec::new();
        while let Some(p) = queue.pop_front() {
            let from = placed[&p];
            for j in part_joints(dom, p) {
                if !j.kind.is_rigid() {
                    if !hinges.iter().any(|h| h.id == j.id) {
                        hinges.push(j);
                    }
                    continue;
                }
                let other = j.other(p);
                if !inside.contains(&other) || placed.contains_key(&other) {
                    continue;
                }
                placed.insert(other, members.len());
                members.push(Member {
                    part: other,
                    offset: CFrame::IDENTITY,
                    link: Some((j, from)),
                });
                queue.push_back(other);
            }
        }
        let mut asm = Self {
            members,
            anchored: is_anchored(dom, root),
            hinges,
        };
        asm.refresh_offsets(dom);
        asm
    }

    pub fn root(&self) -> InstanceId {
        self.members[0].part
    }

    pub fn offset_of(&self, part: InstanceId) -> Option<CFrame> {
        self.members
            .iter()
            .find(|m| m.part == part)
            .map(|m| m.offset)
    }

    /// The rigid joints holding the assembly together.
    pub fn joints(&self) -> impl Iterator<Item = &Joint> {
        self.members
            .iter()
            .filter_map(|m| m.link.as_ref().map(|(j, _)| j))
    }

    /// Recompute offsets from the joints (after a C0, C1, Transform or
    /// motor angle changed).
    pub fn refresh_offsets(&mut self, dom: &Dom) {
        for i in 1..self.members.len() {
            let Some((j, from)) = self.members[i].link else {
                continue;
            };
            let rel = crate::joints::offset(dom, &j);
            let rel = if j.part0 == self.members[from].part {
                rel
            } else {
                rel.inverse()
            };
            self.members[i].offset = self.members[from].offset * rel;
        }
    }

    /// Write every member's CFrame from the root's.
    pub fn place(&self, dom: &mut Dom, root: CFrame) -> Result<(), DomError> {
        for m in &self.members {
            dom.set_property_by_name(m.part, "CFrame", Value::CFrame(root * m.offset))?;
        }
        Ok(())
    }

    /// `AssemblyMass`: the members' masses, except Massless ones that
    /// aren't the root; infinite when anchored.
    pub fn mass(&self, dom: &Dom) -> f32 {
        if self.anchored {
            return f32::INFINITY;
        }
        self.members
            .iter()
            .enumerate()
            .filter(|(i, m)| *i == 0 || !is_massless(dom, m.part))
            .map(|(_, m)| crate::part_mass(dom, m.part))
            .sum()
    }
}

/// The deprecated `BasePart.Velocity`: the assembly's velocity at the
/// part's position, `AssemblyLinearVelocity + AssemblyAngularVelocity ×
/// (position − centre of mass)` (observed: a part 3.56 studs from the
/// centre of mass of an assembly spinning at 2 rad/s reads 7.11). An
/// anchored part reads its own AssemblyLinearVelocity, which it keeps.
pub fn part_velocity(dom: &Dom, part: InstanceId) -> glam::Vec3 {
    let vector = |id, name| match dom.property_by_name(id, name) {
        Some(Value::Vector3(v)) => v.0,
        _ => glam::Vec3::ZERO,
    };
    let linear = vector(part, "AssemblyLinearVelocity");
    let angular = vector(part, "AssemblyAngularVelocity");
    if angular == glam::Vec3::ZERO {
        return linear;
    }
    let position = |id| match dom.property_by_name(id, "CFrame") {
        Some(Value::CFrame(c)) => c.position,
        _ => glam::Vec3::ZERO,
    };
    let com = match Assembly::of(dom, part) {
        Some(asm) if !asm.anchored && asm.members.len() > 1 => {
            let (mut sum, mut mass) = (glam::Vec3::ZERO, 0.0);
            for (i, m) in asm.members.iter().enumerate() {
                if i == 0 || !is_massless(dom, m.part) {
                    let w = crate::part_mass(dom, m.part);
                    sum += position(m.part) * w;
                    mass += w;
                }
            }
            if mass > 0.0 {
                sum / mass
            } else {
                position(part)
            }
        }
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
pub fn move_parts(
    dom: &mut Dom,
    moves: impl IntoIterator<Item = (InstanceId, CFrame)>,
) -> Result<(), DomError> {
    let mut moved = std::collections::HashSet::new();
    for (part, cf) in moves {
        if moved.contains(&part) {
            continue;
        }
        match Assembly::of(dom, part) {
            Some(asm) if asm.members.len() > 1 => {
                let offset = asm.offset_of(part).unwrap_or(CFrame::IDENTITY);
                asm.place(dom, cf * offset.inverse())?;
                moved.extend(asm.members.iter().map(|m| m.part));
            }
            _ => dom.set_property_by_name(part, "CFrame", Value::CFrame(cf))?,
        }
    }
    Ok(())
}

/// An anchored part and the clusters that join its assembly.
fn anchored_members(dom: &Dom, anchored: InstanceId, clusters: &mut Clusters) -> Vec<InstanceId> {
    let mut parts = vec![anchored];
    let mut joined: HashSet<InstanceId> = HashSet::new();
    for j in part_joints(dom, anchored) {
        let other = j.other(anchored);
        if !j.kind.is_rigid() || is_anchored(dom, other) || joined.contains(&other) {
            continue;
        }
        let cluster = clusters.get(dom, other);
        if cluster.anchor() == Some(anchored) {
            joined.extend(cluster.parts.iter().copied());
            parts.extend(cluster.parts.iter().copied());
        }
    }
    parts
}

/// `GetConnectedParts`: the part and the parts rigidly joined to it; with
/// `recursive`, everything reachable through rigid joints, past anchored
/// parts too (observed), so it can be more than the assembly.
pub fn connected_parts(dom: &Dom, part: InstanceId, recursive: bool) -> Vec<InstanceId> {
    let mut parts = vec![part];
    let mut seen = HashSet::from([part]);
    let mut next = 0;
    while next < parts.len() && (recursive || next == 0) {
        let p = parts[next];
        for j in part_joints(dom, p) {
            if j.kind.is_rigid() && seen.insert(j.other(p)) {
                parts.push(j.other(p));
            }
        }
        next += 1;
    }
    parts
}

fn is_anchored(dom: &Dom, part: InstanceId) -> bool {
    dom.property_by_name(part, "Anchored") == Some(Value::Bool(true))
}

pub(crate) fn is_massless(dom: &Dom, part: InstanceId) -> bool {
    dom.property_by_name(part, "Massless") == Some(Value::Bool(true))
}

/// The root of an unanchored cluster (creator-docs "Assemblies"; order
/// observed in Studio): non-Massless parts, then the highest RootPriority,
/// then the largest part. Equal parts go to the newer one in Studio; Rook
/// takes the highest id, which is usually the newer part (Approximation).
fn root_of(dom: &Dom, parts: &[InstanceId]) -> InstanceId {
    let key = |p: InstanceId| {
        let priority = match dom.property_by_name(p, "RootPriority") {
            Some(Value::Int32(n)) => n,
            _ => 0,
        };
        (
            !is_massless(dom, p),
            priority,
            crate::part_volume(dom, p),
            p,
        )
    };
    parts
        .iter()
        .copied()
        .max_by(|a, b| {
            key(*a)
                .partial_cmp(&key(*b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("an assembly has a part")
}
