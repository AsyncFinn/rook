//! Instanced draws grouped by mesh and image, their instances sharing one
//! buffer.

pub(crate) mod cull;

use std::ops::Range;

use super::*;
use pool::RangeAlloc;
use rayon::prelude::*;

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
    /// A box like [`Batches::bounds`] per run of `CULL_SLOTS` slots, so a pass can
    /// skip the runs out of its view. Widened and worked out afresh with
    /// `bounds`.
    chunks: Vec<Aabb>,
    changes: u32,
}

/// How many slots share a box for culling within a batch.
pub(crate) const CULL_SLOTS: u32 = 16;

/// How many batches, in drawing order, share a box for culling.
const CULL_BATCHES: usize = 32;

/// How many clean chunks between changed ones are written along with
/// them, so scattered changes take a few big writes, not many small ones.
const MERGE_GAP: usize = 8;

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
            // A run goes on over gaps of up to `MERGE_GAP` clean chunks:
            // rewriting them costs less than another write.
            let mut end = c;
            while c < self.dirty.len() && c - end <= MERGE_GAP {
                if self.dirty[c] {
                    self.dirty[c] = false;
                    end = c + 1;
                }
                c += 1;
            }
            c = end;
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
    /// A box around each batch's instances in the world. A changed
    /// instance widens it; it's worked out afresh (tight again) once as
    /// many instances have changed as the batch has, so a few parts moving
    /// in a batch of thousands cost little. Kept apart from `list` so
    /// culling reads only the boxes.
    bounds: Vec<Aabb>,
    /// Batches with changes since their boxes were last worked out.
    changed: Vec<usize>,
    pool: InstancePool,
}

impl Batches {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Put `instance` in a batch's slot.
    pub(crate) fn set(&mut self, batch: usize, slot: u32, instance: Instance) {
        let b = &mut self.list[batch];
        let world = instance.world_box();
        self.bounds[batch] = self.bounds[batch].union(world);
        let c = (slot / CULL_SLOTS) as usize;
        if c >= b.chunks.len() {
            b.chunks.resize(c + 1, Aabb::EMPTY);
        }
        b.chunks[c] = b.chunks[c].union(world);
        if b.changes == 0 {
            self.changed.push(batch);
        }
        b.changes += 1;
        self.pool.set(b.region.start + slot, instance);
    }

    /// The instance in a batch's slot moved to where `part` is, its look
    /// the same.
    pub(crate) fn moved(&mut self, batch: usize, slot: u32, part: &PartInstance) {
        let at = self.list[batch].region.start + slot;
        let instance = self.pool.instances[at as usize].placed(part);
        self.set(batch, slot, instance);
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
                    chunks: Vec::new(),
                    changes: 0,
                });
                self.z.push(z);
                self.bounds.push(Aabb::EMPTY);
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
        let (list, bounds, pool) = (&mut self.list, &mut self.bounds, &self.pool);
        // Each batch changed as many times as it has instances gets its
        // boxes worked out afresh; the others wait for more changes.
        self.changed.retain(|&i| {
            let b = &mut list[i];
            if b.changes < b.len.max(1) {
                return true;
            }
            let start = b.region.start as usize;
            b.chunks = pool.instances[start..start + b.len as usize]
                .chunks(CULL_SLOTS as usize)
                .map(|run| {
                    run.iter()
                        .fold(Aabb::EMPTY, |all, i| all.union(i.world_box()))
                })
                .collect();
            bounds[i] = b.chunks.iter().fold(Aabb::EMPTY, |all, c| all.union(*c));
            b.changes = 0;
            false
        });
        self.pool.upload(device, queue);
    }

    /// The buffer every batch's instances are in (none before the first
    /// upload).
    pub(crate) fn buffer(&self) -> Option<&wgpu::Buffer> {
        self.pool.buffer.as_ref()
    }

    /// Whether `view` sees each batch (any of its box grown by
    /// `margin`), in drawing order. Batches are culled [`CULL_BATCHES`] at
    /// a time first: none of a box the view misses is seen, all of one
    /// wholly inside it is.
    pub(crate) fn seen(&self, view: &Frustum, margin: f32) -> Vec<bool> {
        let mut out = vec![false; self.order.len()];
        out.par_chunks_mut(CULL_BATCHES)
            .zip(self.order.par_chunks(CULL_BATCHES))
            .for_each(|(seen, block)| {
                let all = block
                    .iter()
                    .fold(Aabb::EMPTY, |all, &i| all.union(self.bounds[i]));
                if !view.sees(all, margin) {
                    return;
                }
                let inside = view.holds(all, margin);
                for (&i, seen) in block.iter().zip(seen) {
                    let b = self.bounds[i];
                    *seen = if inside {
                        !b.is_empty()
                    } else {
                        view.sees(b, margin)
                    };
                }
            });
        out
    }

    /// The batch `k`th in drawing order: its key, instances in the shared
    /// buffer, a box per `CULL_SLOTS` of those instances and z.
    pub(crate) fn draw(&self, k: usize) -> (&BatchKey, Range<u32>, &[Aabb], i32) {
        let i = self.order[k];
        let b = &self.list[i];
        let start = b.region.start;
        (&b.key, start..start + b.len, &b.chunks[..], self.z[i])
    }
}
