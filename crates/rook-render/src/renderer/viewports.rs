//! ViewportFrames' worlds (docs/behavior/gui/viewport-frames.md): each
//! frame's parts drawn by the main pass's pipelines into a picture of the
//! frame's size, from its own camera, lit by its own Ambient and
//! LightColor and its fixed fog, with no sky, shadows or local lights. The
//! GUI shows the picture (`Paint::Viewport`). A picture is drawn again
//! only when something of its world changed, as Godot redraws a
//! SubViewport set to update when changed.

use super::*;
use detail::DetailView;
use runs::Runs;

/// A ViewportFrame's fog: from 0 to 100,000 studs, of 0.75 light units,
/// whatever Lighting says (measured: black parts at 10 to 99,000 studs fit
/// FogColor 192 not squared, `fog_light` squares this).
const FOG_END: f32 = 100_000.0;
const FOG_COLOR: f32 = 0.867_721_6;

/// What the frame's light is multiplied by (measured: Ambient 102 drew
/// 102 and 104 drew 104, but 106 drew 108 and 250 drew 252, at any
/// distance; LightColor alike), 1.0095 on the 8-bit value it holds.
const EXPOSURE: f32 = 1.019;

#[derive(Debug, Default)]
pub(crate) struct Viewports {
    worlds: FxHashMap<u64, ViewportWorld>,
    /// A picture was made or dropped since the GUI last took their views.
    views_changed: bool,
    /// The version of the GUI's list of frames shown the pictures are
    /// kept for.
    shown_version: u64,
}

#[derive(Debug, Default)]
struct ViewportWorld {
    world: World,
    /// How it's seen and lit; `None` once the scene no longer has it.
    look: Option<Viewport>,
    picture: Option<Picture>,
    /// Something of it changed since its picture was drawn.
    stale: bool,
}

/// What a frame's world is drawn into, at its size.
#[derive(Debug)]
struct Picture {
    size: [u32; 2],
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    globals: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
}

impl Viewports {
    pub(crate) fn clear(&mut self) {
        self.views_changed |= self.worlds.values().any(|w| w.picture.is_some());
        self.worlds.clear();
    }

    pub(crate) fn world(&self, key: u64) -> Option<&World> {
        self.worlds.get(&key).map(|w| &w.world)
    }

    /// Frame `key`'s world, to change: its picture is drawn again.
    pub(crate) fn world_mut(&mut self, key: u64) -> &mut World {
        let w = self.worlds.entry(key).or_default();
        w.stale = true;
        &mut w.world
    }

    /// Forget frame `key`'s world once nothing names it: the scene has
    /// no such frame and no entry is drawn in it.
    fn drop_if_unused(&mut self, key: u64) {
        if let Some(w) = self.worlds.get(&key)
            && w.look.is_none()
            && w.world.entries == 0
        {
            self.views_changed |= w.picture.is_some();
            self.worlds.remove(&key);
        }
    }
}

impl Renderer {
    /// Take the looks of the ViewportFrames at `changed` (keys into
    /// `scene.viewports`; one the scene no longer has is gone) again.
    /// Their entries come with `update_parts`, called first.
    pub fn update_viewports(&mut self, scene: &Scene, changed: &[u64]) {
        for key in changed {
            let look = scene.viewports.get(key).copied();
            if look.is_some() || self.viewports.worlds.contains_key(key) {
                let w = self.viewports.worlds.entry(*key).or_default();
                w.look = look;
                w.stale = true;
            }
            self.viewports.drop_if_unused(*key);
        }
    }

    /// Draw the shown frames' pictures that are out of date, recording
    /// their passes in `encoder` after `upload_args` puts their draws in
    /// the indirect buffer: the draws, by frame.
    pub(crate) fn viewport_draws(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Vec<(u64, Runs)> {
        let (shown, version) = self.gui.viewports();
        if version != self.viewports.shown_version {
            // A frame no longer shown keeps its world, not its picture.
            self.viewports.shown_version = version;
            for (key, w) in &mut self.viewports.worlds {
                if !shown.iter().any(|s| s.key == *key) && w.picture.take().is_some() {
                    self.viewports.views_changed = true;
                }
            }
        }
        let mut out = Vec::new();
        for i in 0..shown.len() {
            let shown = self.gui.viewports().0[i];
            let Some(mut w) = self.viewports.worlds.remove(&shown.key) else {
                continue;
            };
            if let Some(look) = w.look {
                let size = shown.size.map(|v| v.max(1));
                if w.picture.as_ref().is_none_or(|p| p.size != size) {
                    w.picture = Some(self.picture(size));
                    self.viewports.views_changed = true;
                    w.stale = true;
                }
                if std::mem::take(&mut w.stale) {
                    out.push((shown.key, self.viewport_runs(&mut w, &look, encoder)));
                }
            }
            self.viewports.worlds.insert(shown.key, w);
        }
        out
    }

    /// `w`'s draws as `look` sees them, its instances uploaded and its
    /// globals written.
    fn viewport_runs(
        &self,
        w: &mut ViewportWorld,
        look: &Viewport,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Runs {
        let world = &mut w.world;
        (world.opaque).upload(&self.device, &self.queue, encoder, &self.scatter);
        (world.layers).upload(&self.device, &self.queue, encoder, &self.scatter);
        let (Some(camera), Some(picture)) = (look.camera, &w.picture) else {
            return Runs::seen_from(DetailView::default());
        };
        let [width, height] = picture.size;
        let aspect = width as f32 / height as f32;
        let globals = Globals {
            sun_dir: rgb(look.towards_light.to_array(), 0.0),
            sun_color: rgb(look.light_color.map(|c| c * c), 0.0),
            ambient: rgb(look.ambient, 0.0),
            fog_color: [FOG_COLOR, FOG_COLOR, FOG_COLOR, 0.0],
            fog_range: [0.0, FOG_END, EXPOSURE, pixel_studs(&camera, height)],
            ..self.view_globals(&camera, aspect)
        };
        (self.queue).write_buffer(&picture.globals, 0, bytemuck::bytes_of(&globals));
        let view = Frustum::of(camera.view_projection(aspect));
        let transparent =
            (world.transparent).upload(&self.device, &self.queue, &camera, &view, MOTION_STUDS);
        let mut runs = Runs::seen_from(DetailView::of(&camera));
        let seen = (
            world.opaque.seen(&view, MOTION_STUDS),
            world.layers.seen(&view, MOTION_STUDS),
        );
        self.push_world(
            world,
            &mut runs,
            (&view, MOTION_STUDS),
            seen,
            transparent,
            &[],
        );
        runs
    }

    /// A new picture of `size` pixels, and its globals.
    fn picture(&self, [width, height]: [u32; 2]) -> Picture {
        let target = |label, format, usage| {
            (self.device)
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let globals = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook viewport globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook viewport globals"),
            layout: &self.globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        Picture {
            size: [width, height],
            color: target(
                "rook viewport picture",
                FRAME_FORMAT,
                attachment | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            depth: target(
                "rook viewport depth",
                main_depth_format(&self.device),
                attachment,
            ),
            globals,
            globals_group,
        }
    }

    /// Record each of `draws`' passes, whose arguments start at `starts`:
    /// the picture cleared to the frame's background, then its parts.
    pub(crate) fn record_viewports(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        draws: &mut [(u64, Runs)],
        starts: &[usize],
    ) {
        for ((key, runs), &start) in draws.iter_mut().zip(starts) {
            let Some(w) = self.viewports.worlds.get(key) else {
                continue;
            };
            let (Some(picture), Some(look)) = (&w.picture, &w.look) else {
                continue;
            };
            // The picture holds light as the frame does, sqrt(units / 4):
            // half the value the GUI shows (`gui_paint`).
            let [r, g, b, a] = look.background.map(f64::from);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rook viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &picture.color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: r * 0.5,
                            g: g * 0.5,
                            b: b * 0.5,
                            a,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &picture.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: main_depth_format(&self.device)
                        .has_stencil_aspect()
                        .then_some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Discard,
                        }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &picture.globals_group, &[]);
            pass.set_bind_group(1, &self.main_bind_group, &[]);
            pass.set_bind_group(2, self.group(&lone_image(None)), &[]);
            pass.set_bind_group(3, &self.lights.unlit, &[]);
            self.record(&w.world, &mut pass, runs, start, |_, _| {});
        }
    }

    /// The pictures' views by frame, when which pictures there are
    /// changed since last asked.
    pub(crate) fn take_viewport_views(&mut self) -> Option<FxHashMap<u64, wgpu::TextureView>> {
        std::mem::take(&mut self.viewports.views_changed).then(|| {
            (self.viewports.worlds.iter())
                .filter_map(|(key, w)| Some((*key, w.picture.as_ref()?.color.clone())))
                .collect()
        })
    }
}

/// `c` with `w` after it, as the globals hold colors and directions.
fn rgb([r, g, b]: [f32; 3], w: f32) -> [f32; 4] {
    [r, g, b, w]
}

/// Studs a pixel spans 1 stud from `camera`, in a frame `height` pixels
/// tall.
pub(crate) fn pixel_studs(camera: &Camera, height: u32) -> f32 {
    2.0 * (camera.fov_y.to_radians() * 0.5).tan() / height.max(1) as f32
}
