//! Each frame's placing: where each canvas is goes up into the storage
//! array the quads read, the draws are worked out, and untoned canvases
//! are moved to where they are on the screen.

use super::*;

impl WorldGui {
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
    pub(super) fn upload_draws(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
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
}

/// A storage array of `room` canvases' frames, with no bind groups yet.
pub(super) fn new_frames(device: &wgpu::Device, room: u32) -> Frames {
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

/// The corners of a box in canvas pixels around `quads`, turned as they
/// may be.
pub(super) fn extent(quads: &[GpuQuad]) -> (Vec2, Vec2) {
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
pub(super) fn on_screen(point: Vec3, camera: &Camera, (width, height): (u32, u32)) -> Option<Vec2> {
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
pub(super) fn lay(
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
