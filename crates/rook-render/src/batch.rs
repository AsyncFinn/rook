//! Instanced draws grouped by mesh and image.

use super::*;

/// What an instanced draw shares: its mesh and its images (a mesh
/// texture or a layer's decal, and the material's maps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BatchKey {
    pub(crate) shape: DrawShape,
    pub(crate) images: renderer::ImageSet,
}

/// Instances of one key, drawn instanced. An instance keeps its slot while
/// it draws this key; a slot it leaves holds a zeroed instance (which
/// draws nothing) until another takes it.
#[derive(Debug)]
pub(crate) struct Batch {
    pub(crate) key: BatchKey,
    instances: Vec<Instance>,
    free: Vec<usize>,
    /// The GPU copy and how many instances it has room for (made or grown
    /// when drawn).
    buffer: Option<(wgpu::Buffer, usize)>,
    /// Instances changed since they were last uploaded.
    dirty: Option<std::ops::Range<usize>>,
}

impl Batch {
    pub(crate) fn set(&mut self, slot: usize, instance: Instance) {
        self.instances[slot] = instance;
        self.dirty = Some(match self.dirty.take() {
            Some(r) => r.start.min(slot)..r.end.max(slot + 1),
            None => slot..slot + 1,
        });
    }
}

/// A set of batches, drawn in order of their `z` (`ZIndex` for image
/// layers, 0 for parts), then in the order they were made, so the draw
/// order is the same every run.
#[derive(Debug, Default)]
pub(crate) struct Batches {
    pub(crate) list: Vec<Batch>,
    of: HashMap<(i32, BatchKey), usize>,
    /// Indices into `list` in drawing order.
    order: Vec<usize>,
    /// Each batch's z.
    pub(crate) z: Vec<i32>,
}

impl Batches {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Take a slot for `instance` in the batch of `(z, key)`.
    pub(crate) fn occupy(&mut self, z: i32, key: BatchKey, instance: Instance) -> (usize, usize) {
        let batch = match self.of.get(&(z, key)) {
            Some(&b) => b,
            None => {
                let b = self.list.len();
                self.list.push(Batch {
                    key,
                    instances: Vec::new(),
                    free: Vec::new(),
                    buffer: None,
                    dirty: None,
                });
                self.z.push(z);
                self.of.insert((z, key), b);
                let at = self.order.partition_point(|&o| self.z[o] <= z);
                self.order.insert(at, b);
                b
            }
        };
        let b = &mut self.list[batch];
        let slot = match b.free.pop() {
            Some(slot) => slot,
            None => {
                b.instances.push(Instance::zeroed());
                b.instances.len() - 1
            }
        };
        b.set(slot, instance);
        (batch, slot)
    }

    pub(crate) fn vacate(&mut self, batch: usize, slot: usize) {
        let b = &mut self.list[batch];
        b.set(slot, Instance::zeroed());
        b.free.push(slot);
    }

    /// Bring the batches' GPU copies up to date.
    pub(crate) fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let size = std::mem::size_of::<Instance>();
        for b in &mut self.list {
            let len = b.instances.len();
            match &b.buffer {
                Some((buffer, capacity)) if *capacity >= len => {
                    if let Some(r) = b.dirty.take() {
                        queue.write_buffer(
                            buffer,
                            (r.start * size) as u64,
                            bytemuck::cast_slice(&b.instances[r]),
                        );
                    }
                }
                // New, or grown past its buffer: room to grow again.
                _ => {
                    let capacity = len + len / 2 + 16;
                    b.buffer = Some((instance_buffer(device, &b.instances, capacity), capacity));
                    b.dirty = None;
                }
            }
        }
    }

    /// (key, buffer, instances) in drawing order.
    pub(crate) fn draws(&self) -> Vec<(BatchKey, &wgpu::Buffer, std::ops::Range<u32>)> {
        self.order
            .iter()
            .filter_map(|&i| {
                let b = &self.list[i];
                let (buffer, _) = b.buffer.as_ref()?;
                Some((b.key, buffer, 0..b.instances.len() as u32))
            })
            .collect()
    }
}

/// A buffer holding `list` with room for `capacity` instances.
pub(crate) fn instance_buffer(
    device: &wgpu::Device,
    list: &[Instance],
    capacity: usize,
) -> wgpu::Buffer {
    let mut padded = list.to_vec();
    padded.resize(capacity.max(list.len()), Instance::zeroed());
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rook instances"),
        contents: bytemuck::cast_slice(&padded),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}
