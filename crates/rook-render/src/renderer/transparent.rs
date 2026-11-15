//! Transparent parts: kept with their instances, and drawn back to front,
//! terrain water first. Nothing is done again until a part or the view
//! changes, and then the last order is sorted again (nearly sorted, so
//! about one pass) and only what changed is uploaded.

use std::ops::Range;

use super::*;
use runs::Pipe;

/// How a run of transparent instances is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pass {
    Part,
    /// Terrain water's depth, all of it before any of its color, so each
    /// pixel shows only the nearest water surface, as Studio draws it,
    /// whatever order its chunks come in.
    WaterDepth,
    /// Terrain water, seen from either side, where it is nearest.
    Water,
    Image,
    /// See-through Glass: every part's nearest depth first, then each
    /// one's filter on what's behind, then its surface, so a pixel shows
    /// only the nearest and hides the see-through parts behind it, as
    /// Studio draws it.
    GlassDepth,
    GlassFilter,
    GlassSurface,
    ForceField,
    /// Over everything (`PartInstance::on_top`), after the rest.
    OnTop,
}

impl Pass {
    pub(crate) fn pipe(self) -> Pipe {
        match self {
            Pass::Part => Pipe::Transparent,
            Pass::WaterDepth => Pipe::WaterDepth,
            Pass::Water => Pipe::Water,
            Pass::Image => Pipe::Layer,
            Pass::GlassDepth => Pipe::GlassDepth,
            Pass::GlassFilter => Pipe::GlassFilter,
            Pass::GlassSurface => Pipe::GlassSurface,
            Pass::ForceField => Pipe::Additive,
            Pass::OnTop => Pipe::OnTop,
        }
    }

    /// The passes an entry is drawn in, in order.
    fn of(p: &PartInstance) -> &'static [Pass] {
        if p.on_top {
            return &[Pass::OnTop];
        }
        match (p.layer, p.material) {
            (Layer::Image { .. }, _) => &[Pass::Image],
            (Layer::Base, Material::Glass) => {
                &[Pass::GlassDepth, Pass::GlassFilter, Pass::GlassSurface]
            }
            (Layer::Base, Material::ForceField) => &[Pass::ForceField],
            (Layer::Base, Material::Water) => &[Pass::Water],
            (Layer::Base, _) => &[Pass::Part],
        }
    }
}

/// A run of the frame's transparent instances drawn alike.
pub(crate) type TransparentRun = (BatchKey, Pass, Range<u32>);

/// A transparent part, with what drawing it needs.
#[derive(Debug, Clone, Copy)]
struct Entry {
    /// Its index in the scene.
    entry: u32,
    position: Vec3,
    /// An image layer (drawn just after its part at the same distance).
    image: bool,
    on_top: bool,
    /// Terrain water: drawn before the rest, which its depth then hides
    /// beneath it.
    water: bool,
    /// See-through Glass: drawn next, for the same reason.
    glass: bool,
    key: BatchKey,
    passes: &'static [Pass],
    instance: Instance,
    world: Aabb,
}

impl Entry {
    fn new(entry: u32, part: &PartInstance, instance: Instance) -> Self {
        Self {
            entry,
            position: part.position,
            image: part.layer != Layer::Base,
            on_top: part.on_top,
            water: Pass::of(part) == [Pass::Water],
            glass: Pass::of(part).contains(&Pass::GlassDepth),
            key: BatchKey {
                shape: part.shape,
                images: image_set(part),
                double_sided: part.double_sided,
            },
            passes: Pass::of(part),
            instance,
            world: instance.world_box(),
        }
    }
}

impl Entry {
    /// Where it sorts among the rest before distance: water, then Glass,
    /// then the others, and those on top last.
    fn rank(&self) -> u8 {
        if self.on_top {
            3
        } else if self.water {
            0
        } else if self.glass {
            1
        } else {
            2
        }
    }

    /// Its squared distance `d` as drawn: those on top as if nearest,
    /// water and Glass as if farthest.
    fn shown(&self, d: f32) -> f32 {
        if self.on_top {
            f32::NEG_INFINITY
        } else if self.water || self.glass {
            f32::INFINITY
        } else {
            d
        }
    }
}

/// A slot in drawing order, with what it sorts by.
#[derive(Debug, Clone, Copy)]
struct Ordered {
    rank: u8,
    distance: f32,
    image: bool,
    slot: usize,
}

/// What the last upload was sorted and culled for: the view, where the
/// camera was, and how far boxes reach out.
type DrawnFor = (Frustum, Vec3, f32);

#[derive(Debug, Default)]
pub(crate) struct TransparentParts {
    /// By slot; `None` when free.
    slots: Vec<Option<Entry>>,
    free: Vec<usize>,
    /// A slot was filled, emptied or drawn differently since the last
    /// upload: the runs are made again.
    relook: bool,
    /// Slots whose part only moved since the last upload.
    moved: Vec<usize>,
    is_moved: Vec<bool>,
    drawn_for: Option<DrawnFor>,
    runs: Vec<TransparentRun>,
    /// Per slot: whether the last view reached it, and whether it's in
    /// `order`.
    seen: Vec<bool>,
    in_order: Vec<bool>,
    /// Per slot in `order`: its squared distance from the camera.
    distance: Vec<f32>,
    /// Each sorted instance's squared distance from the camera (those on
    /// top nearer than anything).
    distances: Vec<f32>,
    /// Each sorted instance's scene entry and box in the world.
    picks: Vec<(u32, Aabb)>,
    /// The sorted instances, and how many it holds.
    buffer: Option<(wgpu::Buffer, usize)>,
    /// The last upload's slots in drawing order, and the one before's,
    /// kept to compare.
    order: Vec<Ordered>,
    previous: Vec<Ordered>,
    /// The last upload's instances, with the slot each draws.
    instances: Vec<Instance>,
    instance_slots: Vec<u32>,
}

impl TransparentParts {
    pub(crate) fn clear(&mut self) {
        *self = Self {
            buffer: self.buffer.take(),
            relook: true,
            ..Self::default()
        };
    }

    /// Take a slot for scene entry `entry`, drawn as `part`.
    pub(crate) fn occupy(&mut self, entry: u32, part: &PartInstance, instance: Instance) -> usize {
        let t = match self.free.pop() {
            Some(t) => t,
            None => {
                self.slots.push(None);
                self.is_moved.push(false);
                self.seen.push(false);
                self.in_order.push(false);
                self.distance.push(0.0);
                self.slots.len() - 1
            }
        };
        self.set(t, entry, part, instance);
        t
    }

    pub(crate) fn set(&mut self, t: usize, entry: u32, part: &PartInstance, instance: Instance) {
        self.slots[t] = Some(Entry::new(entry, part, instance));
        self.relook = true;
    }

    /// The part in slot `t` moved to where `part` is, its look the same.
    pub(crate) fn moved(&mut self, t: usize, part: &PartInstance) {
        if let Some(e) = &mut self.slots[t] {
            e.position = part.position;
            e.instance = e.instance.placed(part);
            e.world = e.instance.world_box();
            if !std::mem::replace(&mut self.is_moved[t], true) {
                self.moved.push(t);
            }
        }
    }

    pub(crate) fn vacate(&mut self, t: usize) {
        self.slots[t] = None;
        self.free.push(t);
        self.relook = true;
    }

    pub(crate) fn buffer(&self) -> Option<&wgpu::Buffer> {
        self.buffer.as_ref().map(|(b, _)| b)
    }

    /// Each of the last upload's instances' squared distance from the
    /// camera, by instance.
    pub(crate) fn distances(&self) -> &[f32] {
        &self.distances
    }

    /// Each of the last upload's instances' scene entry and box.
    pub(crate) fn picks(&self) -> &[(u32, Aabb)] {
        &self.picks
    }

    /// Upload the parts `view` reaches, sorted back to front for `camera`
    /// (terrain water before the rest, an image on a transparent part just
    /// after its part, and those on top after the rest), unless
    /// neither they nor the view changed since the last upload. Returns
    /// the draws: (key, pass, instance range) runs in that order.
    ///
    /// The last order is sorted again rather than made anew: a camera or
    /// a few parts moving leave it nearly sorted, which the stable sort
    /// takes in about one pass. When the order comes out the same, only
    /// the moved parts' instances are written.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &Camera,
        view: &Frustum,
        margin: f32,
    ) -> Vec<TransparentRun> {
        let drawn_for = (*view, camera.position, margin);
        let view_changed = self.drawn_for != Some(drawn_for);
        if !view_changed && !self.relook && self.moved.is_empty() {
            return self.runs.clone();
        }
        profiling::scope!("transparent");
        self.drawn_for = Some(drawn_for);
        let relook = std::mem::take(&mut self.relook);
        let moved = std::mem::take(&mut self.moved);
        self.reorder(camera, view, margin, view_changed || relook, &moved);
        let same = !relook
            && self.order.len() == self.previous.len()
            && (self.order.iter().zip(&self.previous)).all(|(a, b)| a.slot == b.slot);
        if same {
            self.follow(queue, !moved.is_empty());
        } else {
            self.rebuild(device, queue);
        }
        for t in moved {
            self.is_moved[t] = false;
        }
        self.runs.clone()
    }

    /// Put the slots `view` reaches in drawing order for `camera`, from
    /// the last order (kept in `previous`): what went out of view leaves
    /// it, what came in joins it, and it's sorted again. Every slot is
    /// looked at for a new view (`all`), else only the `moved` ones.
    fn reorder(
        &mut self,
        camera: &Camera,
        view: &Frustum,
        margin: f32,
        all: bool,
        moved: &[usize],
    ) {
        let slots = &self.slots;
        let sees = |t: usize| {
            slots[t]
                .as_ref()
                .is_some_and(|e| view.sees(e.world, margin))
        };
        if all {
            for (t, seen) in self.seen.iter_mut().enumerate() {
                *seen = sees(t);
            }
        } else {
            for &t in moved {
                self.seen[t] = sees(t);
            }
        }
        let ordered = |t: usize| {
            let e = slots[t].as_ref().expect("seen slots are filled");
            Ordered {
                rank: e.rank(),
                distance: e.position.distance_squared(camera.position),
                image: e.image,
                slot: t,
            }
        };
        std::mem::swap(&mut self.order, &mut self.previous);
        self.order.clear();
        for o in &self.previous {
            self.in_order[o.slot] = false;
        }
        for o in &self.previous {
            if self.seen[o.slot] && !self.in_order[o.slot] {
                self.in_order[o.slot] = true;
                self.order.push(ordered(o.slot));
            }
        }
        let mut join = |t: usize| {
            if self.seen[t] && !self.in_order[t] {
                self.in_order[t] = true;
                self.order.push(ordered(t));
            }
        };
        if all {
            (0..slots.len()).for_each(&mut join);
        } else {
            moved.iter().copied().for_each(&mut join);
        }
        // Water first, writing depth: Studio draws see-through parts
        // above water over it and hides those under it (measured;
        // docs/behavior/terrain/water.md#see-through-parts). A water chunk
        // is too big to sort against parts by one point anyway.
        for o in &self.order {
            self.distance[o.slot] = o.distance;
        }
        self.order.sort_by(|a, b| {
            (a.rank.cmp(&b.rank))
                .then(b.distance.total_cmp(&a.distance))
                .then(a.image.cmp(&b.image))
        });
    }

    /// The same order as last time: the distances move with the camera,
    /// and the moved slots' instances (if `any_moved`) are written where
    /// they are.
    fn follow(&mut self, queue: &wgpu::Queue, any_moved: bool) {
        let slots = &self.slots;
        let entry = |t: usize| slots[t].as_ref().expect("ordered slots are filled");
        for (d, &t) in self.distances.iter_mut().zip(&self.instance_slots) {
            *d = entry(t as usize).shown(self.distance[t as usize]);
        }
        let (true, Some((buffer, _))) = (any_moved, &self.buffer) else {
            return;
        };
        let size = size_of::<Instance>() as u64;
        let mut i = 0;
        while i < self.instance_slots.len() {
            let t = self.instance_slots[i] as usize;
            if !self.is_moved[t] {
                i += 1;
                continue;
            }
            let start = i;
            while i < self.instance_slots.len() && self.is_moved[self.instance_slots[i] as usize] {
                let e = entry(self.instance_slots[i] as usize);
                self.instances[i] = e.instance;
                self.picks[i].1 = e.world;
                i += 1;
            }
            rook_types::count!(RenderSlotsUploaded, i - start);
            let written = bytemuck::cast_slice(&self.instances[start..i]);
            queue.write_buffer(buffer, start as u64 * size, written);
        }
    }

    /// A new order: every instance and run made again and uploaded.
    fn rebuild(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let slots = &self.slots;
        let entry = |t: usize| slots[t].as_ref().expect("ordered slots are filled");
        let mut instances = std::mem::take(&mut self.instances);
        instances.clear();
        self.instance_slots.clear();
        self.runs.clear();
        self.distances.clear();
        self.picks.clear();
        let (runs, instance_slots) = (&mut self.runs, &mut self.instance_slots);
        let (distances, picks) = (&mut self.distances, &mut self.picks);
        let mut push = |t: usize, d: f32, pass: Pass| {
            let e = entry(t);
            let i = instances.len() as u32;
            instances.push(e.instance);
            instance_slots.push(t as u32);
            distances.push(e.shown(d));
            picks.push((e.entry, e.world));
            match runs.last_mut() {
                Some((k, q, r)) if *k == e.key && *q == pass => r.end = i + 1,
                _ => runs.push((e.key, pass, i..i + 1)),
            }
        };
        // Depth first: all the water's, then all the Glass's.
        for o in self.order.iter().filter(|o| entry(o.slot).water) {
            push(o.slot, o.distance, Pass::WaterDepth);
        }
        for o in self.order.iter().filter(|o| entry(o.slot).glass) {
            push(o.slot, o.distance, Pass::GlassDepth);
        }
        for o in &self.order {
            for &pass in entry(o.slot)
                .passes
                .iter()
                .filter(|&&p| p != Pass::GlassDepth)
            {
                push(o.slot, o.distance, pass);
            }
        }
        if !instances.is_empty() {
            if self
                .buffer
                .as_ref()
                .is_none_or(|(_, capacity)| *capacity < instances.len())
            {
                // Room to grow, so a few more parts don't make a new buffer.
                let capacity = instances.len() * 2;
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("rook transparent instances"),
                    size: (capacity * size_of::<Instance>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.buffer = Some((buffer, capacity));
            }
            let (buffer, _) = self.buffer.as_ref().expect("made above");
            rook_types::count!(RenderSlotsUploaded, instances.len());
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&instances));
        }
        self.instances = instances;
    }
}

#[cfg(test)]
mod tests {
    use glam::Mat3;

    use super::*;

    /// Water draws before every other transparent part, even one farther
    /// than its chunk's point, its depth first, and counts as farthest for
    /// the world GUI.
    #[test]
    fn water_draws_before_the_other_transparent_parts() {
        let Ok(r) = Renderer::headless() else {
            return;
        };
        let at = |z: f32, material| PartInstance {
            transparency: 0.5,
            material,
            ..PartInstance::plain(
                DrawShape::Part(PartShape::Block),
                Vec3::new(0.0, 0.0, z),
                Mat3::IDENTITY,
                Vec3::splat(2.0),
            )
        };
        let mut t = TransparentParts::default();
        for (i, p) in [
            at(-10.0, Material::SmoothPlastic),
            at(15.0, Material::Water),
            at(10.0, Material::SmoothPlastic),
        ]
        .iter()
        .enumerate()
        {
            t.occupy(i as u32, p, r.instance(p));
        }
        let camera = Camera::look_at(Vec3::new(0.0, 10.0, 20.0), Vec3::ZERO);
        let view = Frustum::of(camera.view_projection(1.0));
        let runs = t.upload(&r.device, &r.queue, &camera, &view, 0.0);
        let passes: Vec<Pass> = runs.iter().map(|run| run.1).collect();
        assert_eq!(passes, [Pass::WaterDepth, Pass::Water, Pass::Part]);
        assert_eq!(
            t.picks().iter().map(|p| p.0).collect::<Vec<_>>(),
            [1, 1, 0, 2]
        );
        assert_eq!(t.distances()[..2], [f32::INFINITY; 2]);
    }

    /// A camera moving without changing the order uploads nothing but
    /// keeps the distances current; a part moving in place uploads only
    /// its instance; a move that changes the order draws in the new one.
    #[test]
    fn the_last_order_is_kept_while_it_holds() {
        use rook_types::counters::{Counter, take_local};
        let Ok(r) = Renderer::headless() else {
            return;
        };
        let at = |z: f32| PartInstance {
            transparency: 0.5,
            ..PartInstance::plain(
                DrawShape::Part(PartShape::Block),
                Vec3::new(0.0, 0.0, z),
                Mat3::IDENTITY,
                Vec3::splat(2.0),
            )
        };
        let mut t = TransparentParts::default();
        let slots: Vec<usize> = [-10.0, 0.0, 10.0]
            .iter()
            .enumerate()
            .map(|(i, &z)| t.occupy(i as u32, &at(z), r.instance(&at(z))))
            .collect();
        let upload = |t: &mut TransparentParts, from: Vec3| {
            let camera = Camera::look_at(from, Vec3::ZERO);
            let view = Frustum::of(camera.view_projection(1.0));
            take_local();
            t.upload(&r.device, &r.queue, &camera, &view, 0.0);
            take_local()[Counter::RenderSlotsUploaded]
        };
        let entries = |t: &TransparentParts| t.picks().iter().map(|p| p.0).collect::<Vec<_>>();
        assert_eq!(upload(&mut t, Vec3::new(0.0, 10.0, 40.0)), 3);
        assert_eq!(entries(&t), [0, 1, 2], "back to front");
        let far = t.distances()[0];
        assert_eq!(upload(&mut t, Vec3::new(0.0, 10.0, 41.0)), 0);
        assert!(t.distances()[0] > far, "the distances follow the camera");
        t.moved(slots[1], &at(1.0));
        assert_eq!(upload(&mut t, Vec3::new(0.0, 10.0, 41.0)), 1);
        assert_eq!(t.picks()[1].1, r.instance(&at(1.0)).world_box());
        t.moved(slots[0], &at(20.0));
        assert_eq!(upload(&mut t, Vec3::new(0.0, 10.0, 41.0)), 3);
        assert_eq!(entries(&t), [1, 2, 0], "the moved part is nearest now");
    }
}
