//! A batch's instance buffer: ranges handed out in chunks, written slot
//! by slot and uploaded as the dirty runs merged.

use super::*;

/// Every batch's instances, kept on the CPU and copied to one GPU buffer.
#[derive(Debug, Default)]
pub(super) struct InstancePool {
    pub(super) instances: Vec<Instance>,
    pub(super) ranges: RangeAlloc,
    /// The GPU copy, as big as `instances` once uploaded.
    pub(super) buffer: Option<wgpu::Buffer>,
    /// The GPU copy from before the pool last grew, copied into the new
    /// one at the next upload, so growing writes only what changed.
    pub(super) outgrown: Option<wgpu::Buffer>,
    /// Chunks changed since they were uploaded: runs of slots a batch
    /// took or moved to.
    pub(super) dirty: Vec<bool>,
    pub(super) any_dirty: bool,
    /// Single slots changed since they were uploaded (an instance placed
    /// or moved), scattered into place ([`Scatter`]), and which are.
    pub(super) changed: Vec<u32>,
    pub(super) is_changed: Vec<bool>,
    /// Where the changed slots go up.
    pub(super) staging: Option<wgpu::Buffer>,
}

impl InstancePool {
    /// A run of `len` slots, the pool growing if it must.
    pub(super) fn alloc(&mut self, len: u32) -> Range<u32> {
        let start = match self.ranges.alloc(len, self.instances.len() as u32) {
            Some(start) => start,
            None => {
                let capacity =
                    (self.instances.len() * 2).max(self.ranges.top() as usize + len as usize);
                self.instances.resize(capacity, Instance::zeroed());
                self.is_changed.resize(capacity, false);
                // A new buffer, which takes the old one's contents on the
                // GPU (Vec's doubling, with a buffer copy for the move).
                if let Some(old) = self.buffer.take() {
                    self.outgrown.get_or_insert(old);
                }
                self.ranges.alloc(len, capacity as u32).expect("grown")
            }
        };
        start..start + len
    }

    pub(super) fn set(&mut self, at: u32, instance: Instance) {
        self.instances[at as usize] = instance;
        if !std::mem::replace(&mut self.is_changed[at as usize], true) {
            self.changed.push(at);
        }
    }

    pub(super) fn mark(&mut self, range: Range<u32>) {
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

    /// Bring the GPU copy up to date: the whole buffer when it's the first,
    /// else (after copying what an outgrown one held) runs of changed
    /// chunks, then the single slots changed.
    pub(super) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scatter: &Scatter,
    ) {
        let size = std::mem::size_of::<Instance>() as u64;
        if self.buffer.is_none()
            && let Some(old) = self.outgrown.take()
        {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rook instances"),
                size: self.instances.len() as u64 * size,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_buffer_to_buffer(&old, 0, &buffer, 0, old.size());
            // Submitted now: writes queued after it land after the copy.
            queue.submit([encoder.finish()]);
            // Slots past the old size not written since are zero, as a new
            // buffer is.
            self.buffer = Some(buffer);
        }
        if self
            .buffer
            .as_ref()
            .is_some_and(|b| !scatter.reaches(b.size()))
        {
            for at in std::mem::take(&mut self.changed) {
                self.is_changed[at as usize] = false;
                self.mark(at..at + 1);
            }
        }
        let Some(buffer) = &self.buffer else {
            if !self.instances.is_empty() {
                rook_types::count!(RenderSlotsUploaded, self.instances.len());
                rook_types::count!(RenderBytesUploaded, self.instances.len() as u64 * size);
                self.buffer = Some(
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rook instances"),
                        contents: bytemuck::cast_slice(&self.instances),
                        usage: wgpu::BufferUsages::VERTEX
                            | wgpu::BufferUsages::COPY_DST
                            | wgpu::BufferUsages::COPY_SRC
                            | wgpu::BufferUsages::STORAGE,
                    }),
                );
            }
            self.dirty.fill(false);
            self.any_dirty = false;
            for at in self.changed.drain(..) {
                self.is_changed[at as usize] = false;
            }
            return;
        };
        if !self.changed.is_empty() {
            rook_types::count!(RenderSlotsUploaded, self.changed.len());
            rook_types::count!(RenderBytesUploaded, self.changed.len() as u64 * size);
            scatter.run(
                device,
                queue,
                encoder,
                &mut self.staging,
                buffer,
                &self.changed,
                &self.instances,
            );
            for &at in &self.changed {
                self.is_changed[at as usize] = false;
            }
            self.changed.clear();
        }
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
            rook_types::count!(RenderSlotsUploaded, end - start);
            rook_types::count!(RenderBytesUploaded, (end - start) as u64 * size);
            queue.write_buffer(
                buffer,
                start as u64 * size,
                bytemuck::cast_slice(&self.instances[start..end]),
            );
        }
    }
}
