//! World GUI on the GPU: every SurfaceGui's and BillboardGui's canvas is
//! drawn by the GUI pass into one atlas, then laid in the world each
//! frame as a rectangle, after the particles, depth tested unless it's
//! AlwaysOnTop (docs/behavior/gui/world.md).

use std::ops::Range;

use glam::Vec2;
use rook_types::{GuiQuad, WorldCanvas, WorldPlace};
use wesl_core::include_wesl;

use super::super::*;
use crate::renderer::effects::particles::blended_pipeline;
use crate::renderer::pipelines::BindLayouts;

/// The atlas's first and largest side, in pixels.
const FIRST_SIDE: u32 = 1024;
const LARGEST_SIDE: u32 = 4096;
/// The most pixels a canvas gets each way; bigger ones draw smaller.
const LARGEST_CANVAS: f32 = 2048.0;
/// Empty pixels between canvases, so filtering doesn't bleed.
const GAP: f32 = 2.0;
/// How far a SurfaceGui stands off its face, so the two don't fight.
const LIFT: f32 = 0.01;

/// The world GUI pipeline's instance: one canvas.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CanvasInstance {
    origin: [f32; 3],
    right: [f32; 3],
    down: [f32; 3],
    uv: [f32; 4],
    /// LightInfluence and Brightness.
    look: [f32; 2],
    one_sided: f32,
}

/// A canvas in the atlas.
#[derive(Debug, Clone, Copy)]
struct Slot {
    /// Its part of the atlas, 0 to 1.
    uv: [f32; 4],
    /// The canvas pixels that part shows, and the canvas's size.
    reach: [f32; 4],
    size: Vec2,
    light_influence: f32,
    brightness: f32,
    on_top: bool,
    max_distance: f32,
}

/// The atlas's texture, and the bind group the world pass samples it by.
#[derive(Debug)]
pub(crate) struct Atlas {
    pub(crate) view: wgpu::TextureView,
    pub(crate) side: u32,
    group: wgpu::BindGroup,
}

/// The frame's canvases drawn depth tested, then on top.
pub(crate) type WorldRuns = [Range<u32>; 2];

#[derive(Debug)]
pub(crate) struct WorldGui {
    tested: wgpu::RenderPipeline,
    on_top: wgpu::RenderPipeline,
    group_layout: wgpu::BindGroupLayout,
    pub(super) atlas: Option<Atlas>,
    /// Each canvas [`WorldGui::pack`] was given, `None` if it draws nothing.
    slots: Vec<Option<Slot>>,
    /// Where each canvas is now.
    places: Vec<Option<WorldPlace>>,
    /// The frame's instances, and how many there's room for.
    buffer: Option<(wgpu::Buffer, usize)>,
}

impl WorldGui {
    pub(crate) fn new(device: &wgpu::Device, layouts: BindLayouts<'_>, table: bool) -> Self {
        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook world gui"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rook world gui"),
            bind_group_layouts: &[
                Some(layouts.globals),
                Some(layouts.main),
                Some(&group_layout),
                Some(layouts.lights),
            ],
            immediate_size: 0,
        });
        let source = if table {
            include_wesl!("world_gui_table")
        } else {
            include_wesl!("world_gui")
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rook world gui"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let attrs = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x4, 4 => Float32x2,
            5 => Float32
        ];
        let pipeline = |compare| {
            blended_pipeline(
                device,
                "rook world gui",
                &layout,
                (&shader, "vs_world_gui", "fs_world_gui"),
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CanvasInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attrs,
                },
                (FRAME_FORMAT, main_depth_format(device)),
                compare,
            )
        };
        Self {
            tested: pipeline(wgpu::CompareFunction::Greater),
            on_top: pipeline(wgpu::CompareFunction::Always),
            group_layout,
            atlas: None,
            slots: Vec::new(),
            places: Vec::new(),
            buffer: None,
        }
    }

    /// Lay `canvases` out in the atlas (growing it if they don't fit, then
    /// drawing them smaller), and return their quads in atlas pixels, to
    /// be drawn into it.
    pub(crate) fn pack(&mut self, device: &wgpu::Device, canvases: &[WorldCanvas]) -> Vec<GuiQuad> {
        self.slots.clear();
        if canvases.iter().all(|c| c.quads.is_empty()) {
            self.slots.resize(canvases.len(), None);
            return Vec::new();
        }
        let extent = |c: &WorldCanvas| Vec2::new(c.reach[2] - c.reach[0], c.reach[3] - c.reach[1]);
        let mut side = self.atlas.as_ref().map_or(FIRST_SIDE, |a| a.side);
        let mut shrink = 1.0;
        let corners = loop {
            let scales: Vec<f32> = canvases
                .iter()
                .map(|c| shrink * (LARGEST_CANVAS / extent(c).max_element().max(1.0)).min(1.0))
                .collect();
            let sizes: Vec<Vec2> = canvases
                .iter()
                .zip(&scales)
                .map(|(c, s)| match c.quads.is_empty() {
                    true => Vec2::ZERO,
                    false => (extent(c) * s).ceil(),
                })
                .collect();
            if let Some(corners) = shelves(&sizes, side as f32) {
                break corners
                    .into_iter()
                    .zip(scales)
                    .zip(sizes)
                    .collect::<Vec<_>>();
            }
            if side < LARGEST_SIDE {
                side *= 2;
            } else {
                shrink *= 0.5;
            }
        };
        if self.atlas.as_ref().is_none_or(|a| a.side != side) {
            self.atlas = Some(self.new_atlas(device, side));
        }
        let texel = 1.0 / side as f32;
        let mut quads = Vec::new();
        for (c, ((corner, scale), size)) in canvases.iter().zip(corners) {
            if c.quads.is_empty() {
                self.slots.push(None);
                continue;
            }
            let lo = Vec2::new(c.reach[0], c.reach[1]);
            let hi = corner + size;
            let rect = [corner.x, corner.y, hi.x, hi.y];
            for q in &c.quads {
                let clip = [
                    q.clip[0].max(c.reach[0]),
                    q.clip[1].max(c.reach[1]),
                    q.clip[2].min(c.reach[2]),
                    q.clip[3].min(c.reach[3]),
                ];
                let at = |x: f32, y: f32| (Vec2::new(x, y) - lo) * scale + corner;
                let (a, b) = (at(clip[0], clip[1]), at(clip[2], clip[3]));
                quads.push(GuiQuad {
                    center: (q.center - lo) * scale + corner,
                    size: q.size * scale,
                    clip: [
                        a.x.max(rect[0]),
                        a.y.max(rect[1]),
                        b.x.min(rect[2]),
                        b.y.min(rect[3]),
                    ],
                    ..q.clone()
                });
            }
            // The slot's edge is the reach's edge, so its pixels map back
            // to canvas pixels exactly.
            let shown = (Vec2::new(c.reach[2], c.reach[3]) - lo) * scale;
            self.slots.push(Some(Slot {
                uv: [
                    corner.x * texel,
                    corner.y * texel,
                    (corner.x + shown.x) * texel,
                    (corner.y + shown.y) * texel,
                ],
                reach: c.reach,
                size: c.size,
                light_influence: c.light_influence.clamp(0.0, 1.0),
                brightness: c.brightness,
                on_top: c.always_on_top,
                max_distance: c.max_distance,
            }));
        }
        quads
    }

    fn new_atlas(&self, device: &wgpu::Device, side: u32) -> Atlas {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rook world gui atlas"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::GUI_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook world gui"),
            layout: &self.group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        Atlas { view, side, group }
    }

    /// Where each packed canvas is from now on.
    pub(crate) fn place(&mut self, places: Vec<Option<WorldPlace>>) {
        self.places = places;
    }

    /// Upload the canvases `camera` sees in a `height`-pixel window, back
    /// to front, those always on top last.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &Camera,
        height: u32,
    ) -> WorldRuns {
        if self.atlas.is_none() {
            return [0..0, 0..0];
        }
        let mut drawn: Vec<(bool, f32, CanvasInstance)> = self
            .slots
            .iter()
            .zip(&self.places)
            .filter_map(|(slot, place)| {
                let (slot, place) = (slot.as_ref()?, place.as_ref()?);
                let (origin, right, down, one_sided) = lay(slot, place, camera, height)?;
                let middle = origin + (right + down) * 0.5;
                let distance = middle.distance(camera.position);
                if distance > slot.max_distance {
                    return None;
                }
                let instance = CanvasInstance {
                    origin: origin.to_array(),
                    right: right.to_array(),
                    down: down.to_array(),
                    uv: slot.uv,
                    look: [slot.light_influence, slot.brightness],
                    one_sided: if one_sided { 1.0 } else { 0.0 },
                };
                Some((slot.on_top, distance, instance))
            })
            .collect();
        drawn.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)));
        let tested = drawn.iter().filter(|d| !d.0).count() as u32;
        let instances: Vec<CanvasInstance> = drawn.into_iter().map(|d| d.2).collect();
        if instances.is_empty() {
            return [0..0, 0..0];
        }
        match &self.buffer {
            Some((buffer, room)) if *room >= instances.len() => {
                queue.write_buffer(buffer, 0, bytemuck::cast_slice(&instances));
            }
            _ => {
                let room = instances.len().next_power_of_two();
                let mut padded = instances.clone();
                padded.resize(room, CanvasInstance::zeroed());
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("rook world gui"),
                    contents: bytemuck::cast_slice(&padded),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
                self.buffer = Some((buffer, room));
            }
        }
        [0..tested, tested..instances.len() as u32]
    }

    /// Draw the frame's canvases (last in the main pass).
    pub(crate) fn record(&self, pass: &mut wgpu::RenderPass<'_>, runs: &WorldRuns) {
        let (Some(atlas), Some((buffer, _))) = (&self.atlas, &self.buffer) else {
            return;
        };
        pass.set_bind_group(2, &atlas.group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        for (pipeline, range) in [&self.tested, &self.on_top].into_iter().zip(runs) {
            if !range.is_empty() {
                pass.set_pipeline(pipeline);
                pass.draw(0..6, range.clone());
            }
        }
    }
}

/// Top left corners for rectangles of `sizes` in rows on a `side`-pixel
/// square, tallest first, or `None` if they don't fit.
fn shelves(sizes: &[Vec2], side: f32) -> Option<Vec<Vec2>> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| sizes[b].y.total_cmp(&sizes[a].y));
    let mut corners = vec![Vec2::ZERO; sizes.len()];
    let (mut x, mut y, mut row) = (0.0, 0.0, 0.0f32);
    for i in order {
        let s = sizes[i];
        if s.x <= 0.0 || s.y <= 0.0 {
            continue;
        }
        if s.x > side {
            return None;
        }
        if x + s.x > side {
            (x, y, row) = (0.0, y + row + GAP, 0.0);
        }
        if y + s.y > side {
            return None;
        }
        corners[i] = Vec2::new(x, y);
        x += s.x + GAP;
        row = row.max(s.y);
    }
    Some(corners)
}

/// A canvas's shown part in the world: its top left corner, its whole
/// width and height as vectors, and whether it's seen only from in front.
fn lay(
    slot: &Slot,
    place: &WorldPlace,
    camera: &Camera,
    height: u32,
) -> Option<(Vec3, Vec3, Vec3, bool)> {
    let (corner, step_right, step_down, one_sided) = match *place {
        WorldPlace::Surface {
            origin,
            right,
            down,
        } => {
            let lift = down.cross(right).normalize_or_zero() * LIFT;
            (origin + lift, right, down, true)
        }
        WorldPlace::Billboard {
            anchor,
            studs,
            size_studs,
            size_pixels,
            shift,
        } => {
            let [right, up, back] = camera.rotation.to_cols_array_2d().map(Vec3::from);
            let center = anchor + right * studs.x + up * studs.y + back * studs.z;
            let depth = (center - camera.position).dot(-back);
            if depth <= 0.05 || slot.size.x <= 0.0 || slot.size.y <= 0.0 {
                return None;
            }
            let pixels_per_stud =
                height as f32 / (2.0 * (camera.fov_y.to_radians() * 0.5).tan() * depth);
            let world = size_studs + size_pixels / pixels_per_stud;
            let center = center + right * shift.x * world.x + up * shift.y * world.y;
            let corner = center - right * world.x * 0.5 + up * world.y * 0.5;
            (
                corner,
                right * world.x / slot.size.x,
                -up * world.y / slot.size.y,
                false,
            )
        }
    };
    let [left, top, right, bottom] = slot.reach;
    Some((
        corner + step_right * left + step_down * top,
        step_right * (right - left),
        step_down * (bottom - top),
        one_sided,
    ))
}
