//! Transparent parts: kept with their instances, and drawn back to front.
//! The sort and its upload are only done again when a part or the view
//! changed.

use std::ops::Range;

use super::*;
use runs::Pipe;

/// How a run of transparent instances is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pass {
    Part,
    /// Terrain water, seen from either side.
    Water,
    Image,
    /// See-through Glass: its nearest depth, then its filter on what's
    /// behind, then its surface.
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
            key: BatchKey {
                shape: part.shape,
                images: image_set(part),
            },
            passes: Pass::of(part),
            instance,
            world: instance.world_box(),
        }
    }
}

/// What the last upload was sorted and culled for: the view, where the
/// camera was, and how far boxes reach out.
type DrawnFor = (Frustum, Vec3, f32);

#[derive(Debug, Default)]
pub(crate) struct TransparentParts {
    /// By slot; `None` when free.
    slots: Vec<Option<Entry>>,
    free: Vec<usize>,
    /// A slot changed since the last upload.
    changed: bool,
    drawn_for: Option<DrawnFor>,
    runs: Vec<TransparentRun>,
    /// Each sorted instance's squared distance from the camera (those on
    /// top nearer than anything).
    distances: Vec<f32>,
    /// Each sorted instance's scene entry and box in the world.
    picks: Vec<(u32, Aabb)>,
    /// The sorted instances, and how many it holds.
    buffer: Option<(wgpu::Buffer, usize)>,
    /// The last upload's slots in drawing order, with their squared
    /// distances, and its instances: kept to fill again without
    /// allocating each frame.
    order: Vec<(f32, usize)>,
    instances: Vec<Instance>,
}

impl TransparentParts {
    pub(crate) fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
        self.changed = true;
    }

    /// Take a slot for scene entry `entry`, drawn as `part`.
    pub(crate) fn occupy(&mut self, entry: u32, part: &PartInstance, instance: Instance) -> usize {
        let t = match self.free.pop() {
            Some(t) => t,
            None => {
                self.slots.push(None);
                self.slots.len() - 1
            }
        };
        self.set(t, entry, part, instance);
        t
    }

    pub(crate) fn set(&mut self, t: usize, entry: u32, part: &PartInstance, instance: Instance) {
        self.slots[t] = Some(Entry::new(entry, part, instance));
        self.changed = true;
    }

    /// The part in slot `t` moved to where `part` is, its look the same.
    pub(crate) fn moved(&mut self, t: usize, part: &PartInstance) {
        if let Some(e) = &mut self.slots[t] {
            e.position = part.position;
            e.instance = e.instance.placed(part);
            e.world = e.instance.world_box();
            self.changed = true;
        }
    }

    pub(crate) fn vacate(&mut self, t: usize) {
        self.slots[t] = None;
        self.free.push(t);
        self.changed = true;
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
    /// (an image on a transparent part just after its part, and those on
    /// top after the rest), unless
    /// neither they nor the view changed since the last upload. Returns
    /// the draws: (key, pass, instance range) runs in that order.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &Camera,
        view: &Frustum,
        margin: f32,
    ) -> Vec<TransparentRun> {
        let drawn_for = (*view, camera.position, margin);
        if !self.changed && self.drawn_for == Some(drawn_for) {
            return self.runs.clone();
        }
        profiling::scope!("transparent");
        self.changed = false;
        self.drawn_for = Some(drawn_for);
        // What the view reaches, back to front for blending.
        let mut order = std::mem::take(&mut self.order);
        order.clear();
        order.extend(
            (self.slots.iter().enumerate())
                .filter_map(|(t, e)| Some((t, e.as_ref()?)))
                .filter(|(_, e)| view.sees(e.world, margin))
                .map(|(t, e)| (e.position.distance_squared(camera.position), t)),
        );
        let slots = &self.slots;
        let entry = |t: usize| slots[t].as_ref().expect("filled above");
        order.sort_by(|&(da, a), &(db, b)| {
            let (a, b) = (entry(a), entry(b));
            a.on_top
                .cmp(&b.on_top)
                .then(db.total_cmp(&da))
                .then(a.image.cmp(&b.image))
        });
        let mut instances = std::mem::take(&mut self.instances);
        instances.clear();
        let mut runs: Vec<TransparentRun> = Vec::new();
        self.distances.clear();
        self.picks.clear();
        for &(d, t) in &order {
            let e = entry(t);
            let d = if e.on_top { f32::NEG_INFINITY } else { d };
            for &pass in e.passes {
                let i = instances.len() as u32;
                instances.push(e.instance);
                self.distances.push(d);
                self.picks.push((e.entry, e.world));
                match runs.last_mut() {
                    Some((k, q, r)) if *k == e.key && *q == pass => r.end = i + 1,
                    _ => runs.push((e.key, pass, i..i + 1)),
                }
            }
        }
        self.order = order;
        self.runs.clone_from(&runs);
        if !instances.is_empty() {
            if self.buffer.as_ref().is_none_or(|(_, capacity)| *capacity < instances.len()) {
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
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(&instances));
        }
        self.instances = instances;
        runs
    }
}
