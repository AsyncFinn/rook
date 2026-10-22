//! World GUI on the GPU: each SurfaceGui's and BillboardGui's quads drawn
//! straight into the world on its canvas's plane, after the particles,
//! depth tested unless it's AlwaysOnTop, so edges and text stay sharp
//! however near they're seen (docs/behavior/gui/world.md). The quads stay
//! on the GPU until the GUI changes; each frame only where each canvas is
//! goes up, in a uniform its draw reads, bound with each run's image.

use glam::Vec2;
use rook_types::{Viewer, WorldCanvas, WorldPlace};
use wesl_core::include_wesl;

use super::super::*;
use super::images::{GuiImages, ImageKey, Run, image_entries, runs};
use super::{GpuQuad, Packed};
use crate::renderer::effects::particles::blended_pipeline;
use crate::renderer::pipelines::BindLayouts;

/// How far a SurfaceGui stands off its face, so the two don't fight.
const LIFT: f32 = 0.01;

/// Where a canvas is this frame, as the shader reads it: its top left
/// corner and the world step per canvas pixel right and down,
/// then LightInfluence, Brightness and whether it's seen only from in
/// front.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CanvasFrame {
    origin: [f32; 4],
    right: [f32; 4],
    down: [f32; 4],
    look: [f32; 4],
}

/// A canvas's runs of quads in the quad buffer, and what it's drawn with.
#[derive(Debug, Clone)]
struct Canvas {
    runs: Vec<Run>,
    size: Vec2,
    light_influence: f32,
    brightness: f32,
    on_top: bool,
    max_distance: f32,
}

/// One canvas drawn this frame: its frame's slot in the uniform buffer,
/// which canvas it is, and whether it's drawn over everything.
#[derive(Debug, Clone)]
pub(crate) struct WorldDraw {
    slot: u32,
    canvas: usize,
    on_top: bool,
}

/// The frame's canvases, back to front, those always on top last.
pub(crate) type WorldRuns = Vec<WorldDraw>;

/// The frames' uniform buffer, how many it holds, and its bind groups,
/// one per image runs draw with.
#[derive(Debug)]
struct Frames {
    buffer: wgpu::Buffer,
    room: usize,
    groups: HashMap<Option<ImageKey>, wgpu::BindGroup>,
}

#[derive(Debug)]
pub(crate) struct WorldGui {
    tested: wgpu::RenderPipeline,
    on_top: wgpu::RenderPipeline,
    group_layout: wgpu::BindGroupLayout,
    /// The glyph atlas, which text quads draw through.
    glyphs: wgpu::TextureView,
    /// Bytes between frames in the uniform buffer.
    stride: u64,
    canvases: Vec<Canvas>,
    quads: Option<wgpu::Buffer>,
    /// Where each canvas is now.
    places: Vec<Option<WorldPlace>>,
    frames: Option<Frames>,
}

impl WorldGui {
    pub(crate) fn new(
        device: &wgpu::Device,
        layouts: BindLayouts<'_>,
        table: bool,
        glyphs: wgpu::TextureView,
    ) -> Self {
        let size = std::mem::size_of::<CanvasFrame>() as u64;
        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook world gui"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(size),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                image_entries(2)[0],
                image_entries(2)[1],
            ],
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
        let pipeline = |compare| {
            blended_pipeline(
                device,
                "rook world gui",
                &layout,
                (&shader, "vs_world_gui", "fs_world_gui"),
                GpuQuad::layout(),
                (FRAME_FORMAT, main_depth_format(device)),
                compare,
            )
        };
        let align = device.limits().min_uniform_buffer_offset_alignment as u64;
        Self {
            tested: pipeline(wgpu::CompareFunction::Greater),
            on_top: pipeline(wgpu::CompareFunction::Always),
            group_layout,
            glyphs,
            stride: size.div_ceil(align) * align,
            canvases: Vec::new(),
            quads: None,
            places: Vec::new(),
            frames: None,
        }
    }

    /// Draw `canvases` from now on, `packed` being each one's quads as the
    /// shader reads them.
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        canvases: &[WorldCanvas],
        packed: &[Packed],
    ) {
        self.canvases.clear();
        let mut all: Vec<GpuQuad> = Vec::new();
        for (c, quads) in canvases.iter().zip(packed) {
            let start = all.len() as u32;
            all.extend(quads.iter().map(|(q, _)| *q));
            let mut runs = runs(quads.iter().map(|(_, image)| *image));
            for run in &mut runs {
                run.quads = run.quads.start + start..run.quads.end + start;
            }
            self.canvases.push(Canvas {
                runs,
                size: c.size,
                light_influence: c.light_influence.clamp(0.0, 1.0),
                brightness: c.brightness,
                on_top: c.always_on_top,
                max_distance: c.max_distance,
            });
        }
        self.quads = (!all.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rook world gui quads"),
                contents: bytemuck::cast_slice(&all),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
    }

    /// Where each canvas is from now on.
    pub(crate) fn place(&mut self, places: Vec<Option<WorldPlace>>) {
        self.places = places;
    }

    /// Upload where each canvas `camera` sees in a `height`-pixel window
    /// is, back to front, those always on top last, and bind the images
    /// (in `images`' cache) their runs draw with.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: (&GuiImages, &GpuCache<ImageHash, Image>),
        camera: &Camera,
        height: u32,
    ) -> WorldRuns {
        if self.quads.is_none() {
            return Vec::new();
        }
        let mut seen: Vec<(bool, f32, CanvasFrame, usize)> = self
            .canvases
            .iter()
            .enumerate()
            .zip(&self.places)
            .filter(|((_, c), _)| !c.runs.is_empty())
            .filter_map(|((i, c), place)| {
                let (origin, right, down, one_sided) = lay(c, place.as_ref()?, camera, height)?;
                let middle = origin + (right * c.size.x + down * c.size.y) * 0.5;
                let distance = middle.distance(camera.position);
                if distance > c.max_distance {
                    return None;
                }
                let frame = CanvasFrame {
                    origin: origin.extend(1.0).to_array(),
                    right: right.extend(0.0).to_array(),
                    down: down.extend(0.0).to_array(),
                    look: [
                        c.light_influence,
                        c.brightness,
                        if one_sided { 1.0 } else { 0.0 },
                        0.0,
                    ],
                };
                Some((c.on_top, distance, frame, i))
            })
            .collect();
        seen.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)));
        if seen.is_empty() {
            return Vec::new();
        }
        if self.frames.as_ref().is_none_or(|f| f.room < seen.len()) {
            self.frames = Some(self.new_frames(device, seen.len().next_power_of_two()));
        }
        let keys: Vec<Option<ImageKey>> = (seen.iter())
            .flat_map(|s| self.canvases[s.3].runs.iter().map(|r| r.image))
            .collect();
        for key in keys {
            self.bind(device, key, images);
        }
        let frames = self.frames.as_ref().expect("made above");
        let mut bytes = vec![0u8; seen.len() * self.stride as usize];
        for (i, s) in seen.iter().enumerate() {
            let at = i * self.stride as usize;
            let frame = bytemuck::bytes_of(&s.2);
            bytes[at..at + frame.len()].copy_from_slice(frame);
        }
        queue.write_buffer(&frames.buffer, 0, &bytes);
        seen.into_iter()
            .enumerate()
            .map(|(i, (on_top, _, _, canvas))| WorldDraw {
                slot: i as u32,
                canvas,
                on_top,
            })
            .collect()
    }

    /// Make the frames' bind group for `image`, once it's on the GPU.
    fn bind(
        &mut self,
        device: &wgpu::Device,
        image: Option<ImageKey>,
        (images, cache): (&GuiImages, &GpuCache<ImageHash, Image>),
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
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook world gui"),
            layout: &self.group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &frames.buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<CanvasFrame>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.glyphs),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        frames.groups.insert(image, group);
    }

    fn new_frames(&self, device: &wgpu::Device, room: usize) -> Frames {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rook world gui frames"),
            size: room as u64 * self.stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Frames {
            buffer,
            room,
            groups: HashMap::default(),
        }
    }

    /// Draw the frame's canvases (last in the main pass).
    pub(crate) fn record(&self, pass: &mut wgpu::RenderPass<'_>, runs: &WorldRuns) {
        let (Some(quads), Some(frames)) = (&self.quads, &self.frames) else {
            return;
        };
        pass.set_vertex_buffer(0, quads.slice(..));
        let mut on_top = None;
        for d in runs {
            if on_top != Some(d.on_top) {
                on_top = Some(d.on_top);
                pass.set_pipeline(if d.on_top { &self.on_top } else { &self.tested });
            }
            let offset = (d.slot as u64 * self.stride) as u32;
            // Runs whose image isn't up yet draw nothing.
            for run in &self.canvases[d.canvas].runs {
                if let Some(group) = frames.groups.get(&run.image) {
                    pass.set_bind_group(2, group, &[offset]);
                    pass.draw(0..6, run.quads.clone());
                }
            }
        }
    }
}

/// A canvas in the world: its top left corner, the world step per canvas
/// pixel right and down, and whether it's seen only from in front.
fn lay(
    canvas: &Canvas,
    place: &WorldPlace,
    camera: &Camera,
    height: u32,
) -> Option<(Vec3, Vec3, Vec3, bool)> {
    let viewer = Viewer {
        position: camera.position,
        rotation: camera.rotation,
        fov_y: camera.fov_y,
        height: height as f32,
    };
    let f = place.frame(canvas.size, &viewer)?;
    let lift = if f.one_sided {
        f.down.cross(f.right).normalize_or_zero() * LIFT
    } else {
        Vec3::ZERO
    };
    Some((f.corner + lift, f.right, f.down, f.one_sided))
}
