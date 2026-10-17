//! A frame's draws as runs: consecutive draws that share their pipeline,
//! instance buffer, mesh pool page and (without an image table) images,
//! each run one multi-draw from the frame's indirect buffer. Where the
//! adapter can't start indirect draws past the first instance, a run's
//! draws go one by one instead, still binding nothing between them.

use std::ops::Range;

use super::*;

/// One instanced draw's arguments, laid out as indirect draws read them.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct DrawArgs {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

/// Which pipeline a run draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pipe {
    Shadow,
    Opaque,
    Layer,
    Transparent,
    GlassDepth,
    GlassFilter,
    GlassSurface,
    Additive,
    /// Terrain seams' later layers, added over the first.
    Over,
}

/// Which instance buffer a run's instances are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Opaque,
    Layers,
    Transparent,
}

#[derive(Debug)]
pub(crate) struct Run {
    pipe: Pipe,
    source: Source,
    page: usize,
    /// The images to bind, without an image table.
    images: Option<ImageSet>,
    args: Range<usize>,
}

/// A pass's draws: their arguments, in runs.
#[derive(Debug, Default)]
pub(crate) struct Runs {
    pub(crate) args: Vec<DrawArgs>,
    pub(crate) runs: Vec<Run>,
    pub(crate) counts: PassCounts,
}

impl Renderer {
    /// Add a batch's draw of `mesh` to `runs`, joining the last run if it
    /// shares everything bound.
    pub(crate) fn push_draw(
        &self,
        runs: &mut Runs,
        pipe: Pipe,
        source: Source,
        mesh: &GpuMesh,
        key: &BatchKey,
        instances: Range<u32>,
    ) {
        if mesh.indices.is_empty() || instances.is_empty() {
            return;
        }
        let images = match (&self.image_binding, pipe) {
            (ImageBinding::Sets(_), p) if p != Pipe::Shadow => Some(key.images),
            _ => None,
        };
        let at = runs.args.len();
        runs.args.push(DrawArgs {
            index_count: mesh.indices.len() as u32,
            instance_count: instances.len() as u32,
            first_index: mesh.indices.start,
            base_vertex: mesh.vertices.start as i32,
            first_instance: instances.start,
        });
        runs.counts.draws += 1;
        runs.counts.triangles += mesh.indices.len() as u64 / 3 * instances.len() as u64;
        match runs.runs.last_mut() {
            Some(r)
                if (r.pipe, r.source, r.page, r.images) == (pipe, source, mesh.page, images)
                    && r.args.end == at =>
            {
                r.args.end += 1;
            }
            _ => runs.runs.push(Run {
                pipe,
                source,
                page: mesh.page,
                images,
                args: at..at + 1,
            }),
        }
    }

    /// Put the frame's arguments in the indirect buffer (passes' args one
    /// after another), returning where each pass's arguments start, in
    /// draws.
    pub(crate) fn upload_args(&mut self, passes: &[&Runs]) -> Vec<usize> {
        let mut starts = Vec::with_capacity(passes.len());
        let mut all: Vec<DrawArgs> = Vec::new();
        for runs in passes {
            starts.push(all.len());
            all.extend_from_slice(&runs.args);
        }
        if !self.multi_draw || all.is_empty() {
            return starts;
        }
        match &self.indirect {
            Some((buffer, capacity)) if *capacity >= all.len() => {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&all));
            }
            _ => {
                let capacity = all.len() * 2;
                let mut padded = all;
                padded.resize(capacity, DrawArgs::zeroed());
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rook draws"),
                        contents: bytemuck::cast_slice(&padded),
                        usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
                    });
                self.indirect = Some((buffer, capacity));
            }
        }
        starts
    }

    /// Record a pass's runs, whose arguments start at draw `base` of the
    /// indirect buffer. Group 2 is bound here only without a table.
    pub(crate) fn record(&self, pass: &mut wgpu::RenderPass<'_>, runs: &mut Runs, base: usize) {
        let mut bound: (
            Option<Pipe>,
            Option<Source>,
            Option<usize>,
            Option<ImageSet>,
        ) = (None, None, None, None);
        // Each Glass filter or surface run is one part (its passes
        // alternate, so they never join): its own stencil reference, so it
        // draws each pixel once (`Pipelines::new`). After 255 runs the
        // references come round again.
        let mut stencil = 0u32;
        for run in &runs.runs {
            if matches!(run.pipe, Pipe::GlassFilter | Pipe::GlassSurface) {
                stencil = stencil % 255 + 1;
                pass.set_stencil_reference(stencil);
            }
            let shadow = run.pipe == Pipe::Shadow;
            let Some(instances) = self.instance_buffer(run.source) else {
                continue;
            };
            if bound.0 != Some(run.pipe) {
                pass.set_pipeline(self.pipeline(run.pipe));
                bound.0 = Some(run.pipe);
            }
            if bound.1 != Some(run.source) {
                pass.set_vertex_buffer(if shadow { 1 } else { 2 }, instances.slice(..));
                bound.1 = Some(run.source);
            }
            if bound.2 != Some(run.page) {
                let page = &self.pool.pages[run.page];
                pass.set_vertex_buffer(0, page.positions.slice(..));
                if !shadow {
                    pass.set_vertex_buffer(1, page.attributes.slice(..));
                }
                pass.set_index_buffer(page.indices.slice(..), wgpu::IndexFormat::Uint32);
                bound.2 = Some(run.page);
            }
            if let Some(set) = run.images
                && bound.3 != Some(set)
            {
                pass.set_bind_group(2, self.group(&set), &[]);
                bound.3 = Some(set);
            }
            match &self.indirect {
                Some((buffer, _)) if self.multi_draw => {
                    let offset = (base + run.args.start) * std::mem::size_of::<DrawArgs>();
                    pass.multi_draw_indexed_indirect(buffer, offset as u64, run.args.len() as u32);
                    runs.counts.calls += 1;
                }
                _ => {
                    for a in &runs.args[run.args.clone()] {
                        let indices = a.first_index..a.first_index + a.index_count;
                        let instances = a.first_instance..a.first_instance + a.instance_count;
                        pass.draw_indexed(indices, a.base_vertex, instances);
                        runs.counts.calls += 1;
                    }
                }
            }
        }
    }

    fn pipeline(&self, pipe: Pipe) -> &wgpu::RenderPipeline {
        let p = &self.pipelines;
        match pipe {
            Pipe::Shadow => &p.shadow,
            Pipe::Opaque => &p.opaque,
            Pipe::Layer => &p.layer,
            Pipe::Transparent => &p.transparent,
            Pipe::GlassDepth => &p.glass_depth,
            Pipe::GlassFilter => &p.glass_filter,
            Pipe::GlassSurface => &p.glass_surface,
            Pipe::Additive => &p.additive,
            Pipe::Over => &p.over,
        }
    }

    fn instance_buffer(&self, source: Source) -> Option<&wgpu::Buffer> {
        match source {
            Source::Opaque => self.opaque.buffer(),
            Source::Layers => self.layers.buffer(),
            Source::Transparent => self.transparent_buffer.as_ref().map(|(b, _)| b),
        }
    }
}
