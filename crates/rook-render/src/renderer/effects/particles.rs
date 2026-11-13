//! Drawing the emitters' particles (docs/behavior/effects/fire-smoke-and-sparkles.md): each a
//! quad facing the camera, sorted back to front and drawn after the
//! transparent parts, testing depth but not writing it. A particle's
//! color comes premultiplied by its opacity and is added to what's
//! behind it, which is dimmed by its coverage: opacity × (1 −
//! LightEmission), so 0 blends and 1 adds.

use std::ops::Range;

use glam::Mat3;

use super::*;

use crate::effects::particles::{Particle, Particles, reach};
use crate::scene::EmitterLook;
use crate::scene::Orientation;

/// The particle pipeline's instance: one particle.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct ParticleInstance {
    position: [f32; 3],
    /// Width and height in studs.
    size: [f32; 2],
    /// sRGB color, and opacity.
    color: [f32; 4],
    /// Degrees, positive anticlockwise on screen.
    rotation: f32,
    /// LightInfluence: 1 lit by the scene, 0 its own color.
    lit: f32,
    /// Its image's slot in the table (unused without one).
    image: u32,
    /// `Brightness`, a multiplier on its light units.
    brightness: f32,
    /// 1 - LightEmission: how far it covers what's behind it.
    keep: f32,
    /// The part of its image it shows (a flipbook frame): offset and
    /// scale.
    frame: [f32; 4],
    /// Its velocity, which the velocity orientations line up with.
    velocity: [f32; 3],
    /// [`Orientation`] as a number.
    orientation: u32,
    /// Studs towards the camera.
    z_offset: f32,
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

/// An emitter the view may reach, and what its particles share.
struct EmitterDraw<'a> {
    look: &'a EmitterLook,
    particles: &'a [Particle],
    /// A locked emitter's place and axes: its particles are kept along
    /// them.
    place: Option<(Vec3, Mat3)>,
    texture: ImageHash,
    /// Its image's slot in the table.
    slot: u32,
    /// How far from its center a particle may reach, to cull it by.
    reach: Option<f32>,
}

impl EmitterDraw<'_> {
    /// The instances of the particles `view` may see, with their squared
    /// distance from `camera`.
    fn instances(&self, view: &Frustum, camera: &Camera) -> Vec<(f32, ParticleInstance)> {
        let look = self.look;
        let keep = 1.0 - look.light_emission.clamp(0.0, 1.0);
        let orientation = match look.orientation {
            Orientation::FacingCamera => 0,
            Orientation::FacingCameraWorldUp => 1,
            Orientation::VelocityParallel => 2,
            Orientation::VelocityPerpendicular => 3,
        };
        let mut out = Vec::new();
        for p in self.particles {
            let (position, velocity) = match self.place {
                Some((at, axes)) => (at + axes * p.position, axes * p.velocity),
                None => (p.position, p.velocity),
            };
            if let Some(reach) = self.reach
                && !view.sees(
                    Aabb {
                        min: position,
                        max: position,
                    },
                    reach,
                )
            {
                continue;
            }
            let t = p.life();
            let alpha = p.alpha(look);
            let size = p.size(look);
            if alpha <= 0.0 || size[0] <= 0.0 || size[1] <= 0.0 {
                continue;
            }
            let color = look.color.at(t);
            let frame = match look.flipbook {
                Some(f) => {
                    let i = p.frame(f.mode, f.frames());
                    let (columns, rows) = (f.columns.max(1), f.rows.max(1));
                    let scale = [1.0 / columns as f32, 1.0 / rows as f32];
                    [
                        (i % columns) as f32 * scale[0],
                        (i / columns) as f32 * scale[1],
                        scale[0],
                        scale[1],
                    ]
                }
                None => [0.0, 0.0, 1.0, 1.0],
            };
            let instance = ParticleInstance {
                position: position.to_array(),
                size,
                color: [color[0], color[1], color[2], alpha],
                // Studio turns positive rotations clockwise.
                rotation: -p.rotation(),
                lit: look.light_influence.clamp(0.0, 1.0),
                image: self.slot,
                brightness: look.brightness.at(t),
                keep,
                frame,
                velocity: velocity.to_array(),
                orientation,
                z_offset: look.z_offset,
            };
            out.push((position.distance_squared(camera.position), instance));
        }
        out
    }
}

impl Renderer {
    /// Run the particles and Trails on by `seconds` without drawing (a
    /// screenshot shows effects as they look once they've been going a
    /// while): particles frame by frame at 60 a second as `camera` sees
    /// them in a `width` × `height` view, so only those in view run, as
    /// when drawn.
    pub fn settle_particles(&mut self, seconds: f32, camera: &Camera, width: u32, height: u32) {
        let view = Frustum::of(camera.view_projection(width as f32 / height.max(1) as f32));
        let frames = (seconds * 60.0).round() as u32;
        for _ in 0..frames {
            self.particles.sim.advance(1.0 / 60.0);
            self.particles.sim.frame(&view, camera.position);
        }
        let mut left = seconds;
        while left > 0.0 {
            let dt = left.min(1.0 / 30.0);
            self.ribbons.sim.step(dt);
            left -= dt;
        }
    }

    /// Ask for the images of the effects in the slots `changed`.
    pub(crate) fn request_effect_images(&mut self, scene: &Scene, changed: &EffectSlots) {
        let at = |list: &[usize], look: &dyn Fn(usize) -> Option<ImageHash>| {
            list.iter().filter_map(|&i| look(i)).collect::<Vec<_>>()
        };
        let hashes = [
            at(&changed.emitters, &|i| {
                scene.emitters[i].as_ref()?.look.texture
            }),
            at(&changed.beams, &|i| scene.beams[i].as_ref()?.look.texture),
            at(&changed.trails, &|i| scene.trails[i].as_ref()?.look.texture),
            at(&changed.floor_wires, &|i| {
                scene.floor_wires[i].as_ref()?.look.texture
            }),
        ];
        for hash in hashes.into_iter().flatten() {
            self.image_readiness(hash);
        }
    }

    /// Upload the particles `view` reaches, back to front from `camera`,
    /// and return their runs by image.
    pub(crate) fn upload_particles(&mut self, camera: &Camera, view: &Frustum) -> Vec<ParticleRun> {
        if self.particles.sim.is_empty() {
            return Vec::new();
        }
        profiling::scope!("upload particles");
        // The emitters in view move on (only they do), then Studio's
        // budget picks which of them draw.
        let seen = self.particles.sim.frame(view, camera.position);
        let sim = &self.particles.sim;
        let mut drawn: Vec<usize> = seen
            .into_iter()
            .filter(|&i| {
                sim.emitter(i).is_some_and(|(e, ps)| {
                    !ps.is_empty() && e.look.texture.is_some_and(|h| self.images.contains(&h))
                })
            })
            .collect();
        sim.within_budget(&mut drawn, camera.position);
        let mut emitters: Vec<EmitterDraw<'_>> = Vec::new();
        for i in drawn {
            let Some((emitter, particles)) = sim.emitter(i) else {
                continue;
            };
            let look = &*emitter.look;
            let Some(texture) = look.texture else {
                continue;
            };
            emitters.push(EmitterDraw {
                look,
                particles,
                place: look.locked.then_some((emitter.position, emitter.rotation)),
                texture,
                slot: 0,
                // One moved by ZOffset isn't culled on its own.
                reach: (look.z_offset == 0.0).then(|| reach(look)),
            });
        }
        for e in &mut emitters {
            e.slot = self.image_slots(&lone_image(Some(e.texture)))[0];
        }
        // Each emitter's particles are built on their own (in parallel),
        // then sorted by small keys pointing at them, not moved themselves.
        let workers = &rook_platform::get().workers;
        let built: Vec<Vec<(f32, ParticleInstance)>> =
            workers.map(&emitters, |e| e.instances(view, camera));
        let textures: Vec<ImageHash> = emitters.iter().map(|e| e.texture).collect();
        // (distance, emitter, particle), back to front; ties keep the
        // order they were made in.
        let mut order: Vec<(f32, u32, u32)> = built
            .iter()
            .enumerate()
            .flat_map(|(e, list)| {
                list.iter()
                    .enumerate()
                    .map(move |(i, &(d, _))| (d, e as u32, i as u32))
            })
            .collect();
        workers.sort_by(&mut order, |a, b| {
            b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
        });
        // With a table every particle draws in one go; otherwise a run per
        // image in a row.
        let table = matches!(self.image_binding, ImageBinding::Table(_));
        let mut runs: Vec<ParticleRun> = Vec::new();
        for (i, &(_, e, _)) in order.iter().enumerate() {
            let i = i as u32;
            let texture = &textures[e as usize];
            match runs.last_mut() {
                Some((t, r)) if table || t == texture => r.end = i + 1,
                _ => runs.push((*texture, i..i + 1)),
            }
        }
        for (texture, _) in &runs {
            self.ensure_group(lone_image(Some(*texture)));
        }
        rook_types::count!(ParticlesDrawn, order.len());
        let instances: Vec<ParticleInstance> = order
            .iter()
            .map(|&(_, e, i)| built[e as usize][i as usize].1)
            .collect();
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
                pass.set_bind_group(2, self.group(&lone_image(Some(*texture))), &[]);
            }
            pass.draw(0..6, range.clone());
        }
    }
}

/// The particle pipeline: premultiplied color over what's behind,
/// dimmed by the coverage in alpha; depth tested, not written.
pub(crate) fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
    depth_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let attrs = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x2, 2 => Float32x4, 3 => Float32, 4 => Float32, 5 => Uint32,
        6 => Float32, 7 => Float32, 8 => Float32x4, 9 => Float32x3, 10 => Uint32, 11 => Float32
    ];
    blended_pipeline(
        device,
        "rook particles",
        layout,
        (shader, "vs_particle", "fs_particle"),
        &[wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ParticleInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &attrs,
        }],
        (color_format, depth_format),
        wgpu::CompareFunction::Greater,
    )
}

/// A pipeline drawing over the frame as particles do: premultiplied color
/// added to what's behind, which is dimmed by the coverage in alpha; depth
/// compared by `depth_compare`, not written; both sides drawn. `entries`
/// are the shader and its vertex and fragment entry points.
pub(crate) fn blended_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    (shader, vertex, fragment): (&wgpu::ShaderModule, &str, &str),
    buffers: &[wgpu::VertexBufferLayout<'_>],
    (color_format, depth_format): (wgpu::TextureFormat, wgpu::TextureFormat),
    depth_compare: wgpu::CompareFunction,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: &buffers.iter().cloned().map(Some).collect::<Vec<_>>(),
        },
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format,
            depth_write_enabled: Some(false),
            depth_compare: Some(depth_compare),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
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
