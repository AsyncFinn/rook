//! Filling the voxels: which tiles a frame fills, from the lights and
//! blockers that changed and the volume following the camera, and the
//! compute pass that fills them.

use super::*;

/// The lowest voxel of a volume whose lowest corner is at `at` (in voxels):
/// along x and z the nearest whole tile, so each texel of the sun cover's
/// levels holds one tile of the world (`cover.rs`).
pub(super) fn snapped(at: Vec3) -> IVec3 {
    let tile = TILE as f32;
    IVec3::new(
        ((at.x / tile).round() * tile) as i32,
        at.y.floor() as i32,
        ((at.z / tile).round() * tile) as i32,
    )
}

/// The blocker changes in `spans` (the world's voxels) as boxes of the
/// volume starting at voxel `low`, one per tile column (or run of them)
/// with the changes' heights there joined: they reach the same tiles but
/// for their heights, so a crowd of small parts moving costs a few boxes.
pub(super) fn by_column(spans: &[Span], low: IVec3) -> Vec<Span> {
    let tile = TILE as i32;
    let mut columns: FxHashMap<(IVec2, IVec2), (i32, i32)> = FxHashMap::default();
    for &(lo, hi) in spans {
        let (lo, hi) = (lo - low, hi - low);
        let column = |v: IVec3| IVec2::new(v.x, v.z).div_euclid(IVec2::splat(tile));
        let key = (column(lo), column(hi));
        let heights = columns.entry(key).or_insert((lo.y, hi.y));
        *heights = (heights.0.min(lo.y), heights.1.max(hi.y));
    }
    // In any order: marking tiles is a union.
    columns
        .into_iter()
        .map(|((a, b), (y0, y1))| {
            (
                IVec3::new(a.x * tile, y0, a.y * tile),
                IVec3::new(b.x * tile + tile - 1, y1, b.y * tile + tile - 1),
            )
        })
        .collect()
}

/// What a blocker change reaches beyond its own voxels.
pub(super) struct Reach {
    /// The place lights with the sky.
    pub(super) sky: bool,
    /// The sun the voxels shadow (towards it).
    pub(super) sun: Option<Vec3>,
    /// [`GpuLights::shadowed_reach`].
    pub(super) shadowed: Vec<Shadowed>,
    /// With the sky's light, [`roofs_of`] the volume.
    pub(super) roofs: Vec<u64>,
}

impl Reach {
    /// Mark the tiles whose voxels a blocker change in `span` (the
    /// volume's voxels) may light differently: with the sky's light, the
    /// span's columns down to the bottom (whether a voxel there is open to
    /// the sky may change), and in every layer below the span, two voxels
    /// wider each way per layer down (`sky_at` in
    /// `shaders/light_volume.wgsl` takes the five by five above), the
    /// tiles with a blocked voxel above that layer: only a voxel with one
    /// above it takes its neighbors' light, the rest are open to all of
    /// the sky whatever changed; with the sun, every voxel whose way to
    /// the sun crosses the span (`sunlit`); and the voxels each light
    /// with `Shadows` whose rays may cross it lights (`unblocked`,
    /// [`Shadowed::mark_behind`]).
    pub(super) fn mark(&self, tiles: &mut Tiles, (lo, hi): Span) {
        if self.sky {
            tiles.mark((lo.with_y(0), hi));
            for y in 0..hi.y {
                let wider = IVec3::new(2, 0, 2) * (hi.y - y);
                // Without the blockers' layers, every tile may be.
                let roofed = |t: usize| self.roofs.get(t).is_none_or(|r| r >> (y + 1) != 0);
                tiles.mark_where((lo.with_y(y) - wider, hi.with_y(y) + wider), roofed);
            }
        }
        if let Some(d) = self.sun {
            // Every voxel whose cone reaches the span (`sunlit`): a cone
            // runs a layer at a time, `along` across x and z per layer,
            // reading the cover `footprint_reach` around it; so each layer
            // past the span takes the span swept back along the cones that
            // cross it, widened by the furthest their footprints read.
            let up = d.y.abs().max(1e-3);
            let along = Vec2::new(d.x, d.z) / up;
            let least = 0.5 * along.length();
            for y in 0..VOLUME[1] as i32 {
                // The layers on from `y` its cones cross the span at.
                let (k0, k1) = if d.y >= 0.0 {
                    ((lo.y - y).max(1), hi.y - y)
                } else {
                    ((y - hi.y).max(1), y - lo.y)
                };
                if k1 < k0 {
                    continue;
                }
                let reach = footprint_reach(cone_radius(k1 as f32 / up, least));
                let (a, b) = (along * k0 as f32, along * k1 as f32);
                let (near, far) = (a.min(b), a.max(b));
                let from = Vec2::new(lo.x as f32, lo.z as f32) - far - reach;
                let to = Vec2::new((hi.x + 1) as f32, (hi.z + 1) as f32) - near + reach;
                tiles.mark((
                    IVec3::new(from.x.floor() as i32, y, from.y.floor() as i32),
                    IVec3::new(to.x.ceil() as i32, y, to.y.ceil() as i32),
                ));
            }
        }
        for light in &self.shadowed {
            let (a, b) = light.near;
            if a.cmple(hi).all() && b.cmpge(lo).all() {
                light.mark_behind(tiles, (lo, hi));
            }
        }
    }
}

/// A light with `Shadows`, in the volume's voxels.
pub(super) struct Shadowed {
    /// Where it is, in voxels (voxel k's center at k + 0.5).
    pub(super) at: Vec3,
    /// The voxels near enough to block it.
    pub(super) near: Span,
    /// The voxels it lights.
    pub(super) lit: Span,
}

impl Shadowed {
    /// How far aside of the line to the light `unblocked` in
    /// `shaders/light_volume.wgsl` casts its rays, in voxels (up to one
    /// voxel along two axes across it: sqrt 2), rounded up.
    pub(super) const SPREAD: f32 = 2.0;

    /// Mark the voxels it lights whose rays may cross `span`: those behind
    /// the span (padded by [`Shadowed::SPREAD`]) seen from the light, layer
    /// by layer, as the sun's shadow is swept in [`Reach::mark`]; all it
    /// lights when the span reaches the light's height, where its rays run
    /// level and the shadow has no bound across.
    pub(super) fn mark_behind(&self, tiles: &mut Tiles, (lo, hi): Span) {
        let (lit_lo, lit_hi) = self.lit;
        let from = lo.as_vec3() - Self::SPREAD;
        let to = (hi + 1).as_vec3() + Self::SPREAD;
        let l = self.at;
        if from.y <= l.y && l.y <= to.y {
            tiles.mark(self.lit);
            return;
        }
        for y in lit_lo.y..=lit_hi.y {
            let c = y as f32 + 0.5;
            // A voxel lies behind the points of the span between it and
            // the light: the span's slice from this layer's height away
            // from the light, whose corners bound its shadow here.
            let (y0, y1) = if from.y > l.y {
                (from.y, to.y.min(c))
            } else {
                (from.y.max(c), to.y)
            };
            if y0 > y1 {
                continue;
            }
            let mut min = Vec2::INFINITY;
            let mut max = Vec2::NEG_INFINITY;
            for p in [y0, y1].into_iter().flat_map(|y| {
                [
                    (from.x, from.z),
                    (from.x, to.z),
                    (to.x, from.z),
                    (to.x, to.z),
                ]
                .map(|(x, z)| Vec3::new(x, y, z))
            }) {
                let q = l + (p - l) * ((c - l.y) / (p.y - l.y));
                min = min.min(Vec2::new(q.x, q.z));
                max = max.max(Vec2::new(q.x, q.z));
            }
            // The voxels whose centers lie in it.
            let a = (min - 0.5)
                .ceil()
                .as_ivec2()
                .max(IVec2::new(lit_lo.x, lit_lo.z));
            let b = (max - 0.5)
                .floor()
                .as_ivec2()
                .min(IVec2::new(lit_hi.x, lit_hi.z));
            if a.cmple(b).all() {
                tiles.mark((IVec3::new(a.x, y, a.y), IVec3::new(b.x, y, b.y)));
            }
        }
    }
}

/// The voxels a volume moving from voxel `old` to `new` (its lowest)
/// came onto and those it left, in the moved volume's own voxels.
pub(super) fn came_and_left(old: IVec3, new: IVec3, size: IVec3) -> (Vec<Span>, Vec<Span>) {
    let (before, after) = ((old, old + size - 1), (new, new + size - 1));
    let local = |(lo, hi): Span| (lo - new, hi - new);
    let came = blockers::minus(after, Some(before)).into_iter().map(local);
    let left = blockers::minus(before, Some(after)).into_iter().map(local);
    (came.collect(), left.collect())
}

/// Each tile's roofed layers (bit `y`: some voxel of the tile's layer `y`
/// is blocked or holds terrain), in the volume starting at voxel `low`,
/// from the blockers' and the terrain's layers by column
/// ([`blockers::Marked::layers`] and `soil_layers`, kept at slots anchored
/// to the world).
pub(super) fn roofs_of(layers: &[u64], soil_layers: &[u64], low: IVec3) -> Vec<u64> {
    let [nx, ny, nz] = VOLUME.map(|n| n as i32);
    // A column's layers rotate into the volume's own like its voxels.
    const _: () = assert!(VOLUME[1] == u64::BITS);
    let ring = low.rem_euclid(IVec3::new(nx, ny, nz));
    let tile = TILE as i32;
    let mut roofs = vec![0u64; (ACROSS[0] * ACROSS[1]) as usize];
    let dims = IVec3::new(nx, ny, nz);
    for z in 0..nz {
        let rz = (z + ring.z) % nz;
        let tiles = ACROSS[0] as i32 * (z / tile);
        for x in 0..nx {
            let t = (tiles + x / tile) as usize;
            let column = blockers::column_of(dims, (x + ring.x) % nx, rz);
            roofs[t] |= layers[column] | soil_layers[column];
        }
    }
    for roof in &mut roofs {
        *roof = roof.rotate_right(ring.y as u32);
    }
    roofs
}

/// The voxels a light lights (the world's).
pub(super) fn lit_voxels(light: &GpuLight) -> Span {
    let (near, far) = light.lit_box();
    voxels_of(near, far)
}

/// The voxels whose centers may lie from `min` to `max` (the world's).
pub(super) fn voxels_of(min: Vec3, max: Vec3) -> Span {
    (
        (min / VOXEL).floor().as_ivec3(),
        (max / VOXEL).floor().as_ivec3(),
    )
}

impl GpuLights {
    /// The tiles the last frame's fill filled, over every layer.
    #[cfg(test)]
    pub(crate) fn filled_tiles(&self) -> u32 {
        self.filled
    }

    /// Fill the voxels again where they changed: whole if the lights were
    /// hashed again, the volume's center drifted [`RECENTER`] from where
    /// the camera puts it, the sun the voxels shadow (`sun`, towards it)
    /// came, went or turned past [`SUN_TURN`], or the sky's light (`sky`)
    /// came or went; otherwise only the tiles that lights that changed
    /// reach, and, when the place lights with the sky, the sun shadows
    /// here or a light has `Shadows`, those the parts blocking light that
    /// changed reach ([`GpuLights::reached`]).
    pub(crate) fn fill(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        camera: &Camera,
        sky: bool,
        sun: Option<Vec3>,
    ) {
        let size = IVec3::new(VOLUME[0] as i32, VOLUME[1] as i32, VOLUME[2] as i32);
        let dims = size.as_vec3();
        let half = dims * (VOXEL * 0.5);
        let forward = camera.forward();
        let center = camera.focus() + Vec3::new(forward.x, 0.0, forward.z) * AHEAD;
        let centered = self
            .origin
            .is_some_and(|o| (o + half - center).abs().max_element() <= RECENTER);
        let solid = sky || sun.is_some() || self.shadows;
        let blocked = solid && (self.blockers.due() || !self.marked);
        let sun_kept = match (sun, self.sun) {
            (Some(a), Some(b)) => a.dot(b) >= SUN_TURN,
            (a, b) => a.is_none() && b.is_none(),
        };
        self.filled = 0;
        if self.dirty.is_empty() && centered && !blocked && sky == self.sky && sun_kept {
            return;
        }
        profiling::scope!("GpuLights::fill");
        // Voxel k is centered at 4k + 2.
        let kept = self.origin.map(|o| (o / VOXEL).floor().as_ivec3());
        let mut low = match kept {
            Some(low) if centered => low,
            _ => snapped((center - half) / VOXEL),
        };
        // The terrain goes up only while the sun or the sky reads it, so a
        // reader that just came takes it whole; the sun cover is built
        // only while the sun reads it, so whole when it comes.
        let sun_came = sun.is_some() && self.sun.is_none();
        let soil_read = sky || sun.is_some();
        let soil_came = soil_read && !self.sky && self.sun.is_none();
        let mut touched = Touched::default();
        let mut roofs = Vec::new();
        if solid && (blocked || !centered || soil_came) {
            match self.blockers.mark(low, size) {
                Some(marked) => {
                    if sky {
                        roofs = roofs_of(marked.layers, marked.soil_layers, low);
                    }
                    let bytes = |buffer: &wgpu::Buffer, of: &[u32], run: &Range<usize>| {
                        let words = run.start / 4..run.end.div_ceil(4);
                        let at = (words.start * 4) as u64;
                        queue.write_buffer(buffer, at, bytemuck::cast_slice(&of[words]));
                    };
                    let every = 0..size.element_product() as usize;
                    let runs = marked.slot_runs();
                    for run in &runs {
                        let words = run.start / 32..run.end.div_ceil(32);
                        let at = (words.start * 4) as u64;
                        let bits = &marked.bits[words];
                        queue.write_buffer(&self.sky_buffers.solid, at, bytemuck::cast_slice(bits));
                        if soil_read && !soil_came {
                            bytes(&self.sky_buffers.soils, marked.soils, run);
                        }
                    }
                    if soil_came {
                        bytes(&self.sky_buffers.soils, marked.soils, &every);
                    }
                    touched = marked.touched;
                }
                // The voxels it moved onto aren't marked yet: it stays
                // where it was, the blockers there still on the GPU, and
                // is filled there only if something else changed.
                None => match kept {
                    Some(kept) if !self.dirty.is_empty() || sky != self.sky || !sun_kept => {
                        low = kept;
                    }
                    _ => return,
                },
            }
        }
        if sun.is_some() {
            let mut changed = Changed::none();
            match &touched {
                _ if sun_came => changed = Changed::all(),
                Touched::All => changed = Changed::all(),
                Touched::Spans(spans) => spans.iter().for_each(|&span| changed.mark(span)),
            }
            self.cover.build(queue, encoder, &changed);
        }
        let origin = low.as_vec3() * VOXEL + VOXEL * 0.5;
        let ring = low.rem_euclid(size);
        let whole = self.dirty.all
            || kept.is_none()
            || sky != self.sky
            || !sun_kept
            || solid != self.marked
            || matches!(touched, Touched::All);
        let dirty = std::mem::take(&mut self.dirty);
        self.marked = solid;
        self.origin = Some(origin);
        self.sky = sky;
        self.sun = sun;
        let lit = !self.lights.is_empty();
        queue.write_buffer(
            &self.volume,
            0,
            bytemuck::bytes_of(&VolumeUniform {
                origin: [origin.x, origin.y, origin.z, f32::from(u8::from(lit))],
                size: [dims.x, dims.y, dims.z, f32::from(u8::from(sky))],
                sun: [f32::from(u8::from(sun.is_some())), 0.0, 0.0, 0.0],
                ring: ring.as_vec3().extend(0.0).to_array(),
            }),
        );
        if !lit && !sky && sun.is_none() {
            return;
        }
        let tiles = if whole {
            Tiles::all()
        } else {
            let mut tiles = Tiles::none();
            for span in dirty.lights {
                tiles.mark((span.0 - low, span.1 - low));
            }
            let reach = Reach {
                sky,
                sun,
                shadowed: self.shadowed_reach(low),
                roofs,
            };
            if let Touched::Spans(spans) = touched {
                for span in by_column(&spans, low) {
                    reach.mark(&mut tiles, span);
                }
            }
            // A volume that moved fills what it came onto, and, as for
            // blockers that changed, what that and what it left reach:
            // those it kept saw open sky and nothing in the way past its
            // old faces.
            if let Some(old) = kept.filter(|&old| old != low) {
                let (came, left) = came_and_left(old, low, size);
                let before = if rook_types::counters::ENABLED {
                    tiles.count()
                } else {
                    0
                };
                for &span in &came {
                    tiles.mark(span);
                }
                // What the volume moved onto is new to it, as a change is.
                rook_types::count!(LightSourcesChanged, tiles.count() - before);
                if solid {
                    for span in came.into_iter().chain(left) {
                        reach.mark(&mut tiles, span);
                    }
                }
            }
            tiles
        };
        let mut layers = vec![LayerUniform::zeroed(); VOLUME[1] as usize];
        let mut dispatches = Vec::new();
        for y in (0..VOLUME[1]).rev() {
            if let Some((bits, lo, hi)) = tiles.layer(y) {
                layers[y as usize] = LayerUniform {
                    at: [y, lo[0], lo[1], 0],
                    tiles: *bits,
                };
                let across = |axis: usize| (hi[axis] - lo[axis] + 1) * TILE / 8;
                dispatches.push((y, across(0), across(1)));
            }
        }
        self.filled = tiles.count();
        rook_types::count!(LightTilesRefilled, self.filled);
        rook_types::gauge!(LightTiles, Tiles::all().count());
        if dispatches.is_empty() {
            return;
        }
        let mut bytes = vec![0u8; (LAYER_STRIDE * u64::from(VOLUME[1])) as usize];
        for (y, layer) in layers.iter().enumerate() {
            let at = y * LAYER_STRIDE as usize;
            bytes[at..at + LAYER_SIZE as usize].copy_from_slice(bytemuck::bytes_of(layer));
        }
        queue.write_buffer(&self.sky_buffers.layers, 0, &bytes);
        queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&FillParams {
                origin: [origin.x, origin.y, origin.z, self.hash.cell],
                size: [
                    self.hash.buckets.len() as u32 - 1,
                    VOLUME[0],
                    VOLUME[1],
                    VOLUME[2],
                ],
                ring: ring.as_uvec3().extend(0).to_array(),
                flags: [
                    u32::from(lit),
                    u32::from(sky),
                    u32::from(solid),
                    u32::from(sun.is_some()),
                ],
                sun: sun.unwrap_or(Vec3::Y).extend(0.0).to_array(),
                cone: [CONE_START, CONE_WIDENS, (LEVELS - 1) as f32, 0.0],
            }),
        );
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("rook light volume fill"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.fill);
        // One layer at a time from the top: each takes the sky's light
        // from the one above.
        for (y, x, z) in dispatches {
            let offset = (LAYER_STRIDE * u64::from(y)) as u32;
            pass.set_bind_group(0, &self.fill_group, &[offset]);
            pass.dispatch_workgroups(x, z, 1);
        }
    }

    /// For each light with `Shadows`, the voxels a part must reach to
    /// cast its shadows (a ray runs from a voxel the light lights to the
    /// light, its eight neighbours up to a voxel and a half aside), and
    /// those it lights, in the volume starting at voxel `low`.
    pub(super) fn shadowed_reach(&self, low: IVec3) -> Vec<Shadowed> {
        if !self.shadows {
            return Vec::new();
        }
        let margin = Vec3::splat(2.0 * VOXEL);
        let local = |(a, b): Span| (a - low, b - low);
        self.lights
            .iter()
            .filter(|l| l.shadows())
            .map(|light| {
                let (near, far) = light.lit_box();
                Shadowed {
                    at: light.at() / VOXEL - low.as_vec3(),
                    near: local(voxels_of(near - margin, far + margin)),
                    lit: local(lit_voxels(light)),
                }
            })
            .collect()
    }
}
