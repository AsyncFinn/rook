//! Timing the GPU's passes with timestamp queries, for `--bench`. Off
//! until asked for, and only where the adapter has timestamps.

use super::*;

/// The main pass's and shadow pass's start and end (the main pass first,
/// so a frame without shadows resolves from 0, as alignment requires).
const QUERIES: u32 = 4;

#[derive(Debug)]
pub(crate) struct GpuTimer {
    queries: wgpu::QuerySet,
    resolved: wgpu::Buffer,
    readback: wgpu::Buffer,
    /// Whether the last frame had a shadow pass.
    shadowed: bool,
}

impl GpuTimer {
    fn new(device: &wgpu::Device) -> Self {
        let size = u64::from(QUERIES) * 8;
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("rook pass times"),
                ty: wgpu::QueryType::Timestamp,
                count: QUERIES,
            }),
            resolved: buffer(
                "rook pass times",
                wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            ),
            readback: buffer(
                "rook pass times readback",
                wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            ),
            shadowed: false,
        }
    }

    /// A pass's timestamp writes: pass 0 is the main pass, 1 the shadow
    /// pass.
    pub(crate) fn writes(&self, pass: u32) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some(pass * 2),
            end_of_pass_write_index: Some(pass * 2 + 1),
        }
    }

    /// Copy the frame's timestamps out, at the end of its encoder.
    pub(crate) fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder, shadowed: bool) {
        self.shadowed = shadowed;
        let count = if shadowed { QUERIES } else { 2 };
        encoder.resolve_query_set(&self.queries, 0..count, &self.resolved, 0);
        encoder.copy_buffer_to_buffer(&self.resolved, 0, &self.readback, 0, None);
    }
}

/// The GPU's time for a frame's passes, in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuTimes {
    pub shadow: f64,
    pub main: f64,
}

impl Renderer {
    /// Time the GPU's passes from now on, if the adapter can
    /// ([`Renderer::gpu_times`]). Returns whether it can.
    pub fn time_gpu(&mut self) -> bool {
        if self.timer.is_none()
            && self
                .device
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            self.timer = Some(GpuTimer::new(&self.device));
        }
        self.timer.is_some()
    }

    /// How long the GPU took over the last frame's passes; waits for it.
    pub fn gpu_times(&self) -> Option<GpuTimes> {
        let timer = self.timer.as_ref()?;
        let slice = timer.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .ok()?;
        let ticks: Vec<u64> = bytemuck::cast_slice(&slice.get_mapped_range().ok()?).to_vec();
        timer.readback.unmap();
        let ms = f64::from(self.queue.get_timestamp_period()) / 1e6;
        let span = |i: usize| ticks[i * 2 + 1].saturating_sub(ticks[i * 2]) as f64 * ms;
        Some(GpuTimes {
            shadow: if timer.shadowed { span(1) } else { 0.0 },
            main: span(0),
        })
    }
}
