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
    Image,
    /// See-through Glass: its nearest depth, then its filter on what's
    /// behind, then its surface.
    GlassDepth,
    GlassFilter,
    GlassSurface,
    ForceField,
}

impl Pass {
    pub(crate) fn pipe(self) -> Pipe {
        match self {
            Pass::Part => Pipe::Transparent,
            Pass::Image => Pipe::Layer,
            Pass::GlassDepth => Pipe::GlassDepth,
            Pass::GlassFilter => Pipe::GlassFilter,
            Pass::GlassSurface => Pipe::GlassSurface,
            Pass::ForceField => Pipe::Additive,
        }
    }

    /// The passes an entry is drawn in, in order.
    fn of(p: &PartInstance) -> &'static [Pass] {
        match (p.layer, p.material) {
            (Layer::Image { .. }, _) => &[Pass::Image],
            (Layer::Base, Material::Glass) => {
                &[Pass::GlassDepth, Pass::GlassFilter, Pass::GlassSurface]
            }
            (Layer::Base, Material::ForceField) => &[Pass::ForceField],
            (Layer::Base, _) => &[Pass::Part],
        }
    }
}

/// A run of the frame's transparent instances drawn alike.
pub(crate) type TransparentRun = (BatchKey, Pass, Range<u32>);

/// A transparent part, with what drawing it needs.
#[derive(Debug, Clone, Copy)]
struct Entry {
    position: Vec3,
    /// An image layer (drawn just after its part at the same distance).
    image: bool,
    key: BatchKey,
    passes: &'static [Pass],
    instance: Instance,
    world: Aabb,
}

impl Entry {
    fn new(part: &PartInstance, instance: Instance) -> Self {
        Self {
            position: part.position,
            image: part.layer != Layer::Base,
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
    /// The sorted instances, and how many it holds.
    buffer: Option<(wgpu::Buffer, usize)>,
}

impl TransparentParts {
    pub(crate) fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
        self.changed = true;
    }

    /// Take a slot for `part`.
    pub(crate) fn occupy(&mut self, part: &PartInstance, instance: Instance) -> usize {
        let t = match self.free.pop() {
            Some(t) => t,
            None => {
                self.slots.push(None);
                self.slots.len() - 1
            }
        };
        self.set(t, part, instance);
        t
    }

    pub(crate) fn set(&mut self, t: usize, part: &PartInstance, instance: Instance) {
        self.slots[t] = Some(Entry::new(part, instance));
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

    /// Upload the parts `view` reaches, sorted back to front for `camera`
    /// (an image on a transparent part just after its part), unless
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
        let mut order: Vec<(f32, &Entry)> = self
            .slots
            .iter()
            .flatten()
            .filter(|e| view.sees(e.world, margin))
            .map(|e| (e.position.distance_squared(camera.position), e))
            .collect();
        order.sort_by(|(da, a), (db, b)| db.total_cmp(da).then(a.image.cmp(&b.image)));
        let mut instances: Vec<Instance> = Vec::with_capacity(order.len());
        let mut runs: Vec<TransparentRun> = Vec::new();
        for &(_, e) in &order {
            for &pass in e.passes {
                let i = instances.len() as u32;
                instances.push(e.instance);
                match runs.last_mut() {
                    Some((k, q, r)) if *k == e.key && *q == pass => r.end = i + 1,
                    _ => runs.push((e.key, pass, i..i + 1)),
                }
            }
        }
        self.runs.clone_from(&runs);
        if instances.is_empty() {
            return runs;
        }
        match &self.buffer {
            Some((buffer, capacity)) if *capacity >= instances.len() => {
                queue.write_buffer(buffer, 0, bytemuck::cast_slice(&instances));
            }
            _ => {
                // Room to grow, so a few more parts don't make a new buffer.
                let capacity = instances.len() * 2;
                let mut padded = instances;
                padded.resize(capacity, Instance::zeroed());
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("rook transparent instances"),
                    contents: bytemuck::cast_slice(&padded),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
                self.buffer = Some((buffer, capacity));
            }
        }
        runs
    }
}
