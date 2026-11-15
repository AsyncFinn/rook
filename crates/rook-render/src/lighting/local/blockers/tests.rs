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

/// No terrain.
static NO_TERRAIN: std::sync::LazyLock<Terrain> = std::sync::LazyLock::new(Terrain::default);

/// What a grid of `list` (standing in `columns`) is marked from,
/// without terrain.
fn of<'a>(list: &'a [Option<Blocker>], columns: &'a Columns) -> Sources<'a> {
    Sources {
        blockers: list,
        columns,
        terrain: &NO_TERRAIN,
    }
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
    let mut grid = Grid::new(IVec3::new(-40, -10, -30), dims, of(&list, &columns));
    // Small and large steps, every direction, and one past the whole volume.
    for low in [
        IVec3::new(-24, -10, -30),
        IVec3::new(-24, -4, -47),
        IVec3::new(-60, -12, -20),
        IVec3::new(100, 3, 90),
        IVec3::new(90, 0, 101),
    ] {
        grid.move_to(low);
        assert!(grid.work(Duration::MAX, of(&list, &columns)));
        let fresh = Grid::new(low, dims, of(&list, &columns));
        assert!(grid.counts == fresh.counts, "counts at {low}");
        assert!(grid.bits == fresh.bits, "bits at {low}");
        assert!(grid.layers == fresh.layers, "layers at {low}");
        assert!(
            grid.layers == layers_of(&grid),
            "layers match bits at {low}"
        );
    }
}

/// The blocked layers of each column, read off the bits.
fn layers_of(grid: &Grid) -> Vec<u64> {
    let mut layers = vec![0; (grid.dims.x * grid.dims.z) as usize];
    each(grid.dims, grid.span(), |_, slot| {
        if grid.bits[slot.index / 32] & (1 << (slot.index % 32)) != 0 {
            layers[slot.column] |= 1 << slot.layer;
        }
    });
    layers
}

/// What goes up to the GPU after a change covers every voxel it
/// touched, wherever the spans wrap around the slots.
#[test]
fn uploads_cover_what_changed() {
    let dims = IVec3::new(64, 32, 64);
    let spans = vec![
        (IVec3::new(-3, -2, 5), IVec3::new(20, 4, 9)),
        (IVec3::new(60, 30, 62), IVec3::new(70, 33, 66)),
        (IVec3::new(-100, 7, -40), IVec3::new(-99, 7, -39)),
    ];
    let empty: Vec<u32> = Vec::new();
    let marked = Marked {
        bits: &empty,
        layers: &[],
        soils: &empty,
        soil_layers: &[],
        touched: Touched::Spans(spans.clone()),
        dims,
    };
    let runs = marked.slot_runs();
    for span in spans {
        each(dims, span, |k, slot| {
            let sent = runs.iter().any(|r| r.contains(&slot.index));
            assert!(sent, "voxel {k} at slot {} not sent", slot.index);
        });
    }
}

/// Bricks marked apart, at volumes whose slots wrap anywhere, match
/// the whole grid marked at once.
#[test]
fn pieces_marked_apart_match_one_marked_whole() {
    let list = scattered();
    let columns = indexed(&list);
    let dims = IVec3::new(64, 32, 64);
    for low in [IVec3::new(-40, -10, -30), IVec3::new(-37, -3, 21)] {
        let pieced = Grid::new(low, dims, of(&list, &columns));
        let mut whole = Grid::empty(low, dims);
        let span = whole.span();
        whole
            .voxels()
            .take(span, of(&list, &columns), &mut Scratch::default());
        assert!(pieced.counts == whole.counts);
        assert!(pieced.bits == whole.bits);
        assert!(pieced.layers == whole.layers);
    }
}

#[test]
fn a_blocker_moving_while_the_grid_moves_lands_once() {
    let mut list = scattered();
    let mut columns = indexed(&list);
    let dims = IVec3::new(64, 32, 64);
    let mut grid = Grid::new(IVec3::new(-40, -10, -30), dims, of(&list, &columns));
    let low = IVec3::new(-20, -10, -30);
    grid.move_to(low);
    // Half the pieces marked before a blocker moves into what came in.
    let half = grid.pending.len() / 2;
    let later = grid.pending.split_off(half);
    assert!(grid.work(Duration::MAX, of(&list, &columns)));
    grid.pending = later;
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
    let moves = [(before, false), (after, true)].map(|(b, add)| (b.expect("a blocker"), add, span));
    grid.apply_moves(&moves);
    assert!(grid.work(Duration::MAX, of(&list, &columns)));
    let fresh = Grid::new(low, dims, of(&list, &columns));
    assert!(grid.bits == fresh.bits);
    assert!(grid.layers == fresh.layers);
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
    blockers.grid = Some(Grid::new(low, dims, of(&list, &blockers.columns)));
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
    let fresh = Grid::new(low, dims, of(&list, &indexed(&list)));
    let marked = blockers.mark(low, dims).expect("marked");
    assert!(marked.bits == fresh.bits.as_slice());
    assert!(marked.layers == fresh.layers.as_slice());
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

/// What `b` marks on its own: each voxel's count.
fn marks(b: Blocker, low: IVec3, dims: IVec3) -> Vec<u32> {
    let list = [Some(b)];
    let grid = Grid::new(low, dims, of(&list, &indexed(&list)));
    grid.counts
}

/// A part moved within its slack marks exactly what it marked, and
/// the slack lets tiny moves through.
#[test]
fn a_part_moved_within_its_slack_marks_the_same() {
    let mut seed = 12345u64;
    let mut rand = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 40) as f32 / (1u64 << 24) as f32
    };
    let (low, dims) = (IVec3::new(-16, -8, -16), IVec3::new(32, 16, 32));
    let span = (low, low + dims - 1);
    let (mut held, mut tiny) = (0, 0);
    for i in 0..600 {
        // Half on the half-stud grid, as built places are; half anywhere.
        let snap = |v: f32| {
            if i % 2 == 0 {
                (v * 2.0).round() * 0.5
            } else {
                v
            }
        };
        let center = Vec3::new(
            rand() * 40.0 - 20.0,
            rand() * 20.0 - 10.0,
            rand() * 40.0 - 20.0,
        );
        // Some reach well past the volume, whose slack is only its.
        let reach = if i % 7 == 0 { 400.0 } else { 6.0 };
        let half = Vec3::new(rand() * reach, rand() * 3.0, rand() * 6.0);
        let (center, half) = (center.map(snap), half.map(snap).max(Vec3::splat(0.1)));
        let random_axis = |a: f32, b: f32, c: f32| Vec3::new(a, b + 0.1, c).normalize();
        let axes = match i % 3 {
            0 => Mat3::IDENTITY,
            1 => Mat3::from_rotation_y(std::f32::consts::FRAC_PI_2),
            _ => Mat3::from_axis_angle(random_axis(rand(), rand(), rand()), rand() * 3.0),
        };
        let from = Blocker::of_box(center, axes, half);
        let slack = Slack::of(&from, span);
        for scale in [1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 0.1] {
            let shift = Vec3::new(rand() - 0.5, rand() - 0.5, rand() - 0.5) * scale;
            let turn = if i % 4 == 0 || i % 3 == 2 {
                let angle = (rand() - 0.5) * scale * 0.1;
                Mat3::from_axis_angle(random_axis(rand(), rand(), rand()), angle)
            } else {
                Mat3::IDENTITY
            };
            let to = Blocker::of_box(center + shift, turn * axes, half);
            if slack.holds(&from, &to, span) {
                held += 1;
                tiny += usize::from(scale <= 1e-4);
                assert_eq!(
                    marks(from, low, dims),
                    marks(to, low, dims),
                    "box {i} moved {scale}"
                );
            }
        }
    }
    // Some voxel center lies on a snapped box's face: no slack there,
    // but most have some.
    assert!(
        tiny > 1200 && held > tiny,
        "{tiny} of 1800 tiny moves held, {held} in all"
    );
}
/// Working a slack out pays only for parts that move little: one
/// that just moved further than any slack reaches gets none.
#[test]
fn only_parts_moving_little_work_a_slack_out() {
    let at = |x: f32, turn: f32| {
        Blocker::of_box(
            Vec3::new(x, 2.0, 2.0),
            Mat3::from_rotation_y(turn),
            Vec3::splat(1.5),
        )
    };
    assert!(Slack::worth(&at(0.0, 0.0), &at(0.001, 0.0)));
    assert!(!Slack::worth(&at(0.0, 0.0), &at(5.0, 0.0)));
    assert!(Slack::worth(&at(0.0, 0.3), &at(0.001, 0.3)));
    assert!(!Slack::worth(&at(0.0, 0.3), &at(0.0, 0.9)));
}

/// A slack covers only the voxels the volume held: a part held within
/// it is marked where it is now once the volume moves, and one that
/// moves after the volume did is marked again. Here a voxel center
/// just past the volume's edge lies outside the pose it was marked at
/// and inside where it goes.
#[test]
fn a_part_held_in_its_slack_is_marked_where_it_is_once_the_volume_moves() {
    // From 91 to just short of 98 along x, the center of the first
    // voxel past the volume.
    let at = |x: f32, z: f32| {
        let (lo, hi) = (91.0 + x, 98.0 - 0.000_05 + x);
        Blocker::of_box(
            Vec3::new((lo + hi) * 0.5, 2.0, z),
            Mat3::IDENTITY,
            Vec3::new((hi - lo) * 0.5, 1.0, 1.0),
        )
    };
    let mut list = scattered();
    list[1] = Some(at(-0.001, 10.0));
    list[2] = Some(at(-0.001, 30.0));
    let (low, dims) = (IVec3::new(-40, -10, -30), IVec3::new(64, 32, 64));
    let mut blockers = Blockers {
        blockers: list.clone(),
        slack: vec![Slack::default(); list.len()],
        columns: indexed(&list),
        ..Blockers::default()
    };
    blockers.grid = Some(Grid::new(low, dims, of(&list, &blockers.columns)));
    blockers.take(1, Some(at(0.0, 10.0)));
    blockers.take(2, Some(at(0.0, 30.0)));
    assert!(blockers.mark(low, dims).is_some());
    blockers.take(1, Some(at(0.0001, 10.0)));
    assert!(blockers.held.contains_key(&1), "held within its slack");
    let moved = low + IVec3::new(PIECE, 0, 0);
    assert!((0..10_000).any(|_| blockers.mark(moved, dims).is_some()));
    blockers.take(2, Some(at(0.0001, 30.0)));
    assert!(blockers.mark(moved, dims).is_some());
    list[1] = Some(at(0.0001, 10.0));
    list[2] = Some(at(0.0001, 30.0));
    let fresh = Grid::new(moved, dims, of(&list, &indexed(&list)));
    let grid = blockers.grid.as_ref().expect("a grid");
    assert!(grid.bits == fresh.bits, "marked where it is");
    assert!(grid.counts == fresh.counts);
}

/// A part parked as far as a float goes blocks no voxel (its voxel
/// span is past an i32's), while one nearby still does.
#[test]
fn a_part_parked_a_float_away_blocks_nothing() {
    let block = DrawShape::Part(PartShape::Block);
    let part = |y: f32| {
        let at = Vec3::new(0.0, y, 0.0);
        PartInstance::plain(block, at, Mat3::IDENTITY, Vec3::ONE)
    };
    let (mut blockers, scene) = (Blockers::default(), Scene::default());
    assert!(blockers.blocker(&part(10.0), &scene).is_some());
    assert_eq!(blockers.blocker(&part(f32::MIN), &scene), None);
    assert_eq!(blockers.blocker(&part(f32::NAN), &scene), None);
}

/// A part moving where the volume doesn't reach changes no voxel, so
/// it asks for no marking; when the volume comes onto it, it's marked
/// where it is then.
#[test]
fn a_part_moving_outside_the_volume_marks_nothing() {
    let far = |x: f32| {
        let center = Vec3::new(2000.0 + x, 2.0, 0.0);
        Blocker::of_box(center, Mat3::IDENTITY, Vec3::splat(2.0))
    };
    let mut list = scattered();
    list[1] = Some(far(0.0));
    let (low, dims) = (IVec3::new(-40, -10, -30), IVec3::new(64, 32, 64));
    let mut blockers = Blockers {
        blockers: list.clone(),
        slack: vec![Slack::default(); list.len()],
        columns: indexed(&list),
        ..Blockers::default()
    };
    blockers.grid = Some(Grid::new(low, dims, of(&list, &blockers.columns)));
    assert!(blockers.mark(low, dims).is_some());
    for step in 1..4 {
        blockers.take(1, Some(far(step as f32 * 8.0)));
    }
    assert!(!blockers.due());
    assert!(blockers.moved.is_empty());

    let there = IVec3::new(500 - 32, -10, -30);
    assert!((0..10_000).any(|_| blockers.mark(there, dims).is_some()));
    list[1] = Some(far(24.0));
    let fresh = Grid::new(there, dims, of(&list, &indexed(&list)));
    let grid = blockers.grid.as_ref().expect("a grid");
    assert!(grid.bits == fresh.bits, "marked where it is");
    assert!(grid.counts == fresh.counts);
}

/// Terrain edited after the voxels were marked marks only the edit's
/// voxels again, as marking afresh marks them; air and water hold no
/// terrain, and the parts' marks stay as they were.
#[test]
fn a_terrain_edit_marks_its_voxels_as_marking_afresh_does() {
    use rook_terrain::grid::edit::Bounds;
    use rook_terrain::{TerrainMaterial, Voxel};
    let mut voxels = rook_terrain::Grid::new();
    for x in -30i32..30 {
        for z in -30..30 {
            let y = (x + z).rem_euclid(5) - 2;
            let occupancy = (x * 7 + z * 3).rem_euclid(256) as u8;
            voxels.set(
                IVec3::new(x, y, z),
                Voxel::new(TerrainMaterial::Rock, occupancy),
            );
        }
    }
    let water = IVec3::new(3, 9, 3);
    voxels.set(water, Voxel::new(TerrainMaterial::Water, 255));
    let list = scattered();
    let mut blockers = Blockers {
        blockers: list.clone(),
        columns: indexed(&list),
        ..Blockers::default()
    };
    blockers.update_terrain(TerrainVoxels::Whole(voxels.clone()));
    let (low, dims) = (IVec3::new(-40, -10, -30), IVec3::new(64, 32, 64));
    assert!(blockers.mark(low, dims).is_some());

    // Dig some out and fill some in, across a chunk's edge.
    let edit = Bounds {
        min: IVec3::new(-6, -3, -6),
        max: IVec3::new(5, 3, 5),
    };
    for x in edit.min.x..=edit.max.x {
        for y in edit.min.y..=edit.max.y {
            for z in edit.min.z..=edit.max.z {
                let v = match (x + y) % 2 == 0 {
                    true => Voxel::AIR,
                    false => Voxel::new(TerrainMaterial::Grass, 200),
                };
                voxels.set(IVec3::new(x, y, z), v);
            }
        }
    }
    blockers.update_terrain(TerrainVoxels::Edits {
        chunks: voxels.region(edit),
        bounds: vec![edit],
    });
    assert!(blockers.due());
    let mut fresh_terrain = Terrain::default();
    fresh_terrain.take(TerrainVoxels::Whole(voxels));
    let fresh = Grid::new(
        low,
        dims,
        Sources {
            blockers: &list,
            columns: &indexed(&list),
            terrain: &fresh_terrain,
        },
    );
    let marked = blockers.mark(low, dims).expect("marked");
    assert!(marked.soils == fresh.soils.as_slice());
    assert!(marked.soil_layers == fresh.soil_layers.as_slice());
    assert!(marked.bits == fresh.bits.as_slice());
    let Touched::Spans(spans) = &marked.touched else {
        panic!("only the edit is touched");
    };
    assert_eq!(spans, &vec![(edit.min, edit.max)]);
    // Some terrain is marked, none where the water is.
    assert!(fresh.soils.iter().any(|&w| w != 0));
    each(dims, (water, water), |_, slot| {
        let byte = (fresh.soils[slot.index / 4] >> ((slot.index % 4) * 8)) & 0xff;
        assert_eq!(byte, 0, "water holds no terrain");
    });
}
