//! Highlights (`PartInstance::highlight`): the parts each covers are drawn
//! into a mask of Highlight numbers after the main pass, and the tone pass
//! fills and outlines what the mask covers over the finished frame
//! (`shaders/tonemap.wgsl`, docs/behavior/rendering/highlight.md).

use std::collections::BTreeMap;
use std::ops::Range;

use super::*;
use runs::Runs;

/// The most Highlights drawn at once: Roblox draws no more than 31
/// (creator docs, Highlight). The mask holds 1 + their index, 0 for none.
pub(crate) const HIGHLIGHT_LIMIT: usize = 31;

/// The mask's format: a Highlight's number per pixel.
pub(crate) const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Uint;

/// What the tone pass reads per Highlight: its fill and its outline, each
/// sRGB with its opacity.
pub(crate) type Looks = [[f32; 4]; 2 * HIGHLIGHT_LIMIT];

/// A run of the mask's draws: one shape, drawn over everything or only
/// where seen.
pub(crate) type HighlightRun = (BatchKey, bool, Range<u32>);

#[derive(Debug)]
pub(crate) struct HighlightParts {
    /// The highlighted entries, by their index in the scene.
    entries: BTreeMap<usize, (Highlight, BatchKey, Instance)>,
    changed: bool,
    runs: Vec<HighlightRun>,
    looks: Looks,
    /// The mask's instances, and how many it holds.
    buffer: Option<(wgpu::Buffer, usize)>,
}

impl Default for HighlightParts {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            changed: false,
            runs: Vec::new(),
            looks: [[0.0; 4]; 2 * HIGHLIGHT_LIMIT],
            buffer: None,
        }
    }
}

impl HighlightParts {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.changed = true;
    }

    /// Entry `i` is drawn as `instance` (or not at all) with its look.
    pub(crate) fn set(&mut self, i: usize, part: &PartInstance, instance: Option<Instance>) {
        let entry = part.highlight.zip(instance).map(|(h, instance)| {
            let key = BatchKey {
                shape: part.shape,
                images: [None; 5],
            };
            (h, key, instance)
        });
        let had = match entry {
            Some(e) => self.entries.insert(i, e).is_some(),
            None => self.entries.remove(&i).is_some(),
        };
        self.changed |= had || entry.is_some();
    }

    pub(crate) fn moved(&mut self, i: usize, part: &PartInstance) {
        if let Some((_, _, instance)) = self.entries.get_mut(&i) {
            *instance = instance.placed(part);
            self.changed = true;
        }
    }

    pub(crate) fn buffer(&self) -> Option<&wgpu::Buffer> {
        self.buffer.as_ref().map(|(b, _)| b)
    }

    /// Upload the mask's instances, unless nothing changed: the draws,
    /// and each Highlight's look by its number - 1.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (Vec<HighlightRun>, Looks) {
        if !self.changed {
            return (self.runs.clone(), self.looks);
        }
        self.changed = false;
        // Numbered in key order; past the limit, a Highlight draws nothing.
        let mut by_key: BTreeMap<u64, Highlight> = BTreeMap::new();
        for (h, _, _) in self.entries.values() {
            by_key.entry(h.key).or_insert(*h);
        }
        let mut numbers: HashMap<u64, u32> = HashMap::default();
        let mut looks = [[0.0; 4]; 2 * HIGHLIGHT_LIMIT];
        for (n, h) in by_key.values().take(HIGHLIGHT_LIMIT).enumerate() {
            numbers.insert(h.key, n as u32 + 1);
            looks[n * 2] = h.fill;
            looks[n * 2 + 1] = h.outline;
        }
        // Grouped by what they draw, so each group is one run.
        let mut groups: Vec<((BatchKey, bool), Vec<Instance>)> = Vec::new();
        for (h, key, instance) in self.entries.values() {
            let Some(&number) = numbers.get(&h.key) else {
                continue;
            };
            let group = (*key, h.on_top);
            let at = match groups.iter().position(|(g, _)| *g == group) {
                Some(at) => at,
                None => {
                    groups.push((group, Vec::new()));
                    groups.len() - 1
                }
            };
            groups[at].1.push(instance.numbered(number));
        }
        let mut runs: Vec<HighlightRun> = Vec::new();
        let mut instances: Vec<Instance> = Vec::new();
        for ((key, on_top), group) in groups {
            let start = instances.len() as u32;
            instances.extend(group);
            runs.push((key, on_top, start..instances.len() as u32));
        }
        if !instances.is_empty() {
            match &self.buffer {
                Some((buffer, capacity)) if *capacity >= instances.len() => {
                    queue.write_buffer(buffer, 0, bytemuck::cast_slice(&instances));
                }
                _ => {
                    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rook highlight instances"),
                        contents: bytemuck::cast_slice(&instances),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
                    self.buffer = Some((buffer, instances.len()));
                }
            }
        }
        self.runs.clone_from(&runs);
        self.looks = looks;
        (runs, looks)
    }
}

/// A mask pipeline: each part's Highlight number where it covers, over
/// everything (`on_top`) or only where nothing nearer is drawn.
pub(crate) fn pipeline(
    device: &wgpu::Device,
    globals: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    on_top: bool,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rook highlights"),
        bind_group_layouts: &[Some(globals)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("rook highlights"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_highlight"),
            compilation_options: Default::default(),
            buffers,
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: main_depth_format(device),
            depth_write_enabled: Some(false),
            // The part's own surface ties with the depth it left (the same
            // vertices through the same invariant maths).
            depth_compare: Some(if on_top {
                wgpu::CompareFunction::Always
            } else {
                wgpu::CompareFunction::GreaterEqual
            }),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_highlight"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: MASK_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Renderer {
    /// The highlighted parts' numbers into the mask, tested against the
    /// main pass's depth.
    pub(crate) fn highlight_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        [mask, depth]: [&wgpu::TextureView; 2],
        runs: &mut Runs,
        start: usize,
    ) {
        profiling::scope!("highlight pass");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rook highlights"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: mask,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            // Read only.
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.bind_group, &[]);
        self.record(&mut pass, runs, start, |_, _| {});
    }
}
