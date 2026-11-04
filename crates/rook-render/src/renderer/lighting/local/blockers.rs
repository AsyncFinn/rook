//! What blocks light in the voxels: the scene's opaque parts as boxes,
//! marked into the light volume's voxels so the fill pass can carry the
//! sky's diffuse light (`EnvironmentDiffuseScale`) down around them and
//! keep lights with `Shadows` out from behind them, and, by how much of
//! each voxel they fill, dim the sun
//! (`shaders/light_volume.wgsl`; docs/behavior/rendering/materials.md, "The sky's
//! diffuse light"; docs/behavior/lighting/local-lights.md, "Shadows";
//! docs/behavior/lighting/sun.md).

use std::ops::Range;
use std::time::{Duration, Instant};

use glam::{IVec2, IVec3, Mat3};

use super::*;

/// How long a frame may spend marking the voxels a moving volume came
/// onto; the rest wait for later frames, the volume staying where it was
/// meanwhile (Studio's grid also moves in chunks, keeping the old one for
/// a while: docs/behavior/lighting/local-lights.md).
const BUDGET: Duration = Duration::from_millis(1);
/// The pieces those voxels are marked in, across x and z (the volume's
/// full height each), in voxels.
const PIECE: i32 = 16;

/// A part as the voxels see it: a box.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Blocker {
    center: Vec3,
    /// The box's axes (unit columns).
    axes: Mat3,
    /// Half the box along each axis, at least half a voxel, so a thin roof
    /// or wall still fills a layer of voxels (a 1-stud roof blocks fully
    /// in Studio).
    half: Vec3,
    min: Vec3,
    max: Vec3,
    /// Half the box as it is, for how much of each voxel it fills.
    true_half: Vec3,
    /// The box as it is, axis-aligned (for a rotated part, around it).
    true_min: Vec3,
    true_max: Vec3,
}

impl Blocker {
    /// A box centered at `center` along `axes`, `half` its size each way.
    fn of_box(center: Vec3, axes: Mat3, true_half: Vec3) -> Self {
        let half = true_half.max(Vec3::splat(VOXEL * 0.5));
        let extent = rook_types::box_reach(axes, half);
        let true_extent = rook_types::box_reach(axes, true_half);
        Self {
            center,
            axes,
            half,
            min: center - extent,
            max: center + extent,
            true_half,
            true_min: center - true_extent,
            true_max: center + true_extent,
        }
    }

    /// The voxels whose centers it may hold.
    fn centers(&self) -> Span {
        let lo = ((self.min - VOXEL * 0.5) / VOXEL).ceil().as_ivec3();
        let hi = ((self.max - VOXEL * 0.5) / VOXEL).floor().as_ivec3();
        (lo, hi)
    }

    /// The voxels it may fill some of.
    fn touched(&self) -> Span {
        let lo = (self.true_min / VOXEL).floor().as_ivec3();
        let hi = (self.true_max / VOXEL).ceil().as_ivec3() - 1;
        (lo, hi)
    }

    /// Every voxel it reaches either way.
    fn voxels(&self) -> Span {
        let ((a, b), (c, d)) = (self.centers(), self.touched());
        (a.min(c), b.max(d))
    }

    /// Whether its axes are the world's, in some order.
    fn aligned(&self) -> bool {
        [self.axes.x_axis, self.axes.y_axis, self.axes.z_axis]
            .iter()
            .all(|a| a.abs().max_element() > 0.9999)
    }

    /// The voxels it fills whole, if it is aligned with the world (their
    /// centers are inside it too); `None` if none are.
    fn whole(&self) -> Option<Span> {
        if !self.aligned() {
            return None;
        }
        let lo = (self.true_min / VOXEL).ceil().as_ivec3();
        let hi = (self.true_max / VOXEL).floor().as_ivec3() - 1;
        (!lo.cmpgt(hi).any()).then_some((lo, hi))
    }
}

/// How full a voxel is, in 255ths.
const FULL: u16 = 255;

#[derive(Debug, Default)]
pub(super) struct Blockers {
    /// By index into `Scene::parts`.
    blockers: Vec<Option<Blocker>>,
    /// Where they stand.
    columns: Columns,
    /// Each mesh file's box in its own space (center, half size); `None`
    /// for terrain, which doesn't block yet.
    bounds: FxHashMap<MeshHash, Option<(Vec3, Vec3)>>,
    /// A blocker changed since the voxels were marked.
    changed: bool,
    /// The voxels marked again since the last [`Blockers::mark`] that
    /// finished.
    touched: Touched,
    /// The voxels as last marked, kept so parts that change only unmark
    /// their old box and mark their new one.
    grid: Option<Grid>,
    /// Blockers changed since the grid was marked: (before, after).
    moved: Vec<(Option<Blocker>, Option<Blocker>)>,
    /// Parts changed while nothing used the blockers, taken when
    /// something does; `stale` marks those listed.
    stale: Vec<usize>,
    is_stale: Vec<bool>,
}

/// The marked voxels of a volume, anchored to the world: voxel `k`
/// (centered at `4k + 2` on each axis) lives at slot `k mod dims`, so a
/// volume that moves keeps the voxels it still covers and marks only those
/// that came in, a few pieces a frame.
#[derive(Debug)]
struct Grid {
    /// The volume's lowest voxel.
    low: IVec3,
    dims: IVec3,
    /// Blockers covering each voxel.
    counts: Vec<u32>,
    /// A bit per voxel covered at all, as the fill pass reads it.
    bits: Vec<u32>,
    /// The same bits by column (`x + nx * z` of the slots), a bit per
    /// layer slot: which layers of each column are blocked, so the fill
    /// can tell where the sky's light spreads sideways.
    layers: Vec<u64>,
    /// How much of each voxel the blockers fill, summed, in 255ths.
    fills: Vec<u16>,
    /// The same, capped at full, a byte per voxel as the fill pass reads
    /// it.
    fill_bytes: Vec<u32>,
    /// Pieces of the volume still to mark since it moved.
    pending: Vec<Span>,
}

/// Voxels from the first corner to the second, both included.
pub(super) type Span = (IVec3, IVec3);

/// Which voxels a mark changed.
#[derive(Debug)]
pub(super) enum Touched {
    /// Those in these boxes, in the world's voxels (within the volume).
    Spans(Vec<Span>),
    /// All of the volume's.
    All,
}

impl Default for Touched {
    fn default() -> Self {
        Self::Spans(Vec::new())
    }
}

impl Touched {
    fn add(&mut self, span: Span) {
        if let Self::Spans(spans) = self {
            spans.push(span);
        }
    }
}

/// The marked voxels, as the fill pass reads them.
#[derive(Debug)]
pub(super) struct Marked<'a> {
    /// A bit per voxel: a blocker holds its center.
    pub(super) bits: &'a [u32],
    /// A byte per voxel, four to a word: how much of it blockers fill.
    pub(super) fills: &'a [u32],
    /// [`Grid`]'s blocked layers by column.
    pub(super) layers: &'a [u64],
    pub(super) touched: Touched,
    dims: IVec3,
}

impl Marked<'_> {
    /// The voxel slots (`Grid`'s) holding what changed, in runs of whole
    /// rows along x, sorted and joined: what goes up to the GPU.
    pub(super) fn slot_runs(&self) -> Vec<Range<usize>> {
        let d = self.dims;
        let Touched::Spans(spans) = &self.touched else {
            let all = 0..(d.x * d.y * d.z) as usize;
            return vec![all];
        };
        let mut runs = Vec::new();
        for &(lo, hi) in spans {
            // The rows, as at most two runs where z wraps around the slots.
            let rows = if hi.z - lo.z + 1 >= d.z {
                vec![(0, d.z - 1)]
            } else {
                let (z0, z1) = (lo.z.rem_euclid(d.z), hi.z.rem_euclid(d.z));
                if z0 <= z1 {
                    vec![(z0, z1)]
                } else {
                    vec![(z0, d.z - 1), (0, z1)]
                }
            };
            for y in lo.y..=hi.y {
                let layer = d.z * y.rem_euclid(d.y);
                for &(z0, z1) in &rows {
                    runs.push((d.x * (layer + z0)) as usize..(d.x * (layer + z1 + 1)) as usize);
                }
            }
        }
        runs.sort_by_key(|r| r.start);
        let mut joined: Vec<Range<usize>> = Vec::with_capacity(runs.len());
        for run in runs {
            match joined.last_mut() {
                Some(last) if run.start <= last.end => last.end = last.end.max(run.end),
                _ => joined.push(run),
            }
        }
        joined
    }
}

/// The voxels of `span` from `lo` to `hi`, or `None` if they miss it.
fn clip((lo, hi): Span, span: Span) -> Option<Span> {
    let (lo, hi) = (lo.max(span.0), hi.min(span.1));
    (!lo.cmpgt(hi).any()).then_some((lo, hi))
}

/// The voxels of `span` outside `cut`, as up to six boxes: those past it
/// along x, then along y among the rest, then along z.
pub(super) fn minus(span: Span, cut: Option<Span>) -> Vec<Span> {
    let Some(cut) = cut.and_then(|cut| clip(cut, span)) else {
        return vec![span];
    };
    let mut pieces = Vec::new();
    let mut rest = span;
    for axis in 0..3 {
        let (lo, hi) = (rest.0[axis], rest.1[axis]);
        for (from, to) in [(lo, cut.0[axis] - 1), (cut.1[axis] + 1, hi)] {
            if from <= to {
                let mut piece = rest;
                piece.0[axis] = from;
                piece.1[axis] = to;
                pieces.push(piece);
            }
        }
        rest.0[axis] = cut.0[axis];
        rest.1[axis] = cut.1[axis];
    }
    pieces
}

/// Where a grid keeps a voxel: its slot (`Grid`'s: the world's voxel
/// index modulo `dims`, x + nx * (z + nz * y)), and its column's slot
/// (x + nx * z) and layer's, so the per-column layers need no division.
#[derive(Debug, Clone, Copy)]
struct Slot {
    index: usize,
    column: usize,
    layer: u32,
}

/// Each voxel of `span` with where a grid of `dims` keeps it, walked a row
/// at a time so the modulo is taken per row, not per voxel.
fn each(dims: IVec3, (lo, hi): Span, mut f: impl FnMut(IVec3, Slot)) {
    let area = dims.x * dims.z;
    for y in lo.y..=hi.y {
        let ry = y.rem_euclid(dims.y);
        for z in lo.z..=hi.z {
            let column = dims.x * z.rem_euclid(dims.z);
            let row = column + area * ry;
            let mut rx = lo.x.rem_euclid(dims.x);
            for x in lo.x..=hi.x {
                let slot = Slot {
                    index: (row + rx) as usize,
                    column: (column + rx) as usize,
                    layer: ry as u32,
                };
                f(IVec3::new(x, y, z), slot);
                rx += 1;
                if rx == dims.x {
                    rx = 0;
                }
            }
        }
    }
}

/// `span` cut into pieces [`PIECE`] voxels across, on a world-aligned grid.
fn pieces((lo, hi): Span) -> impl Iterator<Item = Span> {
    let starts = move |lo: i32, hi: i32| {
        let first = lo.div_euclid(PIECE) * PIECE;
        (first..=hi)
            .step_by(PIECE as usize)
            .map(move |s| (s.max(lo), (s + PIECE - 1).min(hi)))
    };
    starts(lo.z, hi.z).flat_map(move |(z0, z1)| {
        starts(lo.x, hi.x).map(move |(x0, x1)| (IVec3::new(x0, lo.y, z0), IVec3::new(x1, hi.y, z1)))
    })
}

/// Voxel `k`'s center.
fn center(k: IVec3) -> Vec3 {
    k.as_vec3() * VOXEL + VOXEL * 0.5
}

impl Grid {
    fn new(low: IVec3, dims: IVec3, blockers: &[Option<Blocker>], columns: &Columns) -> Self {
        let voxels = (dims.x * dims.y * dims.z) as usize;
        assert!(dims.y <= 64, "a column's layers take a bit each of a u64");
        let mut grid = Self {
            low,
            dims,
            counts: vec![0; voxels],
            bits: vec![0; voxels.div_ceil(32)],
            layers: vec![0; (dims.x * dims.z) as usize],
            fills: vec![0; voxels],
            fill_bytes: vec![0; voxels.div_ceil(4)],
            pending: Vec::new(),
        };
        // Already empty: only the blockers to take.
        grid.take(grid.span(), blockers, columns);
        grid
    }

    fn span(&self) -> Span {
        (self.low, self.low + self.dims - 1)
    }

    /// Move the volume to start at `low`, leaving the voxels that came in
    /// to [`Grid::work`]. Returns their pieces.
    fn move_to(&mut self, low: IVec3) -> Vec<Span> {
        let old = self.span();
        self.low = low;
        let new = self.span();
        // Pieces still to mark from an earlier move keep the part of them
        // still inside; what came in now is outside the old volume, so
        // apart from them.
        self.pending.retain_mut(|span| match clip(*span, new) {
            Some(within) => {
                *span = within;
                true
            }
            None => false,
        });
        // The voxels that came in, as up to three boxes: those past the
        // old volume along x, then along y among the rest, then along z.
        let came: Vec<Span> = minus(new, Some(old)).into_iter().flat_map(pieces).collect();
        self.pending.extend_from_slice(&came);
        came
    }

    /// Mark pending pieces for up to `budget`; whether none are left.
    fn work(&mut self, budget: Duration, blockers: &[Option<Blocker>], columns: &Columns) -> bool {
        let start = Instant::now();
        while start.elapsed() < budget
            && let Some(span) = self.pending.pop()
        {
            self.mark(span, blockers, columns);
        }
        self.pending.is_empty()
    }

    /// Mark `span` afresh with `blockers` (standing in `columns`).
    fn mark(&mut self, span: Span, blockers: &[Option<Blocker>], columns: &Columns) {
        let (counts, fills) = (&mut self.counts, &mut self.fills);
        let (bits, fill_bytes) = (&mut self.bits, &mut self.fill_bytes);
        let layers = &mut self.layers;
        each(self.dims, span, |_, slot| {
            let i = slot.index;
            counts[i] = 0;
            fills[i] = 0;
            bits[i / 32] &= !(1 << (i % 32));
            layers[slot.column] &= !(1 << slot.layer);
            fill_bytes[i / 4] &= !(0xff << ((i % 4) * 8));
        });
        self.take(span, blockers, columns);
    }

    /// Add `blockers` (standing in `columns`) to the voxels of `span`.
    fn take(&mut self, span: Span, blockers: &[Option<Blocker>], columns: &Columns) {
        for i in columns.near(span, blockers) {
            if let Some(b) = &blockers[i] {
                self.apply(b, true, span);
                self.apply_fill(b, true, span);
            }
        }
    }

    /// Add `b`'s share of each voxel of `span` it overlaps (`add`), or take
    /// it away.
    fn apply_fill(&mut self, b: &Blocker, add: bool, span: Span) {
        let Some(within) = clip(b.touched(), span) else {
            return;
        };
        let to_local = b.axes.transpose();
        let aligned = b.aligned();
        // A rotated box is sampled at 64 points a stud apart across each
        // voxel: the points' offsets from the voxel's center, in the box's
        // space, and how far the furthest one is.
        let points: [Vec3; 64] = std::array::from_fn(|n| {
            let n = n as u32;
            let cell = Vec3::new((n % 4) as f32, ((n / 4) % 4) as f32, (n / 16) as f32);
            to_local * ((cell - 1.5) * (VOXEL / 4.0))
        });
        let reach = 1.5 * (VOXEL / 4.0) * 3f32.sqrt();
        let Self {
            dims,
            fills,
            fill_bytes,
            ..
        } = self;
        each(*dims, within, |k, slot| {
            let i = slot.index;
            let at = center(k);
            let share = if aligned {
                // The overlap of two boxes, axis by axis.
                let over = (b.true_max.min(at + VOXEL * 0.5) - b.true_min.max(at - VOXEL * 0.5))
                    .max(Vec3::ZERO);
                over.x * over.y * over.z / (VOXEL * VOXEL * VOXEL)
            } else {
                // A rotated box: how many of the points are inside it,
                // all or none when the voxel is wholly in or out.
                let local = to_local * (at - b.center);
                if (local.abs() - reach).cmpgt(b.true_half).any() {
                    0.0
                } else if (local.abs() + reach).cmple(b.true_half).all() {
                    1.0
                } else {
                    let inside = points
                        .iter()
                        .filter(|&&o| (local + o).abs().cmple(b.true_half).all())
                        .count();
                    inside as f32 / 64.0
                }
            };
            let share = (share * f32::from(FULL)).round() as u16;
            if share == 0 {
                return;
            }
            fills[i] = if add {
                fills[i].saturating_add(share)
            } else {
                fills[i].saturating_sub(share)
            };
            let shift = (i % 4) * 8;
            let byte = u32::from(fills[i].min(FULL));
            let word = &mut fill_bytes[i / 4];
            *word = (*word & !(0xff << shift)) | (byte << shift);
        });
    }

    /// Count `b` in (`add`) or out of the voxels of `span` whose centers
    /// it holds.
    fn apply(&mut self, b: &Blocker, add: bool, span: Span) {
        let Some(within) = clip(b.centers(), span) else {
            return;
        };
        let to_local = b.axes.transpose();
        let Self {
            dims,
            counts,
            bits,
            layers,
            ..
        } = self;
        each(*dims, within, |k, slot| {
            let local = to_local * (center(k) - b.center);
            if !local.abs().cmple(b.half).all() {
                return;
            }
            let i = slot.index;
            let (word, bit) = (i / 32, 1 << (i % 32));
            let layer = 1 << slot.layer;
            if add {
                counts[i] += 1;
                bits[word] |= bit;
                layers[slot.column] |= layer;
            } else {
                // Saturating: a voxel still to mark holds another place's
                // counts until it is.
                counts[i] = counts[i].saturating_sub(1);
                if counts[i] == 0 {
                    bits[word] &= !bit;
                    layers[slot.column] &= !layer;
                }
            }
        });
    }
}

/// A column's width, in voxels.
const COLUMN: i32 = 16;
/// Blockers across more columns than this along an axis (a baseplate) are
/// kept apart and tried for every span, rather than listed in thousands of
/// columns.
const WIDE: i32 = 16;

/// Which blockers stand in each 64-stud column of the world, so the voxels
/// a moving volume comes onto take only the blockers there, not every part
/// in the place.
#[derive(Debug, Default)]
struct Columns {
    /// Indices into the blockers, by column.
    columns: FxHashMap<IVec2, Vec<u32>>,
    wide: Vec<u32>,
}

/// The columns `span` reaches.
fn columns_of((lo, hi): Span) -> (IVec2, IVec2) {
    let column = |v: IVec3| IVec2::new(v.x, v.z).div_euclid(IVec2::splat(COLUMN));
    (column(lo), column(hi))
}

fn is_wide((lo, hi): (IVec2, IVec2)) -> bool {
    (hi - lo).cmpge(IVec2::splat(WIDE)).any()
}

impl Columns {
    fn clear(&mut self) {
        self.columns.clear();
        self.wide.clear();
    }

    fn insert(&mut self, i: usize, b: &Blocker) {
        let range = columns_of(b.voxels());
        if is_wide(range) {
            self.wide.push(i as u32);
            return;
        }
        for c in cells(range) {
            self.columns.entry(c).or_default().push(i as u32);
        }
    }

    fn remove(&mut self, i: usize, b: &Blocker) {
        let take = |list: &mut Vec<u32>| {
            if let Some(at) = list.iter().position(|&j| j as usize == i) {
                list.swap_remove(at);
            }
        };
        let range = columns_of(b.voxels());
        if is_wide(range) {
            take(&mut self.wide);
            return;
        }
        for c in cells(range) {
            if let Some(list) = self.columns.get_mut(&c) {
                take(list);
                if list.is_empty() {
                    self.columns.remove(&c);
                }
            }
        }
    }

    /// The blockers that may reach `span`, each once.
    fn near(&self, span: Span, blockers: &[Option<Blocker>]) -> Vec<usize> {
        let (q0, q1) = columns_of(span);
        let mut near: Vec<usize> = self.wide.iter().map(|&i| i as usize).collect();
        for c in cells((q0, q1)) {
            for &i in self.columns.get(&c).into_iter().flatten() {
                let Some(b) = &blockers[i as usize] else {
                    continue;
                };
                // A blocker over several columns is taken in the first of
                // them the span reaches.
                if columns_of(b.voxels()).0.max(q0) == c {
                    near.push(i as usize);
                }
            }
        }
        near
    }
}

/// Each column from the first corner to the second.
fn cells((lo, hi): (IVec2, IVec2)) -> impl Iterator<Item = IVec2> {
    (lo.y..=hi.y).flat_map(move |z| (lo.x..=hi.x).map(move |x| IVec2::new(x, z)))
}

impl Blockers {
    pub(super) fn clear(&mut self) {
        self.blockers.clear();
        self.columns.clear();
        self.changed = true;
        self.grid = None;
        self.moved.clear();
        self.stale.clear();
        self.is_stale.clear();
    }

    /// Take the parts at `changed` (indices into `scene.parts`) as they
    /// are now, or, while the blockers aren't `used`, note them for later.
    pub(super) fn update(&mut self, scene: &Scene, changed: &[usize], used: bool) {
        if self.blockers.len() < scene.parts.len() {
            self.blockers.resize(scene.parts.len(), None);
            self.is_stale.resize(scene.parts.len(), false);
        }
        if !used {
            for &i in changed {
                if !std::mem::replace(&mut self.is_stale[i], true) {
                    self.stale.push(i);
                }
            }
            return;
        }
        let stale = std::mem::take(&mut self.stale);
        for &i in stale.iter().chain(changed) {
            self.is_stale[i] = false;
            let blocker = self.blocker(&scene.parts[i], scene);
            if self.blockers[i] != blocker {
                if self.grid.is_some() {
                    self.moved.push((self.blockers[i], blocker));
                }
                if let Some(b) = &self.blockers[i] {
                    self.columns.remove(i, b);
                }
                if let Some(b) = &blocker {
                    self.columns.insert(i, b);
                }
                self.blockers[i] = blocker;
                self.changed = true;
                rook_types::count!(LightSourcesChanged, 1);
            }
        }
        // More changes than parts (or nothing marking the voxels for a
        // while): marking afresh is cheaper.
        if self.moved.len() > self.blockers.len()
            && let Some(grid) = &mut self.grid
        {
            grid.pending = pieces(grid.span()).collect();
            self.moved.clear();
            self.touched = Touched::All;
        }
    }

    /// Whether the voxels should be marked again for parts that changed.
    pub(super) fn due(&self) -> bool {
        self.changed
    }

    /// Which parts block: opaque ones that cast shadows (measured: a roof
    /// with `CastShadow` off blocks nothing, nor does such a post block
    /// lights with `Shadows`; a roof at Transparency 0.5 blocks only a
    /// tenth or so), as boxes; wedges, balls, cylinders and unions block
    /// their whole box (Approximation).
    fn blocker(&mut self, part: &PartInstance, scene: &Scene) -> Option<Blocker> {
        if !part.cast_shadow || part.over || part.transparency >= 0.5 {
            return None;
        }
        if !matches!(part.layer, Layer::Base) {
            return None;
        }
        let (center, half) = match part.shape {
            DrawShape::Part(_) => (Vec3::ZERO, part.size * 0.5),
            DrawShape::File(hash) => {
                let (center, half) = match self.bounds.get(&hash) {
                    Some(bounds) => (*bounds)?,
                    // Not loaded yet: the part comes back when it is.
                    None => {
                        let bounds = mesh_bounds(scene.meshes.get(&hash)?);
                        self.bounds.insert(hash, bounds);
                        bounds?
                    }
                };
                (center * part.size, half * part.size.abs())
            }
            DrawShape::Head | DrawShape::Truss(_) | DrawShape::Adorn(_) => return None,
        };
        let axes = part.rotation;
        Some(Blocker::of_box(part.position + axes * center, axes, half))
    }

    /// One bit per voxel of the volume starting at voxel `low`, set where
    /// a voxel's center is inside a blocker, and a byte per voxel, how much
    /// of it the blockers fill (255 full), both kept at [`Grid`]'s slots,
    /// with the voxels marked again since the last mark; `None` while the voxels a moved volume came onto (or all of them,
    /// after more changes than parts) are still being marked, within
    /// [`BUDGET`] a frame, the last volume's staying as they were. A volume
    /// of the same size only takes the blockers that changed since and the
    /// voxels it moved onto; the first is marked whole at once.
    pub(super) fn mark(&mut self, low: IVec3, dims: IVec3) -> Option<Marked<'_>> {
        profiling::scope!("Blockers::mark");
        match &mut self.grid {
            Some(grid) if grid.dims == dims => {
                // The voxels it came onto change; those it still covers
                // keep their slots, so neither they nor what the fill made
                // of them go up again.
                if grid.low != low {
                    for piece in grid.move_to(low) {
                        self.touched.add(piece);
                    }
                }
                // Taken everywhere at once, pieces still to mark included:
                // those are cleared and take the blockers as they are then.
                let span = grid.span();
                for (before, after) in std::mem::take(&mut self.moved) {
                    // Voxels both boxes fill whole are the same after: a
                    // big part that moves a little marks only its edges.
                    let kept = before
                        .zip(after)
                        .and_then(|(a, b)| clip(a.whole()?, b.whole()?));
                    for (b, add) in [(before, false), (after, true)] {
                        let Some(b) = b else {
                            continue;
                        };
                        let Some(within) = clip(b.voxels(), span) else {
                            continue;
                        };
                        for piece in minus(within, kept) {
                            grid.apply(&b, add, piece);
                            grid.apply_fill(&b, add, piece);
                            self.touched.add(piece);
                        }
                    }
                }
                if !grid.work(BUDGET, &self.blockers, &self.columns) {
                    return None;
                }
            }
            grid => {
                self.moved.clear();
                *grid = Some(Grid::new(low, dims, &self.blockers, &self.columns));
                self.touched = Touched::All;
            }
        }
        self.changed = false;
        let grid = self.grid.as_ref().expect("just marked");
        Some(Marked {
            bits: &grid.bits,
            fills: &grid.fill_bytes,
            layers: &grid.layers,
            touched: std::mem::take(&mut self.touched),
            dims,
        })
    }
}

/// A mesh's box in its own space (center, half size); `None` for terrain.
fn mesh_bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
    if mesh.terrain.is_some() {
        return None;
    }
    let (min, max) = mesh.positions.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), &p| (lo.min(Vec3::from(p)), hi.max(Vec3::from(p))),
    );
    if min.cmpgt(max).any() {
        return None;
    }
    Some(((min + max) * 0.5, (max - min) * 0.5))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blockers of every kind the grid marks differently: thin and thick,
    /// axis-aligned and rotated, too wide to list by column, and
    /// across negative coordinates.
    fn scattered() -> Vec<Option<Blocker>> {
        let turned = Mat3::from_rotation_y(0.6) * Mat3::from_rotation_x(0.3);
        let mut list = vec![
            Some(Blocker::of_box(
                Vec3::new(0.0, -0.5, 0.0),
                Mat3::IDENTITY,
                Vec3::new(2048.0, 0.5, 2048.0),
            )),
            None,
        ];
        for i in 0..400 {
            let f = i as f32;
            let at = Vec3::new(
                (f * 37.3) % 1500.0 - 750.0,
                (f * 11.7) % 200.0 - 40.0,
                (f * 53.9) % 1500.0 - 750.0,
            );
            let half = Vec3::new(1.0 + f % 13.0, 0.5 + f % 7.0, 2.0 + f % 29.0);
            let axes = if i % 3 == 0 { turned } else { Mat3::IDENTITY };
            list.push(Some(Blocker::of_box(at, axes, half)));
        }
        list
    }

    fn indexed(list: &[Option<Blocker>]) -> Columns {
        let mut columns = Columns::default();
        for (i, b) in list.iter().enumerate() {
            if let Some(b) = b {
                columns.insert(i, b);
            }
        }
        columns
    }

    #[test]
    fn a_moved_grid_matches_one_marked_where_it_went() {
        let list = scattered();
        let columns = indexed(&list);
        let dims = IVec3::new(64, 32, 64);
        let mut grid = Grid::new(IVec3::new(-40, -10, -30), dims, &list, &columns);
        // Small and large steps, every direction, and one past the whole volume.
        for low in [
            IVec3::new(-24, -10, -30),
            IVec3::new(-24, -4, -47),
            IVec3::new(-60, -12, -20),
            IVec3::new(100, 3, 90),
            IVec3::new(90, 0, 101),
        ] {
            grid.move_to(low);
            assert!(grid.work(Duration::MAX, &list, &columns));
            let fresh = Grid::new(low, dims, &list, &columns);
            assert!(grid.counts == fresh.counts, "counts at {low}");
            assert!(grid.bits == fresh.bits, "bits at {low}");
            assert!(grid.fill_bytes == fresh.fill_bytes, "fills at {low}");
            assert!(grid.layers == fresh.layers, "layers at {low}");
            assert!(
                grid.layers == layers_of(&grid),
                "layers match bits at {low}"
            );
        }
    }

    /// The blocked layers of each column, read off the bits.
    fn layers_of(grid: &Grid) -> Vec<u64> {
        let area = (grid.dims.x * grid.dims.z) as usize;
        let mut layers = vec![0; area];
        for i in 0..grid.counts.len() {
            if grid.bits[i / 32] & (1 << (i % 32)) != 0 {
                layers[i % area] |= 1 << (i / area);
            }
        }
        layers
    }

    #[test]
    fn a_blocker_moving_while_the_grid_moves_lands_once() {
        let mut list = scattered();
        let mut columns = indexed(&list);
        let dims = IVec3::new(64, 32, 64);
        let mut grid = Grid::new(IVec3::new(-40, -10, -30), dims, &list, &columns);
        let low = IVec3::new(-20, -10, -30);
        grid.move_to(low);
        // Half the pieces marked before a blocker moves into what came in.
        let half = grid.pending.len() / 2;
        for _ in 0..half {
            let span = grid.pending.pop().expect("pending");
            grid.mark(span, &list, &columns);
        }
        let before = list[5];
        let after = Some(Blocker::of_box(
            Vec3::new(130.0, 10.0, 40.0),
            Mat3::from_rotation_y(0.4),
            Vec3::new(30.0, 6.0, 9.0),
        ));
        columns.remove(5, before.as_ref().expect("a blocker"));
        columns.insert(5, after.as_ref().expect("a blocker"));
        list[5] = after;
        let span = grid.span();
        for (b, add) in [(before, false), (after, true)] {
            let b = b.expect("a blocker");
            grid.apply(&b, add, span);
            grid.apply_fill(&b, add, span);
        }
        assert!(grid.work(Duration::MAX, &list, &columns));
        let fresh = Grid::new(low, dims, &list, &columns);
        assert!(grid.bits == fresh.bits);
        assert!(grid.layers == fresh.layers);
        assert!(grid.fill_bytes == fresh.fill_bytes);
    }

    /// A big part that moves a little marks only the voxels at its edges,
    /// which come out as marking afresh marks them, and are what's listed
    /// as changed; a rotated one marks its whole box again.
    #[test]
    fn a_big_blocker_moving_a_little_marks_only_its_edges() {
        let mut list = scattered();
        let slab = |y| {
            Blocker::of_box(
                Vec3::new(0.0, y, 0.0),
                Mat3::IDENTITY,
                Vec3::new(2048.0, 20.0, 2048.0),
            )
        };
        list[0] = Some(slab(-20.0));
        let dims = IVec3::new(64, 32, 64);
        let low = IVec3::new(-40, -10, -30);
        let mut blockers = Blockers {
            blockers: list.clone(),
            columns: indexed(&list),
            ..Blockers::default()
        };
        blockers.grid = Some(Grid::new(low, dims, &list, &blockers.columns));
        let turned = Blocker::of_box(
            Vec3::new(30.0, 4.0, 10.0),
            Mat3::from_rotation_y(0.4),
            Vec3::new(9.0, 3.0, 5.0),
        );
        for (i, after) in [(0, slab(-18.5)), (3, turned)] {
            let before = list[i];
            blockers
                .columns
                .remove(i, before.as_ref().expect("a blocker"));
            blockers.columns.insert(i, &after);
            list[i] = Some(after);
            blockers.blockers[i] = Some(after);
            blockers.moved.push((before, Some(after)));
        }
        let fresh = Grid::new(low, dims, &list, &indexed(&list));
        let marked = blockers.mark(low, dims).expect("marked");
        assert!(marked.bits == fresh.bits.as_slice());
        assert!(marked.layers == fresh.layers.as_slice());
        assert!(marked.fills == fresh.fill_bytes.as_slice());
        let Touched::Spans(spans) = &marked.touched else {
            panic!("only what moved is touched");
        };
        let touched: i32 = spans
            .iter()
            .map(|(lo, hi)| (*hi - *lo + 1).element_product())
            .sum();
        // The slab's top and bottom layers, twice, and the turned box.
        assert!(touched < 6 * dims.x * dims.z, "{touched} voxels touched");
        let grid = blockers.grid.as_ref().expect("a grid");
        assert!(grid.counts == fresh.counts);
    }

    #[test]
    fn minus_leaves_what_is_outside_the_cut() {
        let span = (IVec3::ZERO, IVec3::splat(9));
        let cut = (IVec3::splat(2), IVec3::new(12, 7, 7));
        let inside = clip(cut, span).expect("they overlap");
        let pieces = minus(span, Some(cut));
        let mut covered = 0;
        for (lo, hi) in &pieces {
            covered += (*hi - *lo + 1).element_product();
            assert!(clip((*lo, *hi), inside).is_none());
        }
        assert_eq!(covered, 1000 - 8 * 6 * 6);
        assert_eq!(minus(span, None), vec![span]);
    }
}
