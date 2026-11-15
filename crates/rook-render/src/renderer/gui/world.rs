//! World GUI on the GPU: each SurfaceGui's and BillboardGui's quads drawn
//! straight into the world on its canvas's plane, among the transparent
//! parts by distance (those nearer than every one after the particles),
//! depth tested unless it's AlwaysOnTop, so edges and text stay sharp
//! however near they're seen (docs/behavior/gui/world.md). Untoned
//! canvases (the names over heads) are drawn with the screen's GUI
//! instead, moved to where they are on the screen each frame.
//!
//! Every canvas's quads share one buffer, each beside its canvas's id,
//! and stay there until that canvas changes. Each frame only where each
//! canvas is goes up, into a storage array the quads read by id, so the
//! canvases drawn in a row with one image are one indirect multi-draw, as
//! the parts' runs are: hundreds of canvases cut in among the transparent
//! parts, and a draw with its own buffer and frame offset each cost more
//! to encode than to draw.

use glam::Vec2;
use rustc_hash::FxHashMap;
use std::ops::Range;
use std::sync::Arc;

use rook_types::{Viewer, WorldCanvas, WorldPlace};
use wesl_core::include_wesl;

use super::super::*;
use super::images::{GuiImages, ImageKey, Run, image_entries, runs};
use super::{GpuQuad, Held, Packed};
use crate::gpu::pool::RangeAlloc;
use crate::renderer::effects::particles::blended_pipeline;
use crate::renderer::pipelines::BindLayouts;
use crate::renderer::runs::multi_draw;

/// How far a SurfaceGui stands off its face, so the two don't fight.
const LIFT: f32 = 0.01;

/// The shared quad buffer's first room, in quads.
const FIRST_QUADS: u32 = 1 << 12;
/// The bytes of a quad, and of the canvas id beside it.
const QUAD: u64 = std::mem::size_of::<GpuQuad>() as u64;
const ID: u64 = 4;

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

/// One draw of [`wgpu::RenderPass::multi_draw_indirect`]'s list.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct DrawArgs {
    vertex_count: u32,
    instance_count: u32,
    first_vertex: u32,
    first_instance: u32,
}

/// A canvas's quads and their runs, and what it's drawn with.
#[derive(Debug)]
struct Canvas {
    /// The canvas as last given, and the groups it holds.
    source: Arc<WorldCanvas>,
    held: Held,
    /// Its quads' place in the shared buffer (empty: none drawn there),
    /// and its id, where its frame is in the frames' array.
    quads: Range<u32>,
    id: u32,
    /// Its runs, counted from its first quad.
    runs: Vec<Run>,
    /// The corners of a box around every quad, in canvas pixels: quads
    /// can reach past the canvas.
    extent: (Vec2, Vec2),
    size: Vec2,
    light_influence: f32,
    brightness: f32,
    on_top: bool,
    max_distance: f32,
    /// An untoned canvas's quads, kept here to draw with the screen's GUI.
    overlay: Option<Packed>,
}

/// One canvas drawn this frame: its runs' draws in the frame's list,
/// whether it's drawn over everything, how far its middle is from the
/// camera, and where its quads are.
#[derive(Debug, Clone)]
pub(crate) struct WorldDraw {
    draws: Range<u32>,
    pub(crate) on_top: bool,
    pub(crate) distance: f32,
    /// The corners of a box in the world around its quads.
    pub(crate) corners: [Vec3; 4],
}

/// The frame's canvases, back to front, those always on top last.
pub(crate) type WorldRuns = Vec<WorldDraw>;

/// One draw this frame: its arguments, the image it draws with and
/// whether it's on top.
type Draw = (DrawArgs, Option<ImageKey>, bool);

/// Every canvas's quads, and beside each the id of the canvas it's on.
#[derive(Debug)]
struct Quads {
    quads: wgpu::Buffer,
    ids: wgpu::Buffer,
    capacity: u32,
    ranges: RangeAlloc,
}

impl Quads {
    fn new(device: &wgpu::Device, capacity: u32) -> Self {
        let buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        Self {
            quads: buffer("rook world gui quads", u64::from(capacity) * QUAD),
            ids: buffer("rook world gui quad canvases", u64::from(capacity) * ID),
            capacity,
            ranges: RangeAlloc::default(),
        }
    }

    /// Room for `len` quads, the buffers grown (keeping what they hold)
    /// if they must be; none past the device's biggest buffer.
    fn alloc(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, len: u32) -> Option<u32> {
        if let Some(at) = self.ranges.alloc(len, self.capacity) {
            return Some(at);
        }
        let max = (device.limits().max_buffer_size / QUAD).min(u64::from(u32::MAX)) as u32;
        let need = (self.ranges.top().checked_add(len)).filter(|&n| n <= max)?;
        let grown = Self::new(device, self.capacity.saturating_mul(2).clamp(need, max));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rook world gui quads growth"),
        });
        let used = u64::from(self.ranges.top());
        encoder.copy_buffer_to_buffer(&self.quads, 0, &grown.quads, 0, used * QUAD);
        encoder.copy_buffer_to_buffer(&self.ids, 0, &grown.ids, 0, used * ID);
        // Writes queued to the old buffers land before this copy.
        queue.submit([encoder.finish()]);
        (self.quads, self.ids, self.capacity) = (grown.quads, grown.ids, grown.capacity);
        self.ranges.alloc(len, self.capacity)
    }
}

/// The frames' storage array, how many it holds, and its bind groups,
/// one per image runs draw with.
#[derive(Debug)]
struct Frames {
    buffer: wgpu::Buffer,
    room: u32,
    groups: FxHashMap<Option<ImageKey>, wgpu::BindGroup>,
}

#[derive(Debug)]
pub(crate) struct WorldGui {
    tested: wgpu::RenderPipeline,
    on_top: wgpu::RenderPipeline,
    shader: wgpu::ShaderModule,
    /// Group 3: the light group's entries, then the canvases' frames and
    /// what runs draw with. Groups 0 to 2 are the parts', so drawing
    /// canvases among the transparent parts leaves the parts' images
    /// bound; only this group changes.
    group_layout: wgpu::BindGroupLayout,
    /// The glyph atlas, which text quads draw through, and the
    /// gradients' ramps.
    glyphs: wgpu::TextureView,
    ramps: wgpu::TextureView,
    canvases: Vec<Canvas>,
    /// Where each canvas is now (`None`: not drawn, or not placed yet).
    places: Vec<Option<WorldPlace>>,
    /// The canvas each place is for, as last given, so a canvas given
    /// again keeps its place: only new and moved ones are placed.
    placed: Vec<Arc<WorldCanvas>>,
    quads: Option<Quads>,
    /// The canvases' ids, and each id's frame as last laid out.
    ids: RangeAlloc,
    laid: Vec<CanvasFrame>,
    frames: Option<Frames>,
    /// The frame's draws, in the order drawn, and the buffer they're
    /// uploaded to and its room.
    draws: Vec<Draw>,
    indirect: Option<(wgpu::Buffer, usize)>,
    /// Whether the device draws a list of draws in one call.
    multi_draw: bool,
}

impl WorldGui {
    pub(crate) fn new(
        device: &wgpu::Device,
        layouts: BindLayouts<'_>,
        table: bool,
        (glyphs, ramps): (&wgpu::TextureView, &wgpu::TextureView),
    ) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let mut entries = GpuLights::layout_entries().to_vec();
        entries.extend([
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<CanvasFrame>() as u64
                    ),
                },
                count: None,
            },
            texture(5),
            image_entries(6)[0],
            image_entries(6)[1],
            texture(8),
        ]);
        let group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rook world gui"),
            entries: &entries,
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
        let [tested, on_top] = pipelines(device, layouts, &group_layout, &shader);
        Self {
            tested,
            on_top,
            shader,
            group_layout,
            glyphs: glyphs.clone(),
            ramps: ramps.clone(),
            canvases: Vec::new(),
            places: Vec::new(),
            placed: Vec::new(),
            quads: None,
            ids: RangeAlloc::default(),
            laid: Vec::new(),
            frames: None,
            draws: Vec::new(),
            indirect: None,
            multi_draw: multi_draw(device),
        }
    }

    /// Make the pipelines again for the parts' new image layout (the
    /// table grew).
    pub(crate) fn rebuild(&mut self, device: &wgpu::Device, layouts: BindLayouts<'_>) {
        [self.tested, self.on_top] = pipelines(device, layouts, &self.group_layout, &self.shader);
    }

    /// For each of `canvases`, the canvas drawn now it was given as (the
    /// same `Arc`), if any.
    pub(crate) fn kept(&self, canvases: &[Arc<WorldCanvas>]) -> Vec<Option<usize>> {
        let at: FxHashMap<*const WorldCanvas, usize> = (self.canvases.iter().enumerate())
            .map(|(i, c)| (Arc::as_ptr(&c.source), i))
            .collect();
        (canvases.iter())
            .map(|c| at.get(&Arc::as_ptr(c)).copied())
            .collect()
    }

    /// The groups the `kept` canvases hold.
    pub(crate) fn held<'a>(&'a self, kept: &'a [Option<usize>]) -> impl Iterator<Item = &'a Held> {
        kept.iter().flatten().map(|&i| &self.canvases[i].held)
    }

    /// The groups the canvases hold, but for those `replaced` and those at
    /// `len` or past it.
    pub(crate) fn held_but<'a>(
        &'a self,
        replaced: &'a rustc_hash::FxHashSet<usize>,
        len: usize,
    ) -> impl Iterator<Item = &'a Held> {
        (self.canvases.iter().enumerate().take(len))
            .filter(|(i, _)| !replaced.contains(i))
            .map(|(_, c)| &c.held)
    }

    /// The canvases as last given.
    pub(crate) fn sources(&self) -> Vec<Arc<WorldCanvas>> {
        self.placed.clone()
    }

    /// Drop every canvas, so each is given again.
    pub(crate) fn forget(&mut self) {
        self.canvases.clear();
        if let Some(quads) = &mut self.quads {
            quads.ranges = RangeAlloc::default();
        }
        self.ids = RangeAlloc::default();
    }

    /// Draw `canvases` from now on: those [`WorldGui::kept`] found as they
    /// are, the rest from `fresh`, in order, each one's quads as the shader
    /// reads them and the groups it holds.
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvases: &[Arc<WorldCanvas>],
        kept: Vec<Option<usize>>,
        fresh: Vec<(Packed, Held)>,
    ) {
        let mut old: Vec<Option<Canvas>> = self.canvases.drain(..).map(Some).collect();
        let kept: Vec<Option<Canvas>> = (kept.into_iter())
            .map(|k| k.and_then(|i| old[i].take()))
            .collect();
        // What's dropped gives its room back first, for what's new.
        for canvas in old.into_iter().flatten() {
            self.free(&canvas);
        }
        // A canvas given before stays where it was; a new one has no
        // place until it's given one.
        let was: FxHashMap<*const WorldCanvas, usize> = (self.placed.iter().enumerate())
            .map(|(i, c)| (Arc::as_ptr(c), i))
            .collect();
        let mut places = std::mem::take(&mut self.places);
        self.places = (canvases.iter())
            .map(|c| places.get_mut(*was.get(&Arc::as_ptr(c))?)?.take())
            .collect();
        self.placed = canvases.to_vec();
        let mut fresh = fresh.into_iter();
        for (c, kept) in canvases.iter().zip(kept) {
            if let Some(canvas) = kept {
                self.canvases.push(canvas);
                continue;
            }
            let (quads, held) = fresh.next().expect("one per canvas not kept");
            let canvas = self.make(device, queue, c, quads, held);
            self.canvases.push(canvas);
        }
    }

    /// Draw the canvases at `changed`'s indices (in order) from `fresh`
    /// from now on, with the list `len` long: those past it are dropped,
    /// and the rest stay as they are, where they are. A canvas at a new
    /// index has no place until it's given one.
    pub(crate) fn replace(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        len: usize,
        changed: &[(usize, Arc<WorldCanvas>)],
        fresh: Vec<(Packed, Held)>,
    ) {
        for canvas in self.canvases.split_off(len.min(self.canvases.len())) {
            self.free(&canvas);
        }
        self.places.truncate(len);
        self.placed.truncate(len);
        for ((i, c), (quads, held)) in changed.iter().zip(fresh) {
            // What's replaced gives its room back first, for what's new.
            if let Some(old) = self.canvases.get(*i) {
                let (quads, id) = (old.quads.clone(), old.id);
                self.give_back(quads, id);
            }
            let canvas = self.make(device, queue, c, quads, held);
            if *i < self.canvases.len() {
                self.canvases[*i] = canvas;
                self.placed[*i] = c.clone();
            } else {
                assert_eq!(*i, self.canvases.len(), "new canvases come in order");
                self.canvases.push(canvas);
                self.placed.push(c.clone());
                self.places.push(None);
            }
        }
    }

    /// Canvas `c` with its quads, as the shader reads them, put in the
    /// shared buffer.
    fn make(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        c: &Arc<WorldCanvas>,
        quads: Packed,
        held: Held,
    ) -> Canvas {
        let gpu: Vec<GpuQuad> = match c.untoned {
            true => Vec::new(),
            false => quads.iter().map(|(q, _)| *q).collect(),
        };
        let (at, id) = self.put(device, queue, &gpu).unwrap_or((0..0, 0));
        Canvas {
            source: c.clone(),
            held,
            quads: at,
            id,
            runs: runs(quads.iter().map(|(_, image)| *image)),
            extent: extent(&gpu),
            size: c.size,
            light_influence: c.light_influence.clamp(0.0, 1.0),
            brightness: c.brightness,
            on_top: c.always_on_top,
            max_distance: c.max_distance,
            overlay: c.untoned.then_some(quads),
        }
    }

    /// Put a canvas's quads in the shared buffer, each beside a new id
    /// for the canvas: where they are, and the id; `None` when there are
    /// none, or no room.
    fn put(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quads: &[GpuQuad],
    ) -> Option<(Range<u32>, u32)> {
        let len = u32::try_from(quads.len()).ok().filter(|&n| n > 0)?;
        let shared = (self.quads).get_or_insert_with(|| Quads::new(device, FIRST_QUADS));
        let at = shared.alloc(device, queue, len)?;
        let id = self
            .ids
            .alloc(1, u32::MAX)
            .expect("fewer canvases than ids");
        let ids = vec![id; quads.len()];
        queue.write_buffer(
            &shared.quads,
            u64::from(at) * QUAD,
            bytemuck::cast_slice(quads),
        );
        queue.write_buffer(&shared.ids, u64::from(at) * ID, bytemuck::cast_slice(&ids));
        Some((at..at + len, id))
    }

    /// Give a dropped canvas's quads and id back.
    fn free(&mut self, canvas: &Canvas) {
        self.give_back(canvas.quads.clone(), canvas.id);
    }

    /// Give a canvas's `quads` and `id` back.
    fn give_back(&mut self, quads: Range<u32>, id: u32) {
        if quads.is_empty() {
            return;
        }
        if let Some(shared) = &mut self.quads {
            shared.ranges.free(quads);
        }
        self.ids.free(id..id + 1);
    }

    /// Drop the bind groups of groups' textures, which were drawn anew.
    pub(crate) fn forget_groups(&mut self) {
        if let Some(frames) = &mut self.frames {
            (frames.groups).retain(|key, _| !matches!(key, Some(ImageKey::Group(_))));
        }
    }

    /// Where the canvases at these indices (in the last list given) are
    /// from now on; the others stay where they were.
    pub(crate) fn place(&mut self, places: impl IntoIterator<Item = (usize, Option<WorldPlace>)>) {
        for (i, place) in places {
            if let Some(slot) = self.places.get_mut(i) {
                *slot = place;
            }
        }
    }

    /// Upload where each canvas `camera` sees in a `height`-pixel window
    /// is, and its runs' draws, back to front, those always on top last,
    /// and bind the images (in `images`' cache) the runs draw with.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: (&GuiImages, &GpuCache<ImageHash, Image>),
        lights: &GpuLights,
        (camera, view): (&Camera, &Frustum),
        height: u32,
    ) -> WorldRuns {
        self.draws.clear();
        self.laid
            .resize(self.ids.top() as usize, CanvasFrame::zeroed());
        let mut seen: Vec<(bool, f32, usize, [Vec3; 4])> = Vec::new();
        for (i, (c, place)) in self.canvases.iter().zip(&self.places).enumerate() {
            let Some(place) = place.as_ref().filter(|_| !c.quads.is_empty()) else {
                continue;
            };
            let Some((origin, right, down, one_sided)) = lay(c, place, camera, height) else {
                continue;
            };
            let middle = origin + (right * c.size.x + down * c.size.y) * 0.5;
            let distance = middle.distance(camera.position);
            if distance > c.max_distance {
                continue;
            }
            // Wholly out of view: drawing it, and sorting it among the
            // transparent parts, would change no pixel.
            let (low, high) = c.extent;
            let at = |p: Vec2| origin + right * p.x + down * p.y;
            let corners = [
                low,
                Vec2::new(high.x, low.y),
                Vec2::new(low.x, high.y),
                high,
            ]
            .map(at);
            let bounds = Aabb {
                min: corners.into_iter().reduce(Vec3::min).expect("four corners"),
                max: corners.into_iter().reduce(Vec3::max).expect("four corners"),
            };
            if !view.sees(bounds, 0.0) {
                continue;
            }
            self.laid[c.id as usize] = CanvasFrame {
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
            seen.push((c.on_top, distance, i, corners));
        }
        seen.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)));
        if seen.is_empty() {
            return Vec::new();
        }
        if self.frames.as_ref().is_none_or(|f| f.room < self.ids.top()) {
            self.frames = Some(new_frames(device, self.ids.top().next_power_of_two()));
        }
        let keys: Vec<Option<ImageKey>> = (seen.iter())
            .flat_map(|s| self.canvases[s.2].runs.iter().map(|r| r.image))
            .collect();
        for key in keys {
            self.bind(device, key, images, lights);
        }
        let frames = self.frames.as_ref().expect("made above");
        queue.write_buffer(&frames.buffer, 0, bytemuck::cast_slice(&self.laid));
        let mut out = Vec::with_capacity(seen.len());
        for (on_top, distance, i, corners) in seen {
            let c = &self.canvases[i];
            let first = self.draws.len() as u32;
            // Runs whose image isn't up yet draw nothing.
            let ready = (c.runs.iter()).filter(|r| frames.groups.contains_key(&r.image));
            self.draws.extend(ready.map(|r| {
                let args = DrawArgs {
                    vertex_count: 6,
                    instance_count: r.quads.len() as u32,
                    first_vertex: 0,
                    first_instance: c.quads.start + r.quads.start,
                };
                (args, r.image, on_top)
            }));
            out.push(WorldDraw {
                draws: first..self.draws.len() as u32,
                on_top,
                distance,
                corners,
            });
        }
        rook_types::count!(RenderGuiCanvases, out.len());
        if self.multi_draw {
            self.upload_draws(device, queue);
        }
        out
    }

    /// Upload the frame's draws for the multi-draws to read.
    fn upload_draws(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let len = self.draws.len();
        if len == 0 {
            return;
        }
        if self.indirect.as_ref().is_none_or(|(_, room)| *room < len) {
            let room = len.next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rook world gui draws"),
                size: (room * std::mem::size_of::<DrawArgs>()) as u64,
                usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.indirect = Some((buffer, room));
        }
        let (buffer, _) = self.indirect.as_ref().expect("made above");
        let args: Vec<DrawArgs> = self.draws.iter().map(|d| d.0).collect();
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(&args));
    }

    /// The untoned canvases' quads where `camera` sees them in a `size`
    /// window, as screen GUI quads, back to front.
    pub(crate) fn overlay(&self, camera: &Camera, size: (u32, u32)) -> Packed {
        let mut seen: Vec<(f32, Vec2, f32, &Packed)> = (self.canvases.iter().zip(&self.places))
            .filter_map(|(c, place)| {
                let quads = c.overlay.as_ref()?;
                let (origin, right, _, _) = lay(c, place.as_ref()?, camera, size.1)?;
                let corner = on_screen(origin, camera, size)?;
                let k = on_screen(origin + right, camera, size)?.distance(corner);
                let distance = origin.distance(camera.position);
                (distance <= c.max_distance).then_some((distance, corner, k, quads))
            })
            .collect();
        seen.sort_by(|a, b| b.0.total_cmp(&a.0));
        (seen.into_iter())
            .flat_map(|(_, corner, k, quads)| {
                quads
                    .iter()
                    .map(move |(q, image)| (q.moved(corner, k), *image))
            })
            .collect()
    }

    /// Make the frames' bind group for `image`, once it's on the GPU.
    fn bind(
        &mut self,
        device: &wgpu::Device,
        image: Option<ImageKey>,
        (images, cache): (&GuiImages, &GpuCache<ImageHash, Image>),
        lights: &GpuLights,
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
        let mut entries = lights.entries().to_vec();
        entries.extend([
            wgpu::BindGroupEntry {
                binding: 4,
                resource: frames.buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&self.glyphs),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(&self.ramps),
            },
        ]);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook world gui"),
            layout: &self.group_layout,
            entries: &entries,
        });
        frames.groups.insert(image, group);
    }

    /// Draw some of the frame's canvases, in the order given: the draws
    /// in a row with one image and pipeline in one call where the device
    /// can.
    pub(crate) fn record(&self, pass: &mut wgpu::RenderPass<'_>, runs: &[WorldDraw]) {
        let (Some(frames), Some(quads)) = (&self.frames, &self.quads) else {
            return;
        };
        let (Some(first), Some(last)) = (runs.first(), runs.last()) else {
            return;
        };
        // The canvases' draws are in the frame's list in the order drawn.
        let start = first.draws.start as usize;
        let draws = &self.draws[start..last.draws.end as usize];
        if draws.is_empty() {
            return;
        }
        pass.set_vertex_buffer(0, quads.quads.slice(..));
        pass.set_vertex_buffer(1, quads.ids.slice(..));
        let mut on_top = None;
        let mut at = start;
        let mut calls = 0;
        for batch in draws.chunk_by(|a, b| (a.1, a.2) == (b.1, b.2)) {
            let (_, image, top) = batch[0];
            if on_top != Some(top) {
                on_top = Some(top);
                pass.set_pipeline(if top { &self.on_top } else { &self.tested });
            }
            let group = (frames.groups.get(&image)).expect("only runs with a group draw");
            pass.set_bind_group(3, group, &[]);
            match &self.indirect {
                Some((buffer, _)) if self.multi_draw => {
                    let offset = (at * std::mem::size_of::<DrawArgs>()) as u64;
                    pass.multi_draw_indirect(buffer, offset, batch.len() as u32);
                    calls += 1;
                }
                _ => {
                    for (a, _, _) in batch {
                        let first = a.first_instance;
                        pass.draw(0..a.vertex_count, first..first + a.instance_count);
                        calls += 1;
                    }
                }
            }
            at += batch.len();
        }
        rook_types::count!(RenderGuiCalls, calls);
    }
}

/// A storage array of `room` canvases' frames, with no bind groups yet.
fn new_frames(device: &wgpu::Device, room: u32) -> Frames {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rook world gui frames"),
        size: u64::from(room.max(1)) * std::mem::size_of::<CanvasFrame>() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    Frames {
        buffer,
        room,
        groups: FxHashMap::default(),
    }
}

/// The world GUI's pipelines, depth tested and always on top: the parts'
/// groups 0 to 2 (`layouts`), then `group_layout`; the quads, then the
/// canvas each is on.
fn pipelines(
    device: &wgpu::Device,
    layouts: BindLayouts<'_>,
    group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> [wgpu::RenderPipeline; 2] {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("rook world gui"),
        bind_group_layouts: &[
            Some(layouts.globals),
            Some(layouts.main),
            Some(layouts.image),
            Some(group_layout),
        ],
        immediate_size: 0,
    });
    let ids = wgpu::vertex_attr_array![11 => Uint32];
    [
        wgpu::CompareFunction::Greater,
        wgpu::CompareFunction::Always,
    ]
    .map(|compare| {
        blended_pipeline(
            device,
            "rook world gui",
            &layout,
            (shader, "vs_world_gui", "fs_world_gui"),
            &[
                GpuQuad::layout(),
                wgpu::VertexBufferLayout {
                    array_stride: ID,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &ids,
                },
            ],
            (FRAME_FORMAT, main_depth_format(device)),
            compare,
        )
    })
}

/// The corners of a box in canvas pixels around `quads`, turned as they
/// may be.
fn extent(quads: &[GpuQuad]) -> (Vec2, Vec2) {
    let mut low = Vec2::splat(f32::INFINITY);
    let mut high = Vec2::splat(f32::NEG_INFINITY);
    for q in quads {
        // A turned quad stays within its half diagonal of its center.
        let reach = Vec2::splat(Vec2::from(q.half_size).length());
        low = low.min(Vec2::from(q.center) - reach);
        high = high.max(Vec2::from(q.center) + reach);
    }
    (low, high)
}

/// Where `point` is in a `size` window `camera` looks through, in pixels
/// from its top left; `None` behind the camera.
fn on_screen(point: Vec3, camera: &Camera, (width, height): (u32, u32)) -> Option<Vec2> {
    let v = camera.rotation.transpose() * (point - camera.position);
    if v.z > -0.05 {
        return None;
    }
    let f = height as f32 / (2.0 * (camera.fov_y.to_radians() * 0.5).tan());
    let at = Vec2::new(v.x, -v.y) * (f / -v.z);
    Some(Vec2::new(width as f32, height as f32) * 0.5 + at)
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
