//! Drawing a frame: the shadow pass, the sky, then parts and images.

use super::effects::particles::ParticleRun;
use super::effects::ribbons::RibbonRun;
use super::*;
use crate::batch::CULL_SLOTS;
use crate::batch::cull::seen_runs;
use runs::{Pipe, Runs, Source};
use transparent::TransparentRun;

impl Renderer {
    /// Upload the instances changed since the last frame, and the
    /// transparent ones `view` reaches sorted back to front for `camera`.
    /// Returns the transparent draws.
    fn upload_instances(
        &mut self,
        camera: &Camera,
        view: &Frustum,
        margin: f32,
    ) -> Vec<TransparentRun> {
        {
            profiling::scope!("opaque and layers");
            self.opaque.upload(&self.device, &self.queue);
            self.layers.upload(&self.device, &self.queue);
        }
        self.transparent
            .upload(&self.device, &self.queue, camera, view, margin)
    }

    fn depth_view(&mut self, width: u32, height: u32) -> wgpu::TextureView {
        if let Some((_, view, w, h)) = &self.depth
            && *w == width
            && *h == height
        {
            return view.clone();
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rook depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: main_depth_format(&self.device),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.depth = Some((texture, view.clone(), width, height));
        view
    }

    /// Draw the scene into `target` (a view of a `color_format` texture):
    /// into the frame, then through the tone curve onto `target`. Mesh
    /// files and images queued for upload go up first, within a per-frame
    /// budget.
    pub fn render(&mut self, target: &wgpu::TextureView, width: u32, height: u32, camera: &Camera) {
        profiling::scope!("Renderer::render");
        let start = std::time::Instant::now();
        self.upload_files(FRAME_UPLOAD_BYTES);
        let mut times = StageTimes::default();
        let mut lap = start;
        let mut stage = |time: &mut std::time::Duration| {
            let now = std::time::Instant::now();
            *time = now - lap;
            lap = now;
        };
        stage(&mut times.uploads);
        let l = self.lighting;
        let aspect = width as f32 / height.max(1) as f32;
        let sun = l.sun_direction.normalize_or(Vec3::Y);
        // No shadow pass at all with GlobalShadows off, or with the sun
        // below the horizon, where it lights nothing from above.
        let shadow =
            (l.global_shadows && sun.y > 0.0).then(|| ShadowView::fit(camera, aspect, sun));
        // What each pass can see; batches wholly outside draw nothing.
        let view_proj = camera.view_projection(aspect);
        let view = Frustum::of(view_proj);
        let margin = MOTION_STUDS.max(self.terrain.wave_size.abs());
        self.write_globals(camera, aspect, sun, shadow);

        stage(&mut times.setup);
        let transparent_runs = {
            profiling::scope!("upload instances");
            self.upload_instances(camera, &view, margin)
        };

        stage(&mut times.instances);
        let ribbon_runs = self.upload_ribbons(camera, &view);
        let particle_runs = self.upload_particles(camera, &view);
        stage(&mut times.effects);

        // Each pass's draws, joined into runs that share what's bound.
        let (mut shadow_runs, mut main_runs, starts) =
            self.build_draws(shadow, &view, margin, transparent_runs);

        stage(&mut times.draws);
        let depth = self.depth_view(width, height);
        let frame = self.tonemap.frame(&self.device, width, height);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rook frame"),
            });
        self.lights.fill(
            &self.queue,
            &mut encoder,
            camera,
            l.environment_diffuse > 0.0,
        );
        self.update_sky_cube(&mut encoder);
        if shadow.is_some() {
            self.shadow_pass(&mut encoder, &mut shadow_runs, starts[0]);
        }
        self.main_pass(
            &mut encoder,
            [&frame, &depth],
            &mut main_runs,
            starts[1],
            (&ribbon_runs, &particle_runs),
        );
        self.tonemap
            .record(&self.queue, &mut encoder, target, l.tonemapper);
        if let Some(timer) = &mut self.timer {
            timer.resolve(&mut encoder, shadow.is_some());
        }
        self.draw_counts = DrawCounts {
            shadow: shadow_runs.counts,
            main: main_runs.counts,
        };
        let commands = {
            profiling::scope!("finish");
            encoder.finish()
        };
        self.queue.submit([commands]);
        stage(&mut times.passes);
        self.stage_times = times;
        self.record_time = start.elapsed();
    }

    /// The lighting, camera and sky the shaders read this frame.
    fn write_globals(
        &mut self,
        camera: &Camera,
        aspect: f32,
        sun: Vec3,
        shadow: Option<ShadowView>,
    ) {
        let l = self.lighting;
        for hash in l.skybox.into_iter().flatten() {
            if self.images.contains(&hash) {
                self.ensure_group([Some(hash), None, None, None, None]);
            }
        }
        self.update_table();
        let mut sky_images = [[0; 4]; 2];
        for (i, hash) in l.skybox.iter().flatten().enumerate() {
            sky_images[i / 4][i % 4] = self.image_slots(&[Some(*hash), None, None, None, None])[0];
        }
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        let mut faces = [[0.0; 4]; 12];
        for face in NormalId::ALL {
            let (right, up) = face.image_axes();
            faces[face.index() * 2] = rgb(right.to_array(), rook_types::SURFACE_TILE_STUDS);
            faces[face.index() * 2 + 1] = rgb(up.to_array(), 0.0);
        }
        let view_proj = camera.view_projection(aspect);
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: rgb(camera.position.to_array(), 1.0),
            sun_dir: rgb(sun.to_array(), 0.0),
            sun_color: rgb(l.sun_color, 0.0),
            ambient: rgb(l.ambient, 0.0),
            outdoor_ambient: rgb(l.outdoor_ambient, 0.0),
            fog_color: rgb(l.fog_color, 0.0),
            fog_range: [l.fog_start, l.fog_end, l.exposure, 0.0],
            shadow_view_proj: shadow
                .map_or(glam::Mat4::IDENTITY, |s| s.view_proj)
                .to_cols_array_2d(),
            shadow: match shadow {
                Some(s) => [
                    1.0 / SHADOW_SIZE as f32,
                    s.normal_offset,
                    s.depth_bias,
                    SHADOW_DISTANCE,
                ],
                None => [0.0; 4],
            },
            faces,
            sky_from_clip: camera.sky_from_clip(aspect).to_cols_array_2d(),
            sky_faces: {
                let mut out = [[0.0; 4]; 18];
                for (i, (dir, right, up)) in SKYBOX_FACES.iter().enumerate() {
                    out[i * 3] = rgb(dir.to_array(), 0.0);
                    out[i * 3 + 1] = rgb(right.to_array(), 0.0);
                    out[i * 3 + 2] = rgb(up.to_array(), 0.0);
                }
                out
            },
            terrain: [
                self.time,
                self.terrain.wave_size,
                self.terrain.wave_speed,
                self.terrain.grass_reach,
            ],
            wind: rgb(self.terrain.wind.to_array(), self.water_phase as f32),
            sky_top: rgb(l.sky_top, l.environment_diffuse),
            sky_horizon: rgb(l.sky_horizon, l.environment_specular),
            sky_bottom: rgb(l.sky_bottom, 0.0),
            sky_images,
            terrain_layers: self.terrain_layers(),
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
    }

    /// Depth from the sun into the shadow map.
    fn shadow_pass(&self, encoder: &mut wgpu::CommandEncoder, runs: &mut Runs, start: usize) {
        profiling::scope!("shadow pass");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rook shadow map"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.shadow_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: self.timer.as_ref().map(|t| t.writes(1)),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.bind_group, &[]);
        self.record(&mut pass, runs, start);
    }

    /// The sky, the parts and the effects into the frame.
    fn main_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        [frame, depth]: [&wgpu::TextureView; 2],
        runs: &mut Runs,
        start: usize,
        (ribbons, particles): (&[RibbonRun], &[ParticleRun]),
    ) {
        profiling::scope!("main pass");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rook parts"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // The sky covers every pixel first.
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: main_depth_format(&self.device)
                    .has_stencil_aspect()
                    .then_some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Discard,
                    }),
            }),
            timestamp_writes: self.timer.as_ref().map(|t| t.writes(0)),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_bind_group(1, &self.main_bind_group, &[]);
        pass.set_bind_group(3, &self.lights.bind_group, &[]);
        if let ImageBinding::Table(_) = &self.image_binding {
            pass.set_bind_group(2, self.group(&[None; 5]), &[]);
        }
        self.record_sky(&mut pass);
        self.record(&mut pass, runs, start);
        self.record_ribbons(&mut pass, ribbons);
        self.record_particles(&mut pass, particles);
    }

    /// Each pass's draws, joined into runs that share what's bound, with
    /// their arguments uploaded: (shadow pass, main pass, where each
    /// pass's arguments start).
    fn build_draws(
        &mut self,
        shadow: Option<ShadowView>,
        view: &Frustum,
        margin: f32,
        transparent_runs: Vec<TransparentRun>,
    ) -> (Runs, Runs, Vec<usize>) {
        profiling::scope!("build draws");
        let mut shadow_runs = Runs::default();
        let mut main_runs = Runs::default();
        // The sun's whole view, not just the camera's: a part off screen
        // still shadows what's on it.
        let sun_view = shadow.map(|s| Frustum::of(s.view_proj));
        // The three cullings at once.
        let ((seen, shadowed), layers_seen) = rayon::join(
            || {
                rayon::join(
                    || self.opaque.seen(view, margin),
                    || {
                        sun_view
                            .as_ref()
                            .map(|sun| (sun, self.opaque.seen(sun, margin)))
                    },
                )
            },
            || self.layers.seen(view, margin),
        );
        for (k, &seen) in seen.iter().enumerate() {
            let sun = shadowed.as_ref().filter(|(_, s)| s[k]).map(|(sun, _)| *sun);
            if sun.is_none() && !seen {
                continue;
            }
            let (key, instances, chunks, _) = self.opaque.draw(k);
            let Some(mesh) = self.meshes.get(&key.shape) else {
                continue;
            };
            if let Some(sun) = sun {
                let (pipe, source) = (Pipe::Shadow, Source::Opaque);
                for run in seen_runs(sun, chunks, CULL_SLOTS, margin, instances.clone()) {
                    self.push_draw(&mut shadow_runs, pipe, source, mesh, key, run);
                }
            }
            if seen {
                let (pipe, source) = (Pipe::Opaque, Source::Opaque);
                for run in seen_runs(view, chunks, CULL_SLOTS, margin, instances) {
                    self.push_draw(&mut main_runs, pipe, source, mesh, key, run);
                }
            }
        }
        // Then images on opaque parts (before transparent parts, so a
        // window in front of a decal covers it), then transparent parts
        // back to front.
        for k in (0..layers_seen.len()).filter(|&k| layers_seen[k]) {
            let (key, instances, chunks, z) = self.layers.draw(k);
            let Some(mesh) = self.meshes.get(&key.shape) else {
                continue;
            };
            let pipe = if z == placement::OVER_Z {
                Pipe::Over
            } else {
                Pipe::Layer
            };
            let source = Source::Layers;
            for run in seen_runs(view, chunks, CULL_SLOTS, margin, instances) {
                self.push_draw(&mut main_runs, pipe, source, mesh, key, run);
            }
        }
        for (key, pass, instances) in transparent_runs {
            let Some(mesh) = self.meshes.get(&key.shape) else {
                continue;
            };
            self.push_draw(
                &mut main_runs,
                pass.pipe(),
                Source::Transparent,
                mesh,
                &key,
                instances,
            );
        }
        let starts = self.upload_args(&[&shadow_runs, &main_runs]);
        (shadow_runs, main_runs, starts)
    }

    /// Each terrain layer's blend, as the shader reads it.
    fn terrain_layers(&self) -> [[f32; 4]; 2 * TERRAIN_LAYERS] {
        let mut out = [[0.0; 4]; 2 * TERRAIN_LAYERS];
        for (i, l) in self.terrain.layers.iter().enumerate() {
            let slot = self.image_slots(&[None, None, None, l.height, None])[3];
            out[i * 2] = [l.remap[0], l.remap[1], l.scale, l.studs];
            out[i * 2 + 1] = [
                slot as f32,
                f32::from(u8::from(l.cube)),
                l.detile.shift,
                l.detile.turn,
            ];
        }
        out
    }

    /// Build the sky's reflection cube again if the sky changed: from the
    /// skybox once all six images are up, else from the plain sky.
    fn update_sky_cube(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let l = self.lighting;
        let images = l.skybox.and_then(|hashes| {
            let views = hashes.map(|h| self.images.get(&h).map(|i| &i.linear));
            views
                .iter()
                .all(Option::is_some)
                .then(|| (hashes, views.map(|v| v.unwrap())))
        });
        self.sky_cube.update(
            &self.device,
            &self.queue,
            encoder,
            images,
            l.sky_top,
            l.sky_horizon,
        );
    }

    /// The skybox, once all six images are up; otherwise the plain sky
    /// colour stands for it. With a table, one draw of six faces; else a
    /// draw per face with its image bound.
    fn record_sky(&self, pass: &mut wgpu::RenderPass<'_>) {
        let faces = self
            .lighting
            .skybox
            .filter(|faces| faces.iter().all(|h| self.images.contains(h)));
        let Some(faces) = faces else {
            pass.set_pipeline(&self.pipelines.plain_sky);
            // It reads no image, but its layout has the image group.
            if let ImageBinding::Sets(_) = &self.image_binding {
                pass.set_bind_group(2, self.group(&[None; 5]), &[]);
            }
            pass.draw(0..3, 0..1);
            return;
        };
        pass.set_pipeline(&self.pipelines.sky);
        match &self.image_binding {
            ImageBinding::Table(_) => pass.draw(0..3, 0..6),
            ImageBinding::Sets(groups) => {
                for (face, hash) in faces.iter().enumerate() {
                    let Some(group) = groups.get(&[Some(*hash), None, None, None, None]) else {
                        continue;
                    };
                    pass.set_bind_group(2, group, &[]);
                    pass.draw(0..3, face as u32..face as u32 + 1);
                }
            }
        }
    }
}
