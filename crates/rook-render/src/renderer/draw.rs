//! Drawing a frame: the shadow pass, the sky, then parts and images.

use super::*;

/// How a run of transparent instances is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    Part,
    Image,
    /// See-through Glass filtering what's behind it, before its surface.
    GlassFilter,
    ForceField,
}

impl Pass {
    /// The passes an entry is drawn in, in order.
    fn of(p: &PartInstance) -> &'static [Pass] {
        match (p.layer, p.material) {
            (Layer::Image { .. }, _) => &[Pass::Image],
            (Layer::Base, Material::Glass) => &[Pass::GlassFilter, Pass::Part],
            (Layer::Base, Material::ForceField) => &[Pass::ForceField],
            (Layer::Base, _) => &[Pass::Part],
        }
    }
}

impl Renderer {
    /// Upload the instances changed since the last frame, and the
    /// transparent ones `view` reaches sorted back to front for `camera`
    /// (an image on a transparent part just after its part). Returns the
    /// transparent draws: (key, pass, instance range) runs in that order.
    fn upload_instances(
        &mut self,
        camera: &Camera,
        view: &Frustum,
        margin: f32,
    ) -> Vec<(BatchKey, Pass, std::ops::Range<u32>)> {
        self.opaque.upload(&self.device, &self.queue);
        self.layers.upload(&self.device, &self.queue);
        // Back to front for blending.
        let mut order: Vec<&PartInstance> = self.transparent_parts.iter().flatten().collect();
        order.sort_by(|a, b| {
            let da = a.position.distance_squared(camera.position);
            let db = b.position.distance_squared(camera.position);
            let image = |p: &PartInstance| p.layer != Layer::Base;
            db.total_cmp(&da).then(image(a).cmp(&image(b)))
        });
        let mut instances: Vec<Instance> = Vec::with_capacity(order.len());
        let mut runs: Vec<(BatchKey, Pass, std::ops::Range<u32>)> = Vec::new();
        for p in &order {
            let instance = Instance::new(p, self.bounds(p.shape));
            if !view.sees(instance.world_box(), margin) {
                continue;
            }
            let key = BatchKey {
                shape: p.shape,
                images: image_set(p),
            };
            for &pass in Pass::of(p) {
                let i = instances.len() as u32;
                instances.push(instance);
                match runs.last_mut() {
                    Some((k, q, r)) if *k == key && *q == pass => r.end = i + 1,
                    _ => runs.push((key, pass, i..i + 1)),
                }
            }
        }
        if instances.is_empty() {
            return runs;
        }
        match &self.transparent_buffer {
            Some((buffer, capacity)) if *capacity >= instances.len() => {
                self.queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(&instances));
            }
            _ => {
                // Room to grow, so a few more parts don't make a new buffer.
                let capacity = instances.len() * 2;
                let buffer = instance_buffer(&self.device, &instances, capacity);
                self.transparent_buffer = Some((buffer, capacity));
            }
        }
        runs
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
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.depth = Some((texture, view.clone(), width, height));
        view
    }

    /// Draw the scene into `target` (a view of a `color_format` texture).
    /// Mesh files and images queued for upload go up first, within a
    /// per-frame budget.
    pub fn render(&mut self, target: &wgpu::TextureView, width: u32, height: u32, camera: &Camera) {
        profiling::scope!("Renderer::render");
        self.upload_files(FRAME_UPLOAD_BYTES);
        let l = self.lighting;
        let aspect = width as f32 / height.max(1) as f32;
        let sun = l.sun_direction.normalize_or(Vec3::Y);
        // No shadow pass at all with GlobalShadows off, or with the sun
        // below the horizon, where it lights nothing from above.
        let shadow =
            (l.global_shadows && sun.y > 0.0).then(|| ShadowView::fit(camera, aspect, sun));
        for hash in l.skybox.into_iter().flatten() {
            if self.images.contains(&hash) {
                self.ensure_group([Some(hash), None, None, None, None]);
            }
        }
        let rgb = |c: [f32; 3], w: f32| [c[0], c[1], c[2], w];
        let mut faces = [[0.0; 4]; 12];
        for face in NormalId::ALL {
            let (right, up) = face.image_axes();
            faces[face.index() * 2] = rgb(right.to_array(), rook_types::SURFACE_TILE_STUDS);
            faces[face.index() * 2 + 1] = rgb(up.to_array(), 0.0);
        }
        // What each pass can see; batches wholly outside draw nothing.
        let view_proj = camera.view_projection(aspect);
        let view = Frustum::of(view_proj);
        let margin = MOTION_STUDS.max(self.terrain.wave_size.abs());
        let globals = Globals {
            view_proj: view_proj.to_cols_array_2d(),
            camera_pos: rgb(camera.position.to_array(), 1.0),
            sun_dir: rgb(sun.to_array(), 0.0),
            ambient: rgb(l.ambient, 0.0),
            outdoor_ambient: rgb(l.outdoor_ambient, 0.0),
            fog_color: rgb(l.fog_color, l.brightness),
            fog_range: [l.fog_start, l.fog_end, 0.0, 0.0],
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
            sky: rgb(l.reflection, l.environment_specular),
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
            sky_top: rgb(l.sky_top, 0.0),
            sky_horizon: rgb(l.sky_horizon, 0.0),
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let transparent_runs = self.upload_instances(camera, &view, margin);

        let depth = self.depth_view(width, height);
        let sky = srgb_to_linear(l.sky_horizon);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rook frame"),
            });
        let opaque = self.opaque.draws();
        let mut counts = DrawCounts::default();
        if let Some(shadow) = shadow {
            profiling::scope!("shadow pass");
            // The sun's whole view, not just the camera's: a part off
            // screen still shadows what's on it.
            let sun_view = Frustum::of(shadow.view_proj);
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
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_pipeline(&self.pipelines.shadow);
            for (key, buffer, instances, bounds) in &opaque {
                if sun_view.sees(*bounds, margin) {
                    counts.shadow +=
                        self.draw(&mut pass, key.shape, None, buffer, instances.clone());
                }
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rook parts"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(sky[0]),
                            g: f64::from(sky[1]),
                            b: f64::from(sky[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.main_bind_group, &[]);
            // The skybox, once all six images are up; otherwise the clear
            // colour stands for it.
            let skybox = l.skybox.and_then(|faces| {
                let groups: Vec<_> = faces
                    .iter()
                    .map(|h| self.groups.get(&[Some(*h), None, None, None, None]))
                    .collect::<Option<_>>()?;
                Some(groups)
            });
            if let Some(groups) = skybox {
                pass.set_pipeline(&self.pipelines.sky);
                for (face, group) in groups.into_iter().enumerate() {
                    pass.set_bind_group(2, group, &[]);
                    pass.draw(0..3, face as u32..face as u32 + 1);
                }
            }
            // Opaque parts, then images on them (before transparent parts,
            // so a window in front of a decal covers it), then transparent
            // parts back to front.
            for (pipeline, draws) in [
                (&self.pipelines.opaque, opaque),
                (&self.pipelines.layer, self.layers.draws()),
            ] {
                if draws.is_empty() {
                    continue;
                }
                pass.set_pipeline(pipeline);
                for (key, buffer, instances, bounds) in draws {
                    if !view.sees(bounds, margin) {
                        continue;
                    }
                    let images = Some(self.group(&key.images));
                    counts.main += self.draw(&mut pass, key.shape, images, buffer, instances);
                }
            }
            if let Some((buffer, _)) = &self.transparent_buffer {
                for (key, kind, instances) in transparent_runs {
                    pass.set_pipeline(match kind {
                        Pass::Part => &self.pipelines.transparent,
                        Pass::Image => &self.pipelines.layer,
                        Pass::GlassFilter => &self.pipelines.glass_filter,
                        Pass::ForceField => &self.pipelines.additive,
                    });
                    let images = Some(self.group(&key.images));
                    counts.main += self.draw(&mut pass, key.shape, images, buffer, instances);
                }
            }
        }
        self.draw_counts = counts;
        self.queue.submit([encoder.finish()]);
    }

    /// Draw `instances` of `buffer` as `shape`, with `image` bound as
    /// group 2 (not in the shadow pass). Returns how many draw calls that
    /// took (none for a shape without a mesh).
    fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        shape: DrawShape,
        image: Option<&wgpu::BindGroup>,
        buffer: &wgpu::Buffer,
        instances: std::ops::Range<u32>,
    ) -> u32 {
        // Entries are only drawn for shapes with a mesh.
        let Some(mesh) = self.meshes.get(&shape) else {
            return 0;
        };
        if let Some(group) = image {
            pass.set_bind_group(2, group, &[]);
        }
        pass.set_vertex_buffer(0, mesh.vertices.slice(..));
        pass.set_vertex_buffer(1, buffer.slice(..));
        pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.index_count, 0, instances);
        1
    }
}
