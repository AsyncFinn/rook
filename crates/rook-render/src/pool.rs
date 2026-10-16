//! Every mesh's vertices and indices in a few shared buffers, so draws of
//! different meshes bind nothing new and one multi-draw can cover them.
//!
//! The buffers come in pages: a page holds positions, the other vertex
//! attributes and indices, each in its own buffer, and grows (copying what
//! it holds) up to the device's biggest buffer (at most [`PAGE_BYTES`]),
//! after which meshes go in a new page. Draws change page only between
//! runs, so one page for a whole place is best: an 86k-part place's terrain alone is
//! 6.6M vertices and 36M indices. Positions are a stream of their own so
//! the shadow pass reads 12 bytes a vertex instead of 48.

use std::ops::Range;

use super::*;

/// The most a page's buffer grows to, where the device allows it.
const PAGE_BYTES: u64 = 1 << 30;
/// A new page's room, in vertices and indices.
const FIRST_VERTICES: u32 = 1 << 16;
const FIRST_INDICES: u32 = 1 << 18;

/// Ranges of `0..capacity` handed out first fit; freed ranges merge with
/// their neighbours.
#[derive(Debug, Default)]
pub(crate) struct RangeAlloc {
    /// Free ranges below `top`, sorted and apart.
    free: Vec<Range<u32>>,
    /// Everything from here up is free.
    top: u32,
}

impl RangeAlloc {
    /// Where `len` units fit below `capacity`, if they do.
    pub(crate) fn alloc(&mut self, len: u32, capacity: u32) -> Option<u32> {
        if let Some(i) = self.free.iter().position(|r| r.len() as u32 >= len) {
            let start = self.free[i].start;
            self.free[i].start += len;
            if self.free[i].is_empty() {
                self.free.remove(i);
            }
            return Some(start);
        }
        let start = self.top;
        (start.checked_add(len)? <= capacity).then(|| {
            self.top += len;
            start
        })
    }

    pub(crate) fn free(&mut self, range: Range<u32>) {
        if range.is_empty() {
            return;
        }
        let at = self.free.partition_point(|r| r.start < range.start);
        self.free.insert(at, range);
        // Merge with the next, then the previous.
        if at + 1 < self.free.len() && self.free[at].end == self.free[at + 1].start {
            self.free[at].end = self.free.remove(at + 1).end;
        }
        let at = if at > 0 && self.free[at - 1].end == self.free[at].start {
            self.free[at - 1].end = self.free.remove(at).end;
            at - 1
        } else {
            at
        };
        if self.free[at].end == self.top {
            self.top = self.free.remove(at).start;
        }
    }

    /// One past the last unit handed out.
    pub(crate) fn top(&self) -> u32 {
        self.top
    }
}

/// What a vertex carries besides its position (see [`mesh::Vertex`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct VertexAttributes {
    pub(crate) normal: [f32; 3],
    pub(crate) uv: [f32; 2],
    pub(crate) box_point: [f32; 3],
    pub(crate) face: u32,
}

/// A mesh in the pool: its page, its vertices and its indices (which
/// count from its first vertex).
#[derive(Debug, Clone)]
pub(crate) struct GpuMesh {
    pub(crate) page: usize,
    pub(crate) vertices: Range<u32>,
    pub(crate) indices: Range<u32>,
    /// The mesh's bounds in its own units.
    pub(crate) bounds: (Vec3, Vec3),
}

/// One page's buffers.
#[derive(Debug)]
pub(crate) struct Page {
    pub(crate) positions: wgpu::Buffer,
    pub(crate) attributes: wgpu::Buffer,
    pub(crate) indices: wgpu::Buffer,
    vertex_capacity: u32,
    index_capacity: u32,
    vertex_ranges: RangeAlloc,
    index_ranges: RangeAlloc,
}

/// The size of each stream's element.
const POSITION: u64 = 12;
const ATTRIBUTES: u64 = std::mem::size_of::<VertexAttributes>() as u64;
const INDEX: u64 = 4;

impl Page {
    fn new(device: &wgpu::Device, vertex_capacity: u32, index_capacity: u32) -> Self {
        let (positions, attributes, indices) = buffers(device, vertex_capacity, index_capacity);
        Self {
            positions,
            attributes,
            indices,
            vertex_capacity,
            index_capacity,
            vertex_ranges: RangeAlloc::default(),
            index_ranges: RangeAlloc::default(),
        }
    }

    /// Room for `vertices` and `indices`, growing the buffers if they can.
    fn alloc(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: u32,
        indices: u32,
    ) -> Option<(u32, u32)> {
        let max_vertices = (page_bytes(device) / ATTRIBUTES) as u32;
        let max_indices = (page_bytes(device) / INDEX) as u32;
        let need = |ranges: &RangeAlloc, len: u32, capacity: u32, max: u32| {
            let fits =
                ranges.free.iter().any(|r| r.len() as u32 >= len) || ranges.top + len <= capacity;
            match fits {
                true => Some(capacity),
                false if ranges.top + len <= max => {
                    Some((capacity * 2).max(ranges.top + len).min(max))
                }
                false => None,
            }
        };
        let vertex_capacity = need(
            &self.vertex_ranges,
            vertices,
            self.vertex_capacity,
            max_vertices,
        )?;
        let index_capacity = need(
            &self.index_ranges,
            indices,
            self.index_capacity,
            max_indices,
        )?;
        if (vertex_capacity, index_capacity) != (self.vertex_capacity, self.index_capacity) {
            self.grow(device, queue, vertex_capacity, index_capacity);
        }
        let v = self.vertex_ranges.alloc(vertices, self.vertex_capacity)?;
        let i = self.index_ranges.alloc(indices, self.index_capacity)?;
        Some((v, i))
    }

    /// New, bigger buffers holding what the old ones held.
    fn grow(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertex_capacity: u32,
        index_capacity: u32,
    ) {
        profiling::scope!("MeshPool::grow");
        let (positions, attributes, indices) = buffers(device, vertex_capacity, index_capacity);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rook mesh pool growth"),
        });
        let vertices = u64::from(self.vertex_ranges.top());
        let used = u64::from(self.index_ranges.top());
        for (from, to, bytes) in [
            (&self.positions, &positions, vertices * POSITION),
            (&self.attributes, &attributes, vertices * ATTRIBUTES),
            (&self.indices, &indices, used * INDEX),
        ] {
            if bytes > 0 {
                encoder.copy_buffer_to_buffer(from, 0, to, 0, bytes);
            }
        }
        // Writes queued before this land in the old buffers first: a queue
        // runs its pending writes before the next submission.
        queue.submit([encoder.finish()]);
        self.positions = positions;
        self.attributes = attributes;
        self.indices = indices;
        self.vertex_capacity = vertex_capacity;
        self.index_capacity = index_capacity;
    }
}

/// The most a page's buffer may hold on `device`.
fn page_bytes(device: &wgpu::Device) -> u64 {
    device.limits().max_buffer_size.min(PAGE_BYTES)
}

fn buffers(
    device: &wgpu::Device,
    vertices: u32,
    indices: u32,
) -> (wgpu::Buffer, wgpu::Buffer, wgpu::Buffer) {
    let buffer = |label, size: u64, usage| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    };
    let vertex = wgpu::BufferUsages::VERTEX;
    (
        buffer(
            "rook mesh positions",
            u64::from(vertices) * POSITION,
            vertex,
        ),
        buffer(
            "rook mesh attributes",
            u64::from(vertices) * ATTRIBUTES,
            vertex,
        ),
        buffer(
            "rook mesh indices",
            u64::from(indices) * INDEX,
            wgpu::BufferUsages::INDEX,
        ),
    )
}

/// The pages, and what uploading into them needs.
#[derive(Debug)]
pub(crate) struct MeshPool {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub(crate) pages: Vec<Page>,
}

impl MeshPool {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            pages: Vec::new(),
        }
    }

    /// Put a mesh in the first page with room for it (a new page if none
    /// has). A mesh too big for any page draws nothing.
    pub(crate) fn add(&mut self, data: &mesh::MeshData) -> GpuMesh {
        let bounds = data.bounds();
        let (vertices, indices) = (data.vertices.len() as u32, data.indices.len() as u32);
        let empty = GpuMesh {
            page: 0,
            vertices: 0..0,
            indices: 0..0,
            bounds,
        };
        let most = page_bytes(&self.device);
        let too_big = u64::from(vertices) * ATTRIBUTES > most || u64::from(indices) * INDEX > most;
        if indices == 0 || too_big {
            return empty;
        }
        let (device, queue) = (&self.device, &self.queue);
        let found = self
            .pages
            .iter_mut()
            .enumerate()
            .find_map(|(p, page)| Some((p, page.alloc(device, queue, vertices, indices)?)));
        let (page, (v, i)) = match found {
            Some(found) => found,
            None => {
                let mut page = Page::new(
                    device,
                    FIRST_VERTICES.max(vertices),
                    FIRST_INDICES.max(indices),
                );
                let Some(at) = page.alloc(device, queue, vertices, indices) else {
                    return empty;
                };
                self.pages.push(page);
                (self.pages.len() - 1, at)
            }
        };
        let positions: Vec<[f32; 3]> = data.vertices.iter().map(|v| v.position).collect();
        let attributes: Vec<VertexAttributes> = data
            .vertices
            .iter()
            .map(|v| VertexAttributes {
                normal: v.normal,
                uv: v.uv,
                box_point: v.box_point,
                face: v.face,
            })
            .collect();
        let p = &self.pages[page];
        let at = |start: u32, size: u64| u64::from(start) * size;
        queue.write_buffer(
            &p.positions,
            at(v, POSITION),
            bytemuck::cast_slice(&positions),
        );
        queue.write_buffer(
            &p.attributes,
            at(v, ATTRIBUTES),
            bytemuck::cast_slice(&attributes),
        );
        queue.write_buffer(
            &p.indices,
            at(i, INDEX),
            bytemuck::cast_slice(&data.indices),
        );
        GpuMesh {
            page,
            vertices: v..v + vertices,
            indices: i..i + indices,
            bounds,
        }
    }

    /// Give a mesh's room back.
    pub(crate) fn remove(&mut self, mesh: &GpuMesh) {
        if let Some(page) = self.pages.get_mut(mesh.page) {
            page.vertex_ranges.free(mesh.vertices.clone());
            page.index_ranges.free(mesh.indices.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_reuse_and_merge() {
        let mut r = RangeAlloc::default();
        let a = r.alloc(10, 100).unwrap();
        let b = r.alloc(10, 100).unwrap();
        let c = r.alloc(10, 100).unwrap();
        assert_eq!((a, b, c), (0, 10, 20));
        assert_eq!(r.alloc(80, 100), None);
        r.free(a..a + 10);
        r.free(b..b + 10);
        // 0..20 merged: a 15 fits there.
        assert_eq!(r.alloc(15, 100), Some(0));
        // Freeing the topmost range lowers the top past free space below.
        r.free(c..c + 10);
        assert_eq!(r.top(), 15);
        r.free(0..15);
        assert_eq!(r.top(), 0);
        assert!(r.free.is_empty());
    }
}
