//! Instanced draws grouped by mesh and image, their instances sharing one
//! buffer.

use std::ops::Range;

use super::*;
use pool::RangeAlloc;

/// What an instanced draw shares: its mesh and its images (a mesh
/// texture or a layer's decal, and the material's maps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BatchKey {
    pub(crate) shape: DrawShape,
    pub(crate) images: renderer::ImageSet,
}

/// Instances of one key, drawn instanced from a run of slots in the
/// shared buffer. An instance keeps its slot while it draws this key; a
/// slot it leaves holds a zeroed instance (which draws nothing) until
/// another takes it.
#[derive(Debug)]
pub(crate) struct Batch {
    pub(crate) key: BatchKey,
    /// Its run of slots in the shared buffer, and how many are in use.
    region: Range<u32>,
    len: u32,
    free: Vec<u32>,
    /// A box around every instance in the world. A changed instance
    /// widens it; it's worked out afresh (tight again) once as many
    /// instances have changed as the batch has, so a few parts moving in
    /// a batch of thousands cost little.
    bounds: Aabb,
    changes: u32,
}

/// Instances are written to the GPU in chunks of this many.
const CHUNK: u32 = 64;

/// Every batch's instances, kept on the CPU and copied to one GPU buffer.
#[derive(Debug, Default)]
struct InstancePool {
    instances: Vec<Instance>,
    ranges: RangeAlloc,
    /// The GPU copy, as big as `instances` once uploaded.
    buffer: Option<wgpu::Buffer>,
    /// Chunks changed since they were uploaded.
    dirty: Vec<bool>,
    any_dirty: bool,
}

impl InstancePool {
    /// A run of `len` slots, the pool growing if it must.
    fn alloc(&mut self, len: u32) -> Range<u32> {
        let start = match self.ranges.alloc(len, self.instances.len() as u32) {
            Some(start) => start,
            None => {
                let capacity =
                    (self.instances.len() * 2).max(self.ranges.top() as usize + len as usize);
                self.instances.resize(capacity, Instance::zeroed());
                // A new buffer, written whole.
                self.buffer = None;
                self.ranges.alloc(len, capacity as u32).expect("grown")
            }
        };
        start..start + len
    }

    fn set(&mut self, at: u32, instance: Instance) {
        self.instances[at as usize] = instance;
        self.mark(at..at + 1);
    }

    fn mark(&mut self, range: Range<u32>) {
        if range.is_empty() {
            return;
        }
        let chunks = (self.instances.len() as u32).div_ceil(CHUNK) as usize;
        self.dirty.resize(chunks, false);
        for c in range.start / CHUNK..=(range.end - 1) / CHUNK {
            self.dirty[c as usize] = true;
        }
        self.any_dirty = true;
    }

    /// Bring the GPU copy up to date: runs of changed chunks, or the whole
    /// buffer when it's new.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let size = std::mem::size_of::<Instance>() as u64;
        let Some(buffer) = &self.buffer else {
            if !self.instances.is_empty() {
                self.buffer = Some(
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rook instances"),
                        contents: bytemuck::cast_slice(&self.instances),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    }),
                );
            }
            self.dirty.fill(false);
            self.any_dirty = false;
            return;
        };
        if !std::mem::take(&mut self.any_dirty) {
            return;
        }
        let mut c = 0;
        while c < self.dirty.len() {
            if !self.dirty[c] {
                c += 1;
                continue;
            }
            let first = c;
            while c < self.dirty.len() && self.dirty[c] {
                self.dirty[c] = false;
                c += 1;
            }
            let start = first * CHUNK as usize;
            let end = (c * CHUNK as usize).min(self.instances.len());
            queue.write_buffer(
                buffer,
                start as u64 * size,
                bytemuck::cast_slice(&self.instances[start..end]),
            );
        }
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
    pool: InstancePool,
}

impl Batches {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Put `instance` in a batch's slot.
    pub(crate) fn set(&mut self, batch: usize, slot: u32, instance: Instance) {
        let b = &mut self.list[batch];
        b.bounds = b.bounds.union(instance.world_box());
        b.changes += 1;
        self.pool.set(b.region.start + slot, instance);
    }

    /// Take a slot for `instance` in the batch of `(z, key)`.
    pub(crate) fn occupy(&mut self, z: i32, key: BatchKey, instance: Instance) -> (usize, u32) {
        let batch = match self.of.get(&(z, key)) {
            Some(&b) => b,
            None => {
                let b = self.list.len();
                self.list.push(Batch {
                    key,
                    region: 0..0,
                    len: 0,
                    free: Vec::new(),
                    bounds: Aabb::EMPTY,
                    changes: 0,
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
                if b.len == b.region.len() as u32 {
                    // Full: move to a run with room to grow.
                    let region = self.pool.alloc(b.len + b.len / 2 + 4);
                    let old = b.region.clone();
                    let (from, to) = (old.start as usize, region.start as usize);
                    self.pool
                        .instances
                        .copy_within(from..from + b.len as usize, to);
                    self.pool.ranges.free(old);
                    self.pool.mark(region.start..region.start + b.len);
                    b.region = region;
                }
                b.len += 1;
                b.len - 1
            }
        };
        self.set(batch, slot, instance);
        (batch, slot)
    }

    pub(crate) fn vacate(&mut self, batch: usize, slot: u32) {
        self.set(batch, slot, Instance::zeroed());
        self.list[batch].free.push(slot);
    }

    /// Bring the GPU copy of the instances and the batches' bounds up to
    /// date.
    pub(crate) fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        for b in &mut self.list {
            if b.changes >= b.len.max(1) {
                let start = b.region.start as usize;
                b.bounds = self.pool.instances[start..start + b.len as usize]
                    .iter()
                    .fold(Aabb::EMPTY, |all, i| all.union(i.world_box()));
                b.changes = 0;
            }
        }
        self.pool.upload(device, queue);
    }

    /// The buffer every batch's instances are in (none before the first
    /// upload).
    pub(crate) fn buffer(&self) -> Option<&wgpu::Buffer> {
        self.pool.buffer.as_ref()
    }

    /// Each batch's key, instances in the shared buffer and bounds, in
    /// drawing order.
    pub(crate) fn draws(&self) -> impl Iterator<Item = (&BatchKey, Range<u32>, Aabb)> {
        self.order
            .iter()
            .map(|&i| &self.list[i])
            .filter(|b| b.len > 0)
            .map(|b| {
                let start = b.region.start;
                (&b.key, start..start + b.len, b.bounds)
            })
    }
}
