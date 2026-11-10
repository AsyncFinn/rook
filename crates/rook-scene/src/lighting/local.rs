//! Local lights: PointLight, SpotLight and SurfaceLight under parts and
//! their Attachments in Workspace (docs/behavior/lighting/local-lights.md).

use rook_render::{LightShape, LocalLight};

use super::*;

fn is_light(dom: &Dom, id: InstanceId) -> bool {
    dom.is_a(id, "Light")
}

/// A light as it shines now: under a part, from the part (a surface
/// light from the part's `Face`); under an Attachment, from the
/// attachment, facing along its axes. `None` when it isn't under either
/// (Studio lights nothing then). A disabled light shines with brightness
/// 0, so that scripts switching lights on and off (alarms) change only
/// their colors.
fn light(dom: &Dom, id: InstanceId) -> Option<LocalLight> {
    let enabled = dom.bool_property(id, "Enabled") != Some(false);
    let parent = dom.parent(id)?;
    let part = if is_part(dom, parent) {
        parent
    } else if dom.class_name(parent) == "Attachment" {
        dom.parent(parent).filter(|&p| is_part(dom, p))?
    } else {
        return None;
    };
    let part_cf = dom.drawn_cframe(part)?;
    let part_size = dom.vector3_property(part, "Size").unwrap_or(Vec3::ONE);
    let (cf, size) = if part == parent {
        (part_cf, part_size)
    } else {
        (part_cf * dom.cframe_property(parent, "CFrame")?, Vec3::ZERO)
    };
    let extent = part_cf.box_reach(part_size);
    // SpotLight and SurfaceLight face Front (-Z) by default.
    let face = dom
        .enum_property(id, "Face")
        .and_then(NormalId::from_u32)
        .unwrap_or(NormalId::Front);
    let direction = cf.rotation * face.normal();
    let angle = dom.float_property(id, "Angle").unwrap_or(90.0);
    let (position, shape) = match dom.class_name(id) {
        "PointLight" => (cf.position, LightShape::Point),
        // Measured: a spot light shines from its part's center.
        "SpotLight" => (cf.position, LightShape::Spot { direction, angle }),
        "SurfaceLight" => {
            // From the center of the part's face.
            let n = face.normal();
            let out = n * n.abs().dot(size * 0.5);
            (
                cf.position + cf.rotation * out,
                LightShape::Surface { direction, angle },
            )
        }
        _ => return None,
    };
    Some(LocalLight {
        position,
        shape,
        color: dom
            .color3_property(id, "Color")
            .map(Color3::to_array)
            .unwrap_or([1.0; 3]),
        brightness: if enabled {
            dom.float_property(id, "Brightness").unwrap_or(1.0)
        } else {
            0.0
        },
        range: dom.float_property(id, "Range").unwrap_or(8.0),
        shadows: dom.bool_property(id, "Shadows").unwrap_or(false),
        part: (part_cf.position - extent, part_cf.position + extent),
    })
}

/// The instances whose changes change light `id`: itself, and the part or
/// Attachment it hangs from (and that Attachment's part).
fn holders(dom: &Dom, id: InstanceId) -> Vec<InstanceId> {
    let mut out = vec![id];
    let mut holder = dom.parent(id);
    while let Some(h) = holder {
        out.push(h);
        if dom.class_name(h) != "Attachment" {
            break;
        }
        holder = dom.parent(h);
    }
    out
}

/// What [`light`] gives a light Studio wouldn't draw (not under a part or
/// an Attachment): no light at all.
const DARK: LocalLight = LocalLight {
    position: Vec3::ZERO,
    shape: LightShape::Point,
    color: [0.0; 3],
    brightness: 0.0,
    range: 0.0,
    shadows: false,
    part: (Vec3::ZERO, Vec3::ZERO),
};

/// Every local light in Workspace, drawn or not, as `scene.lights`: found
/// once, then kept up to date light by light from the journal.
#[derive(Debug, Default)]
pub(crate) struct LightCache {
    ids: Vec<InstanceId>,
    /// Each light's entry, and the instances it was read from.
    lights: FxHashMap<InstanceId, (usize, Vec<InstanceId>)>,
    /// The lights each instance changes (see [`holders`]).
    held: FxHashMap<InstanceId, Vec<InstanceId>>,
    /// Entries changed since the renderer took them (`scene.lights` may
    /// also have grown or shrunk).
    changed: Vec<usize>,
}

impl LightCache {
    pub(crate) fn build(&mut self, dom: &Dom, list: &mut Vec<LocalLight>) {
        profiling::scope!("LightCache::build");
        *self = Self::default();
        list.clear();
        let Some(ws) = dom.find_service("Workspace") else {
            return;
        };
        for id in dom.descendants(ws) {
            if is_light(dom, id) {
                self.place(dom, id, list);
            }
        }
    }

    pub(crate) fn take_changes(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.changed)
    }

    /// Bring the lights up to date with a frame's changes: those that
    /// entered or left Workspace come or go, and only the lights that
    /// changed, and those on parts and Attachments that did, are read
    /// again.
    pub(crate) fn apply(&mut self, dom: &Dom, changes: &RenderChanges, list: &mut Vec<LocalLight>) {
        // Freed: dropped unread.
        for id in &changes.collected {
            self.remove(*id, list);
        }
        let held = |ids: &mut FxHashSet<InstanceId>, held: &FxHashMap<_, Vec<_>>, of| {
            ids.extend(held.get(of).into_iter().flatten().copied());
        };
        let mut placed = FxHashSet::default();
        for id in changes.collected.iter().chain(&changes.placed) {
            held(&mut placed, &self.held, id);
        }
        placed.extend(&changes.lights_placed);
        for &id in &placed {
            self.place(dom, id, list);
        }
        let mut stale = FxHashSet::default();
        for id in (changes.moved.iter())
            .chain(&changes.parts)
            .chain(&changes.lights)
        {
            held(&mut stale, &self.held, id);
        }
        if stale.is_empty() {
            return;
        }
        profiling::scope!("LightCache::read");
        for id in stale.difference(&placed) {
            let i = self.lights[id].0;
            list[i] = light(dom, *id).unwrap_or(DARK);
            self.changed.push(i);
        }
    }

    /// Read light `id` again, adding it if it's new to Workspace and
    /// dropping it if it left.
    fn place(&mut self, dom: &Dom, id: InstanceId, list: &mut Vec<LocalLight>) {
        let ws = dom.find_service("Workspace");
        let here = dom.get(id).is_some()
            && is_light(dom, id)
            && ws.is_some_and(|ws| dom.is_ancestor_of(ws, id));
        if !here {
            self.remove(id, list);
            return;
        }
        let read = light(dom, id).unwrap_or(DARK);
        let holders = holders(dom, id);
        let i = match self.lights.get(&id) {
            Some((i, old)) if *old == holders => {
                list[*i] = read;
                self.changed.push(*i);
                return;
            }
            Some(&(i, _)) => {
                self.unhold(id);
                list[i] = read;
                i
            }
            None => {
                self.ids.push(id);
                list.push(read);
                list.len() - 1
            }
        };
        self.changed.push(i);
        for &h in &holders {
            self.held.entry(h).or_default().push(id);
        }
        self.lights.insert(id, (i, holders));
    }

    /// Stop drawing light `id`; the last entry takes its place.
    fn remove(&mut self, id: InstanceId, list: &mut Vec<LocalLight>) {
        if !self.lights.contains_key(&id) {
            return;
        }
        let i = self.unhold(id);
        self.lights.remove(&id);
        self.ids.swap_remove(i);
        list.swap_remove(i);
        if let Some(&moved) = self.ids.get(i) {
            self.lights.get_mut(&moved).expect("known").0 = i;
            self.changed.push(i);
        }
    }

    /// Forget what light `id` hangs from; its entry.
    fn unhold(&mut self, id: InstanceId) -> usize {
        let (i, holders) = &self.lights[&id];
        for h in holders {
            if let Some(lights) = self.held.get_mut(h) {
                lights.retain(|l| *l != id);
                if lights.is_empty() {
                    self.held.remove(h);
                }
            }
        }
        *i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A light leaving Workspace touches only its own entry: the last
    /// light takes its place and the rest keep theirs.
    #[test]
    fn lights_come_and_go_one_by_one() {
        let mut dom = Dom::new();
        let ws = dom.get_service("Workspace").unwrap();
        let part = dom.create_by_name("Part").unwrap();
        dom.set_parent(part, Some(ws)).unwrap();
        let lights: Vec<InstanceId> = (0..3)
            .map(|_| {
                let l = dom.create_by_name("PointLight").unwrap();
                dom.set_parent(l, Some(part)).unwrap();
                l
            })
            .collect();
        let mut cache = LightCache::default();
        let mut list = Vec::new();
        cache.build(&dom, &mut list);
        cache.take_changes();
        assert_eq!(list.len(), 3);
        dom.set_parent(lights[0], None).unwrap();
        let mut changes = RenderChanges::default();
        changes.lights_placed.insert(lights[0]);
        cache.apply(&dom, &changes, &mut list);
        assert_eq!(cache.take_changes(), [0]);
        assert_eq!(list.len(), 2);
        assert_eq!(cache.ids, [lights[2], lights[1]]);
    }
}
