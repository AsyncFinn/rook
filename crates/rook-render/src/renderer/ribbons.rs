//! Drawing Beams and Trails (docs/behavior/beams-and-trails.md): each
//! strip's quads, with the particles' colours, blending and lighting
//! (`fs_particle`), after the transparent parts and before the particles,
//! farthest strip first, both sides showing.

use std::ops::Range;

use super::*;
use crate::ribbons::{Edge, Ribbons, Strip};

/// The ribbon pipeline's vertex.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct RibbonVertex {
    position: [f32; 3],
    uv: [f32; 2],
    /// sRGB colour, and opacity.
    color: [f32; 4],
    /// LightInfluence.
    lit: f32,
    /// Its image's slot in the table (white without one).
    image: u32,
    brightness: f32,
    /// 1 - LightEmission.
    keep: f32,
}

/// The ribbons, and the frame's vertices with how many the buffer has
/// room for.
#[derive(Debug, Default)]
pub(crate) struct GpuRibbons {
    pub(crate) sim: Ribbons,
    buffer: Option<(wgpu::Buffer, usize)>,
}

/// A run of the frame's ribbon vertices drawn with one image (none:
/// white).
pub(crate) type RibbonRun = (Option<ImageHash>, Range<u32>);

impl Renderer {
    /// Ask for the ribbons' images.
    pub(crate) fn request_ribbon_images(&mut self, scene: &Scene) {
        let beams = scene.beams.iter().map(|b| b.look.texture);
        let trails = scene.trails.iter().map(|t| t.look.texture);
        for hash in beams.chain(trails).flatten().collect::<Vec<_>>() {
            self.image_readiness(hash, Some(scene));
        }
    }

    /// Upload the ribbons `view` reaches, farthest first, and return their
    /// runs by image.
    pub(crate) fn upload_ribbons(&mut self, camera: &Camera, view: &Frustum) -> Vec<RibbonRun> {
        if !self.ribbons.sim.any() {
            return Vec::new();
        }
        profiling::scope!("upload ribbons");
        let strips = self.ribbons.sim.strips(camera.position, self.time);
        let mut drawn: Vec<(f32, &Strip<'_>, Option<ImageHash>)> = Vec::new();
        for strip in &strips {
            let mut b = Aabb::EMPTY;
            for e in &strip.edges {
                for p in e.sides {
                    b = b.union(Aabb { min: p, max: p });
                }
            }
            if b.is_empty() || !view.sees(b, 0.0) {
                continue;
            }
            let centre = (b.min + b.max) * 0.5;
            // An image still loading draws as plain colour (creator-docs).
            let texture = strip.look.texture.filter(|h| self.images.contains(h));
            drawn.push((centre.distance_squared(camera.position), strip, texture));
        }
        drawn.sort_by(|a, b| b.0.total_cmp(&a.0));
        let table = matches!(self.image_binding, ImageBinding::Table(_));
        let mut runs: Vec<RibbonRun> = Vec::new();
        let mut vertices: Vec<RibbonVertex> = Vec::new();
        for (_, strip, texture) in drawn {
            let slot = match texture {
                Some(t) => self.image_slots(&[Some(t), None, None, None, None])[0],
                None => 0,
            };
            let start = vertices.len() as u32;
            quads(strip, slot, &mut vertices);
            let end = vertices.len() as u32;
            match runs.last_mut() {
                Some((t, r)) if table || *t == texture => r.end = end,
                _ => runs.push((texture, start..end)),
            }
        }
        for (texture, _) in &runs {
            self.ensure_group([*texture, None, None, None, None]);
        }
        if vertices.is_empty() {
            return Vec::new();
        }
        match &self.ribbons.buffer {
            Some((buffer, capacity)) if *capacity >= vertices.len() => {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&vertices));
            }
            _ => {
                let capacity = vertices.len().next_power_of_two();
                vertices.resize(capacity, RibbonVertex::zeroed());
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rook ribbons"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
                self.ribbons.buffer = Some((buffer, capacity));
            }
        }
        runs
    }

    /// Draw the frame's ribbons (in the main pass, before the particles).
    pub(crate) fn record_ribbons(&self, pass: &mut wgpu::RenderPass<'_>, runs: &[RibbonRun]) {
        let Some((buffer, _)) = &self.ribbons.buffer else {
            return;
        };
        if runs.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipelines.ribbons);
        pass.set_vertex_buffer(0, buffer.slice(..));
        for (texture, range) in runs {
            if let ImageBinding::Sets(_) = &self.image_binding {
                pass.set_bind_group(2, self.group(&[*texture, None, None, None, None]), &[]);
            }
            pass.draw(range.clone(), 0..1);
        }
    }
}

/// Two triangles between each pair of edges.
fn quads(strip: &Strip<'_>, image: u32, out: &mut Vec<RibbonVertex>) {
    let look = strip.look;
    let vertex = |e: &Edge, side: usize| RibbonVertex {
        position: e.sides[side].to_array(),
        uv: [e.u, side as f32],
        color: [e.color[0], e.color[1], e.color[2], e.alpha],
        lit: look.light_influence.clamp(0.0, 1.0),
        image,
        brightness: look.brightness,
        keep: 1.0 - look.light_emission.clamp(0.0, 1.0),
    };
    for pair in strip.edges.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.alpha <= 0.0 && b.alpha <= 0.0 {
            continue;
        }
        let (a0, a1, b0, b1) = (vertex(a, 0), vertex(a, 1), vertex(b, 0), vertex(b, 1));
        out.extend_from_slice(&[a0, a1, b0, b0, a1, b1]);
    }
}

/// The ribbon pipeline: as the particles' (premultiplied colour over what's
/// behind, depth tested, not written), from a plain triangle list, both
/// sides drawn.
pub(crate) fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let attrs = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32, 4 => Uint32, 5 => Float32,
        6 => Float32
    ];
    super::particles::blended_pipeline(
        device,
        "rook ribbons",
        layout,
        (shader, "vs_ribbon", "fs_particle"),
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RibbonVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attrs,
        },
        color_format,
        depth_format,
    )
}
