//! Rigid clusters: parts welded together, found once per part and shared
//! by every lookup that meets them.

use super::*;

/// Unanchored parts joined to each other, and the anchored parts they're
/// joined to.
#[derive(Debug)]
pub(super) struct Cluster {
    pub(super) parts: Vec<InstanceId>,
    /// (anchored part, joint to it).
    pub(super) anchors: Vec<(InstanceId, Joint)>,
}

impl Cluster {
    pub(super) fn from(dom: &Dom, start: InstanceId, joints: &mut Joints) -> Self {
        let mut parts = vec![start];
        let mut seen = FxHashSet::from_iter([start]);
        let mut anchors = Vec::new();
        let mut next = 0;
        while next < parts.len() {
            let p = parts[next];
            for &j in joints.of(dom, p).iter() {
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
    pub(super) fn anchor(&self) -> Option<InstanceId> {
        self.anchors
            .iter()
            .max_by_key(|(_, j)| j.id)
            .map(|(a, _)| *a)
    }
}

/// Each part's active joints, found once. Checking a joint is active takes
/// a dozen lookups, and a rebuild would otherwise check every joint of a
/// big assembly four times (from both its parts, walking the cluster and
/// then laying out the assembly).
#[derive(Debug, Default)]
pub(super) struct Joints(FxHashMap<InstanceId, Rc<[Joint]>>);

impl Joints {
    pub(super) fn of(&mut self, dom: &Dom, part: InstanceId) -> Rc<[Joint]> {
        self.0
            .entry(part)
            .or_insert_with(|| part_joints(dom, part).into())
            .clone()
    }
}

/// Clusters already walked, by every part in them, and the joints found
/// on the way. A cluster is the same whichever of its parts the walk
/// starts from (its anchor is the newest joint whatever order they're
/// found in), so finding the assemblies of many parts at once (a rebuild)
/// walks each cluster once instead of once per part or per joint into it.
/// Valid while joints, parts' places in the tree and `Anchored` don't
/// change.
#[derive(Debug, Default)]
pub struct Clusters {
    pub(super) of: FxHashMap<InstanceId, Rc<Cluster>>,
    pub(super) joints: Joints,
}

impl Clusters {
    pub(super) fn get(&mut self, dom: &Dom, part: InstanceId) -> Rc<Cluster> {
        if let Some(c) = self.of.get(&part) {
            return c.clone();
        }
        let cluster = Rc::new(Cluster::from(dom, part, &mut self.joints));
        for &p in &cluster.parts {
            self.of.insert(p, cluster.clone());
        }
        cluster
    }
}

/// An anchored part and the clusters that join its assembly.
pub(super) fn anchored_members(
    dom: &Dom,
    anchored: InstanceId,
    clusters: &mut Clusters,
) -> Vec<InstanceId> {
    let mut parts = vec![anchored];
    let mut joined: FxHashSet<InstanceId> = FxHashSet::default();
    for &j in clusters.joints.of(dom, anchored).iter() {
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
