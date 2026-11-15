//! Recording the world GUI: its pipelines, bind groups and the indirect
//! multi-draws among the transparent parts.

use super::*;

impl WorldGui {
    /// Make the frames' bind group for `image`, once it's on the GPU.
    pub(super) fn bind(
        &mut self,
        device: &wgpu::Device,
        image: Option<ImageKey>,
        (images, cache): (&GuiImages, &GpuCache<ImageHash, Image>),
        lights: &GpuLights,
    ) {
        let Some(frames) = &mut self.frames else {
            return;
        };
        if frames.groups.contains_key(&image) {
            return;
        }
        let Some((view, sampler)) = images.view(image, cache) else {
            return;
        };
        let mut entries = lights.entries().to_vec();
        entries.extend([
            wgpu::BindGroupEntry {
                binding: 4,
                resource: frames.buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&self.glyphs),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(&self.ramps),
            },
        ]);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook world gui"),
            layout: &self.group_layout,
            entries: &entries,
        });
        frames.groups.insert(image, group);
    }

    /// Draw some of the frame's canvases, in the order given: the draws
    /// in a row with one image and pipeline in one call where the device
    /// can.
    pub(crate) fn record(&self, pass: &mut wgpu::RenderPass<'_>, runs: &[WorldDraw]) {
        let (Some(frames), Some(quads)) = (&self.frames, &self.quads) else {
            return;
        };
        let (Some(first), Some(last)) = (runs.first(), runs.last()) else {
            return;
        };
        // The canvases' draws are in the frame's list in the order drawn.
        let start = first.draws.start as usize;
        let draws = &self.draws[start..last.draws.end as usize];
        if draws.is_empty() {
            return;
        }
        pass.set_vertex_buffer(0, quads.quads.slice(..));
        pass.set_vertex_buffer(1, quads.ids.slice(..));
        let mut on_top = None;
        let mut at = start;
        let mut calls = 0;
        for batch in draws.chunk_by(|a, b| (a.1, a.2) == (b.1, b.2)) {
            let (_, image, top) = batch[0];
            if on_top != Some(top) {
                on_top = Some(top);
                pass.set_pipeline(if top { &self.on_top } else { &self.tested });
            }
            let group = (frames.groups.get(&image)).expect("only runs with a group draw");
            pass.set_bind_group(3, group, &[]);
            match &self.indirect {
                Some((buffer, _)) if self.multi_draw => {
                    let offset = (at * std::mem::size_of::<DrawArgs>()) as u64;
                    pass.multi_draw_indirect(buffer, offset, batch.len() as u32);
                    calls += 1;
                }
                _ => {
                    for (a, _, _) in batch {
                        let first = a.first_instance;
                        pass.draw(0..a.vertex_count, first..first + a.instance_count);
                        calls += 1;
                    }
                }
            }
            at += batch.len();
        }
        rook_types::count!(RenderGuiCalls, calls);
    }
}

/// The world GUI's pipelines, depth tested and always on top: the parts'
/// groups 0 to 2 (`layouts`), then `group_layout`; the quads, then the
/// canvas each is on.
pub(super) fn pipelines(
    device: &wgpu::Device,
    layouts: BindLayouts<'_>,
    group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> [wgpu::RenderPipeline; 2] {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rook world gui"),
        bind_group_layouts: &[
            Some(layouts.globals),
            Some(layouts.main),
            Some(layouts.image),
            Some(group_layout),
        ],
        immediate_size: 0,
    });
    let ids = wgpu::vertex_attr_array![11 => Uint32];
    [
        wgpu::CompareFunction::Greater,
        wgpu::CompareFunction::Always,
    ]
    .map(|compare| {
        blended_pipeline(
            device,
            "rook world gui",
            &layout,
            (shader, "vs_world_gui", "fs_world_gui"),
            &[
                GpuQuad::layout(),
                wgpu::VertexBufferLayout {
                    array_stride: ID,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &ids,
                },
            ],
            (FRAME_FORMAT, main_depth_format(device)),
            compare,
        )
    })
}
