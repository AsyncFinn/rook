//! Scatter upload, as Unreal's GPU scene does it: the instances that
//! changed this frame go up packed with their slots, in one write, and a
//! compute pass recorded in the frame's own commands
//! (`shaders/scatter.wgsl`) copies each into its slot. A
//! frame of parts moving all over the place then uploads what moved,
//! not every chunk of the buffer holding something that moved.

use wesl_core::include_wesl;

use super::*;

/// Instances per workgroup (`shaders/scatter.wgsl`).
const GROUP: u32 = 64;

/// The scatter pass, shared by every instance pool.
#[derive(Debug)]
pub(crate) struct Scatter {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    /// The biggest buffer the pass may write; a pool bigger than this
    /// uploads its chunks instead.
    max_binding: u64,
}

impl Scatter {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let entry = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook scatter"),
            entries: &[entry(0, true), entry(1, false)],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook scatter"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("scatter").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook scatter"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let words = (std::mem::size_of::<Instance>() / 4) as f64;
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rook scatter"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("scatter"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("WORDS", words)],
                ..Default::default()
            },
            cache: None,
        });
        Self {
            pipeline,
            layout,
            max_binding: device.limits().max_storage_buffer_binding_size,
        }
    }

    /// Whether the pass can write a buffer of `bytes`.
    pub(super) fn reaches(&self, bytes: u64) -> bool {
        bytes <= self.max_binding
    }

    /// Copy `instances` at `slots` into `target` in `encoder`'s commands,
    /// uploading them packed into `staging` (grown as needed). Writes
    /// queued to `target` before those commands are submitted land before
    /// this copy; they write the same instances.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        staging: &mut Option<wgpu::Buffer>,
        target: &wgpu::Buffer,
        slots: &[u32],
        instances: &[Instance],
    ) {
        let count = slots.len() as u32;
        let mut words: Vec<u32> =
            Vec::with_capacity(1 + slots.len() * (1 + std::mem::size_of::<Instance>() / 4));
        words.push(count);
        for &slot in slots {
            words.push(slot);
            words.extend_from_slice(bytemuck::cast_slice(std::slice::from_ref(
                &instances[slot as usize],
            )));
        }
        let bytes = (words.len() * 4) as u64;
        if staging.as_ref().is_none_or(|b| b.size() < bytes) {
            *staging = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rook scatter"),
                size: bytes.next_power_of_two(),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let staging = staging.as_ref().expect("made above");
        queue.write_buffer(staging, 0, bytemuck::cast_slice(&words));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook scatter"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: staging.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: target.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rook scatter"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(count.div_ceil(GROUP), 1, 1);
    }
}
