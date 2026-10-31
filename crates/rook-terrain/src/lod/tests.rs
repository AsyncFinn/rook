use super::*;
use crate::random::Random;
use crate::{TerrainMaterial, Voxel, mesh_chunk_at};

/// The level drawing the point `p`: the levels whose chunk holding it they
/// draw. Exactly one, or a ring would leave a hole or draw twice.
fn levels_at(rings: &Rings, p: Vec3) -> Vec<u8> {
    (0..LEVELS)
        .filter(|&k| rings.draws(k, (p / chunk_studs(k)).floor().as_ivec3()))
        .collect()
}

#[test]
fn every_point_is_drawn_by_one_level() {
    let mut r = Random::at(IVec3::new(1, 2, 3));
    let mut next = |span: f32| (r.next() * 2.0 - 1.0) * span;
    let mut rings = Rings::around(Vec3::ZERO);
    for _ in 0..200 {
        // A walk, so the boxes move from where they were (hysteresis and
        // nesting clamps included), not only placed fresh.
        let camera = Vec3::new(next(6000.0), next(600.0), next(6000.0));
        for step in 0..8 {
            rings.follow(camera * (step as f32 / 7.0));
        }
        for _ in 0..50 {
            let p = camera + Vec3::new(next(20000.0), next(3000.0), next(20000.0));
            assert_eq!(levels_at(&rings, p).len(), 1, "{p} from {camera}");
        }
        // The camera's own spot is the finest level's.
        assert_eq!(levels_at(&rings, camera), [0], "{camera}");
    }
}

#[test]
fn rings_match_studio_from_a_grid_corner() {
    // Measured from a camera on a 4,096-stud grid line: 4-stud cells to
    // 256 studs, 32-stud ones from 1,024 to 2,048 (Studio's 8-stud ring
    // reaches about 768, Rook's 512).
    let rings = Rings::around(Vec3::new(0.0, 30.0, 64.0));
    let level = |x: f32| levels_at(&rings, Vec3::new(x, 4.0, 96.0))[0];
    assert_eq!([level(250.0), level(260.0)], [0, 1]);
    assert_eq!([level(1020.0), level(1030.0)], [2, 3]);
    assert_eq!([level(2040.0), level(2060.0)], [3, 4]);
}

#[test]
fn small_moves_keep_the_rings() {
    let mut rings = Rings::around(Vec3::new(10.0, 10.0, 10.0));
    assert!(!rings.follow(Vec3::new(130.0, 10.0, -100.0)));
    // Past half a level-1 chunk and the slack: the finest box moves, the
    // coarser ones stay.
    let before = rings;
    assert!(rings.follow(Vec3::new(280.0, 10.0, 10.0)));
    assert_ne!(rings.centers[0], before.centers[0]);
    assert_eq!(rings.centers[1..], before.centers[1..]);
}

#[test]
fn coarse_voxels_average_their_eight() {
    let mut fine = Grid::new();
    for x in 0..64 {
        for z in 0..64 {
            fine.set(IVec3::new(x, 0, z), Voxel::new(TerrainMaterial::Grass, 255));
            fine.set(IVec3::new(x, 1, z), Voxel::new(TerrainMaterial::Rock, 127));
        }
    }
    let coarse = fine.coarse();
    let v = coarse.voxel(IVec3::new(3, 0, 3));
    // Four full Grass and four half Rock: three quarters, mostly Grass.
    assert_eq!(v.material, TerrainMaterial::Grass);
    assert_eq!(v.occupancy, 191);
    assert_eq!(coarse.voxel(IVec3::new(3, 1, 3)), Voxel::AIR);
    let levels = pyramid(fine);
    assert_eq!(levels.len(), usize::from(LEVELS));
    assert_ne!(levels[5].voxel(IVec3::ZERO), Voxel::AIR);
}

#[test]
fn skirts_are_drawn_but_not_collided_with() {
    let mut grid = Grid::new();
    for x in -4..40 {
        for z in -4..40 {
            grid.set(IVec3::new(x, 0, z), Voxel::new(TerrainMaterial::Grass, 255));
        }
    }
    let bare = mesh_chunk_at(&grid, IVec3::ZERO, Detail::FULL);
    let hung = mesh_chunk_at(&grid, IVec3::ZERO, Detail::at(0));
    assert_eq!(bare.surfaces, hung.surfaces);
    let drawn = |m: &crate::ChunkMesh| -> usize { m.draws.iter().map(|d| d.indices.len()).sum() };
    // A 32 × 32 top has 128 open edges, each hung with two triangles.
    assert_eq!(drawn(&hung) - drawn(&bare), 128 * 2 * 3);
}

/// The highest and lowest drawn point of chunk `chunk` of `level`, in
/// world studs.
fn drawn_heights(levels: &[Grid], level: u8, chunk: IVec3) -> (f32, f32) {
    let mesh = mesh_chunk_at(&levels[usize::from(level)], chunk, Detail::at(level));
    let origin = mesh.origin();
    let ys = mesh
        .draws
        .iter()
        .flat_map(|d| d.positions.iter())
        .map(|p| p[1] + origin.y);
    ys.fold((f32::MIN, f32::MAX), |(hi, lo), y| (hi.max(y), lo.min(y)))
}

/// A Grass slab under a 3,072-stud square, its top at y = 0 and `depth`
/// voxels thick.
fn slab(depth: i32) -> Vec<Grid> {
    let mut grid = Grid::new();
    for x in -384..384 {
        for z in -384..384 {
            for y in -depth..0 {
                grid.set(IVec3::new(x, y, z), Voxel::new(TerrainMaterial::Grass, 255));
            }
        }
    }
    pyramid(grid)
}

#[test]
fn far_ground_lies_where_near_ground_does() {
    // Studio's slab edge runs straight to the horizon across its rings; a
    // coarse voxel's surface half its own voxel out stood 2 to 62 studs
    // higher.
    let levels = slab(40);
    for level in 0..LEVELS {
        let (top, _) = drawn_heights(&levels, level, IVec3::new(0, -1, 0));
        assert!((top - 2.0).abs() < 1e-3, "level {level}: top at {top}");
    }
}

#[test]
fn skirts_never_rise_above_the_ground() {
    // Under a slab thinner than a coarse voxel, skirts hung from its
    // underside stood up through it as walls and shards.
    let levels = slab(6);
    for level in 0..LEVELS {
        let (top, _) = drawn_heights(&levels, level, IVec3::new(0, -1, 0));
        assert!(top <= 2.0 + 1e-3, "level {level}: reaches {top}");
    }
}

#[test]
fn rings_are_squares_on_the_ground() {
    // Studio draws the terrain under a camera 4,000 studs up in 4-stud
    // cells.
    let rings = Rings::around(Vec3::new(10.0, 4000.0, 10.0));
    assert_eq!(levels_at(&rings, Vec3::new(10.0, 0.0, 10.0)), [0]);
    assert_eq!(levels_at(&rings, Vec3::new(10.0, -3000.0, 10.0)), [0]);
}

#[test]
fn far_lakes_lie_at_their_level() {
    // A lake one voxel deep on a bed of Rock: averaged alone, its water
    // sank into the bed from 16-stud voxels on, and far lakes drew as bare
    // ground.
    let mut grid = Grid::new();
    for x in -384..384 {
        for z in -384..384 {
            for y in -40..-1 {
                grid.set(IVec3::new(x, y, z), Voxel::new(TerrainMaterial::Rock, 255));
            }
            grid.set(
                IVec3::new(x, -2, z),
                Voxel::new(TerrainMaterial::Water, 255),
            );
        }
    }
    let levels = pyramid(grid);
    for level in 0..LEVELS {
        let mesh = mesh_chunk_at(
            &levels[usize::from(level)],
            IVec3::new(0, -1, 0),
            Detail::at(level),
        );
        let y = mesh.origin().y;
        let water: Vec<f32> = mesh
            .draws
            .iter()
            .filter(|d| d.layer.material == TerrainMaterial::Water)
            .flat_map(|d| d.positions.iter().map(|p| p[1] + y))
            .collect();
        assert!(!water.is_empty(), "level {level}: no water");
        let top = water.iter().copied().fold(f32::MIN, f32::max);
        assert!((top + 4.0).abs() < 1e-3, "level {level}: water at {top}");
    }
}

#[test]
fn thin_slabs_keep_their_thickness_far_away() {
    // Measured in Studio: a slab one voxel thick (20 to 24 studs) and a
    // quarter full spans 21 to 23 near, then its coarse voxel's center
    // and 1 either side: 19 to 21 in 8-stud cells, 23 to 25 in 16 and 15
    // to 17 in 32. Collapsed to the center, each ring drew it a stud lower
    // and the rings stepped against each other.
    let mut grid = Grid::new();
    for x in -384..384 {
        for z in -384..384 {
            grid.set(IVec3::new(x, 5, z), Voxel::new(TerrainMaterial::Snow, 63));
        }
    }
    let levels = pyramid(grid);
    for (level, want) in [(0, 23.0), (1, 21.0), (2, 25.0), (3, 17.0)] {
        let (top, _) = drawn_heights(&levels, level, IVec3::ZERO);
        assert!((top - want).abs() < 1e-3, "level {level}: top at {top}");
    }
    // A full slab from 8 to 32 studs, in a 32-stud voxel with air on both
    // sides: Studio's top was at 30.9 (near, 34).
    let mut grid = Grid::new();
    for x in -384..384 {
        for z in -384..384 {
            for y in 2..8 {
                grid.set(IVec3::new(x, y, z), Voxel::new(TerrainMaterial::Snow, 255));
            }
        }
    }
    let (top, _) = drawn_heights(&pyramid(grid), 3, IVec3::ZERO);
    assert!((top - 30.9).abs() < 1.0, "thick slab's top at {top}");
}

#[test]
fn no_level_comes_nearer_than_nearest_says() {
    // Grass grows on the drawn chunks of the levels `nearest` puts within
    // its reach; a level drawn nearer would show bare ground.
    let mut r = Random::at(IVec3::new(4, 5, 6));
    let mut next = |span: f32| (r.next() * 2.0 - 1.0) * span;
    let mut rings = Rings::around(Vec3::ZERO);
    let mut camera = Vec3::ZERO;
    for _ in 0..2000 {
        camera += Vec3::new(next(150.0), 0.0, next(150.0));
        rings.follow(camera);
        for _ in 0..40 {
            let angle = next(std::f32::consts::PI);
            for k in 1..LEVELS {
                let d = nearest(k) - 0.5;
                let p = camera + Vec3::new(angle.cos(), 0.0, angle.sin()) * d;
                assert!(!levels_at(&rings, p).contains(&k), "level {k} at {p} from {camera}");
            }
        }
    }
}
