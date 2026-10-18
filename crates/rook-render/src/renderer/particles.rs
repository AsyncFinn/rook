//! Drawing the emitters' particles (docs/behavior/effects.md): each a
//! quad facing the camera, sorted back to front and drawn after the
//! transparent parts, testing depth but not writing it. A particle's
//! colour comes premultiplied by its opacity and is added to what's
//! behind it, which is dimmed by its coverage: opacity × (1 −
//! LightEmission), so 0 blends and 1 adds.

use std::ops::Range;

use super::*;
use crate::particles::Particles;

/// The particle pipeline's instance: one particle.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct ParticleInstance {
    position: [f32; 3],
    size: f32,
    /// Linear colour times opacity, and coverage.
    color: [f32; 4],
    /// Degrees.
    rotation: f32,
    /// 1 lit by the scene, 0 its own colour.
    lit: f32,
    /// Its image's slot in the table (unused without one).
    image: u32,
    _pad: u32,
}

/// The particles and what the last frame drew of them.
#[derive(Debug, Default)]
pub(crate) struct GpuParticles {
    pub(crate) sim: Particles,
    /// The frame's instances, and how many it has room for.
    buffer: Option<(wgpu::Buffer, usize)>,
}

/// A run of the frame's particles drawn with one image.
pub(crate) type ParticleRun = (ImageHash, Range<u32>);

impl Renderer {
    /// Run the particles on by `seconds` without drawing (a screenshot
    /// shows effects as they look once they've been going a while).
    pub fn settle_particles(&mut self, seconds: f32) {
        let mut left = seconds;
        while left > 0.0 {
            let dt = left.min(1.0 / 30.0);
            self.particles.sim.step(dt);
            left -= dt;
        }
    }

    /// Ask for the emitters' images.
    pub(crate) fn request_particle_images(&mut self, scene: &Scene) {
        for e in &scene.emitters {
            if let Some(hash) = e.look.texture {
                self.image_readiness(hash, Some(scene));
            }
        }
    }

    /// Upload the particles `view` reaches, back to front from `camera`,
    /// and return their runs by image.
    pub(crate) fn upload_particles(&mut self, camera: &Camera, view: &Frustum) -> Vec<ParticleRun> {
        if !self.particles.sim.any() {
            return Vec::new();
        }
        profiling::scope!("upload particles");
        let mut drawn: Vec<(f32, ImageHash, ParticleInstance)> = Vec::new();
        for (look, particles, bounds) in self.particles.sim.drawn() {
            let Some(texture) = look.texture.filter(|h| self.images.contains(h)) else {
                continue;
            };
            let biggest = look.size.0.iter().map(|p| p.1).fold(0.0, f32::max)
                * look.size_spread[1].max(look.size_spread[0]);
            if !view.sees(bounds, biggest * 0.5) {
                continue;
            }
            let slot = self.image_slots(&[Some(texture), None, None, None, None])[0];
            let color = srgb_to_linear(look.color);
            let keep = 1.0 - look.light_emission.clamp(0.0, 1.0);
            for p in particles {
                let t = p.life();
                let alpha = look.alpha.at(t).clamp(0.0, 1.0);
                let size = p.size(look);
                if alpha <= 0.0 || size <= 0.0 {
                    continue;
                }
                let k = look.brightness.at(t) * alpha;
                let instance = ParticleInstance {
                    position: p.position.to_array(),
                    size,
                    color: [color[0] * k, color[1] * k, color[2] * k, alpha * keep],
                    rotation: p.rotation(),
                    lit: f32::from(u8::from(look.lit)),
                    image: slot,
                    _pad: 0,
                };
                let d = p.position.distance_squared(camera.position);
                drawn.push((d, texture, instance));
            }
        }
        drawn.sort_by(|a, b| b.0.total_cmp(&a.0));
        // With a table every particle draws in one go; otherwise a run per
        // image in a row.
        let table = matches!(self.image_binding, ImageBinding::Table(_));
        let mut runs: Vec<ParticleRun> = Vec::new();
        for (i, (_, texture, _)) in drawn.iter().enumerate() {
            let i = i as u32;
            match runs.last_mut() {
                Some((t, r)) if table || t == texture => r.end = i + 1,
                _ => runs.push((*texture, i..i + 1)),
            }
        }
        for (texture, _) in &runs {
            self.ensure_group([Some(*texture), None, None, None, None]);
        }
        let instances: Vec<ParticleInstance> = drawn.into_iter().map(|d| d.2).collect();
        if instances.is_empty() {
            return runs;
        }
        match &self.particles.buffer {
            Some((buffer, capacity)) if *capacity >= instances.len() => {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&instances));
            }
            _ => {
                let capacity = instances.len().next_power_of_two();
                let mut padded = instances;
                padded.resize(capacity, ParticleInstance::zeroed());
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rook particles"),
                        contents: bytemuck::cast_slice(&padded),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
                self.particles.buffer = Some((buffer, capacity));
            }
        }
        runs
    }

    /// Draw the frame's particles (after everything else in the main
    /// pass).
    pub(crate) fn record_particles(&self, pass: &mut wgpu::RenderPass<'_>, runs: &[ParticleRun]) {
        let Some((buffer, _)) = &self.particles.buffer else {
            return;
        };
        if runs.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipelines.particles);
        pass.set_vertex_buffer(0, buffer.slice(..));
        for (texture, range) in runs {
            if let ImageBinding::Sets(_) = &self.image_binding {
                pass.set_bind_group(
                    2,
                    self.group(&[Some(*texture), None, None, None, None]),
                    &[],
                );
            }
            pass.draw(0..6, range.clone());
        }
    }
}

/// The particle pipeline: premultiplied colour over what's behind,
/// dimmed by the coverage in alpha; depth tested, not written.
pub(crate) fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let attrs = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32, 2 => Float32x4, 3 => Float32, 4 => Float32, 5 => Uint32
    ];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("rook particles"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_particle"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<ParticleInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &attrs,
            })],
        },
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Greater),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_particle"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent::OVER,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
