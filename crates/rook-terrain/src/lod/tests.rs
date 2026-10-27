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
