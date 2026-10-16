//! Fills, merges and regions against what Studio 0.742's `ReadVoxels`
//! read back after the same calls (2026-10-16; bytes as `SmoothGrid`
//! stores them).

use glam::{Affine3A, IVec3, Quat, Vec3};

use super::*;
use crate::MESH_REACH;
use crate::grid::cells::*;
use TerrainMaterial::{Air, Grass, Rock, Water};

/// Studio's voxel 0 centre, where the probes put their shapes.
const C: Vec3 = Vec3::splat(2.0);

fn at(p: Vec3) -> Affine3A {
    Affine3A::from_translation(p)
}

/// The voxels a fill of Grass leaves, as (voxel, byte), sorted.
fn filled(shape: Shape) -> Vec<((i32, i32, i32), u8)> {
    let mut grid = Grid::new();
    let bounds = grid.fill(&shape, Grass).expect("something filled");
    let mut out: Vec<_> = bounds
        .voxels()
        .filter_map(|v| {
            let voxel = grid.voxel(v);
            (voxel.material != Air).then_some(((v.x, v.y, v.z), voxel.occupancy))
        })
        .collect();
    out.sort();
    out
}

#[test]
fn blocks_fill_their_least_overlap() {
    let block = |p: Vec3, size: Vec3| filled(Shape::Block { pose: at(p), size });
    assert_eq!(block(C, Vec3::splat(4.0)), [((0, 0, 0), 255)]);
    // Straddling eight voxels fills each half, not an eighth.
    let straddle = block(Vec3::ZERO, Vec3::splat(4.0));
    assert_eq!(straddle.len(), 8);
    assert!(straddle.iter().all(|(_, b)| *b == 128));
    assert_eq!(block(C, Vec3::ONE), [((0, 0, 0), 64)]);
    assert_eq!(block(C, Vec3::splat(3.0)), [((0, 0, 0), 191)]);
    assert_eq!(
        block(Vec3::new(2.0, 2.5, 2.0), Vec3::splat(4.0)),
        [((0, 0, 0), 223), ((0, 1, 0), 32)]
    );
    let turned = filled(Shape::Block {
        pose: Affine3A::from_rotation_translation(Quat::from_rotation_y(45f32.to_radians()), C),
        size: Vec3::new(8.0, 4.0, 8.0),
    });
    let byte = |v| turned.iter().find(|(at, _)| *at == v).map(|(_, b)| *b);
    assert_eq!(byte((0, 0, 0)), Some(255));
    assert_eq!(byte((1, 0, 0)), Some(202));
    assert_eq!(byte((1, 0, 1)), Some(22));
    assert_eq!(turned.len(), 9);
}

#[test]
fn balls_fill_by_distance() {
    let ball = |center, radius| filled(Shape::Ball { center, radius });
    assert_eq!(ball(C, 2.0), [((0, 0, 0), 255)]);
    assert_eq!(ball(C, 0.5), [((0, 0, 0), 159)]);
    let r4 = ball(C, 4.0);
    assert_eq!(r4.len(), 19);
    let byte = |v| r4.iter().find(|(at, _)| *at == v).map(|(_, b)| *b);
    assert_eq!(byte((1, 0, 0)), Some(128));
    assert_eq!(byte((1, 1, 0)), Some(22));
    let r6 = ball(Vec3::ZERO, 6.0);
    assert_eq!(r6.len(), 32);
    assert!(r6.contains(&((-2, -1, -1), 87)));
}

#[test]
fn cylinders_cut_their_ends() {
    let cylinder = |p, height, radius| {
        filled(Shape::Cylinder {
            pose: at(p),
            height,
            radius,
        })
    };
    // A voxel only touching the end isn't filled; one reaching past it is
    // filled whole.
    assert_eq!(cylinder(C, 4.0, 4.0), [((0, 0, 0), 255)]);
    assert_eq!(cylinder(C, 8.0, 4.0).len(), 3);
    assert_eq!(cylinder(C, 4.0, 3.0), [((0, 0, 0), 191)]);
    let r6 = cylinder(C, 4.0, 6.0);
    assert!(r6.contains(&((1, 0, 0), 127)) && r6.contains(&((1, 0, 1), 21)));
    let corner = cylinder(Vec3::ZERO, 8.0, 4.0);
    assert_eq!(corner.len(), 8);
    assert!(corner.iter().all(|(_, b)| *b == 74));
}

#[test]
fn wedges_slope_down_to_minus_z() {
    let wedge = |p, size| filled(Shape::Wedge { pose: at(p), size });
    let profile = wedge(C, Vec3::new(4.0, 8.0, 16.0));
    assert_eq!(
        profile,
        [
            ((0, -1, -2), 64),
            ((0, -1, -1), 191),
            ((0, -1, 0), 255),
            ((0, -1, 1), 255),
            ((0, -1, 2), 255),
            ((0, 0, 0), 64),
            ((0, 0, 1), 191),
            ((0, 0, 2), 255),
            ((0, 1, 2), 64),
        ]
    );
    // Across, a wedge fills as a block does.
    let flat = wedge(C, Vec3::new(8.0, 4.0, 8.0));
    assert!(flat.contains(&((1, 0, 1), 128)) && flat.contains(&((0, 0, 1), 191)));
    // A voxel-sized wedge fills nothing: its slope crosses the centre.
    let mut grid = Grid::new();
    let small = Shape::Wedge {
        pose: at(C),
        size: Vec3::splat(4.0),
    };
    assert_eq!(grid.fill(&small, Grass), None);
}

#[test]
fn fills_merge_as_studio_does() {
    let block = |size: f32| Shape::Block {
        pose: at(C),
        size: Vec3::splat(size),
    };
    let after = |first: (f32, TerrainMaterial), then: (f32, TerrainMaterial)| {
        let mut grid = Grid::new();
        grid.fill(&block(first.0), first.1);
        grid.fill(&block(then.0), then.1);
        let v = grid.voxel(IVec3::ZERO);
        (v.material, v.occupancy)
    };
    assert_eq!(after((4.0, Grass), (2.0, Rock)), (Grass, 255));
    assert_eq!(after((2.0, Grass), (3.0, Rock)), (Rock, 191));
    assert_eq!(after((2.0, Grass), (2.0, Rock)), (Rock, 128));
    assert_eq!(after((4.0, Water), (2.0, Rock)), (Water, 255));
    assert_eq!(after((4.0, Grass), (2.0, Water)), (Grass, 255));
    assert_eq!(after((4.0, Grass), (2.0, Air)), (Grass, 127));
    assert_eq!(after((4.0, Grass), (4.0, Air)), (Air, 0));
    assert!(
        Shape::Ball {
            center: C,
            radius: -4.0
        }
        .is_empty()
    );
}

#[test]
fn regions_truncate_and_replace() {
    let v = |x, y, z| Vec3::new(x, y, z);
    assert_eq!(region_voxels(v(-1.9, 0.0, 0.0), v(2.1, 4.0, 4.0)).1.x, 0);
    assert_eq!(region_voxels(v(2.0, 0.0, 0.0), v(6.0, 4.0, 4.0)).1.x, 1);
    assert_eq!(
        region_voxels(v(1.0, 0.0, 0.0), v(8.0, 4.0, 4.0)),
        (IVec3::ZERO, IVec3::new(2, 1, 1))
    );
    let mut grid = Grid::new();
    let one = Bounds::of_block(IVec3::ZERO, IVec3::ONE).unwrap();
    grid.set(IVec3::ZERO, Voxel::new(Grass, 128));
    grid.replace(one, Grass, Water);
    assert_eq!(grid.voxel(IVec3::ZERO), Voxel::new(Water, 128));
    grid.replace(one, Water, Air);
    assert_eq!(grid.voxel(IVec3::ZERO), Voxel::AIR);
    assert_eq!(grid.count_cells(), 0);
}

#[test]
fn copies_paste_with_or_without_air() {
    let mut grid = Grid::new();
    grid.set(IVec3::ZERO, Voxel::new(Grass, 255));
    grid.set(IVec3::X, Voxel::new(Rock, 128));
    let copy = grid.copy(Bounds::of_block(IVec3::ZERO, IVec3::new(3, 1, 1)).unwrap());
    let sand = Voxel::new(TerrainMaterial::Sand, 255);
    let corner = IVec3::new(10, 0, 0);
    grid.set(corner + 2 * IVec3::X, sand);
    grid.paste(&copy, IVec3::new(3, 1, 1), corner, false);
    assert_eq!(grid.voxel(corner + IVec3::X), Voxel::new(Rock, 128));
    assert_eq!(grid.voxel(corner + 2 * IVec3::X), sand);
    grid.paste(&copy, IVec3::new(3, 1, 1), corner, true);
    assert_eq!(grid.voxel(corner + 2 * IVec3::X), Voxel::AIR);
}

#[test]
fn cells() {
    assert_eq!(
        world_to_cell(Vec3::new(5.0, -1.0, 9.9)),
        IVec3::new(1, -1, 2)
    );
    assert_eq!(
        cell_center(Vec3::new(-1.5, 2.5, 0.4)),
        Vec3::new(-6.0, 14.0, 2.0)
    );
    assert_eq!(
        cell_corner(Vec3::new(1.0, 2.0, 3.0)),
        Vec3::new(4.0, 8.0, 12.0)
    );
    let mut grid = Grid::new();
    grid.fill_block(
        Bounds::of_block(IVec3::splat(-2), IVec3::splat(4)).unwrap(),
        Rock,
    );
    let p = Vec3::splat(8.0);
    assert_eq!(world_to_cell_prefer(&grid, p, true), IVec3::ONE);
    assert_eq!(world_to_cell_prefer(&grid, p, false), IVec3::splat(2));
    assert_eq!(
        world_to_cell_prefer(&grid, Vec3::new(8.0, 5.0, 5.0), true),
        IVec3::ONE
    );
    assert_eq!(
        from_cell_material("MossyStone"),
        Some(TerrainMaterial::Pavement)
    );
    assert_eq!(to_cell_material(TerrainMaterial::Pavement), "MossyStone");
    assert_eq!(to_cell_material(Rock), "Grass");
}

#[test]
fn changes_box_the_voxels_that_differ() {
    let mut a = Grid::new();
    a.fill_block(
        Bounds::of_block(IVec3::ZERO, IVec3::splat(40)).unwrap(),
        Rock,
    );
    let mut b = a.clone();
    b.set(IVec3::new(3, 4, 5), Voxel::AIR);
    b.set(IVec3::new(6, 4, 5), Voxel::AIR);
    assert_eq!(
        a.changes(&b),
        [Bounds {
            min: IVec3::new(3, 4, 5),
            max: IVec3::new(6, 4, 5),
        }]
    );
    // Within two voxels of chunk 0's low corner: its neighbours below too.
    let corner = Bounds {
        min: IVec3::ZERO,
        max: IVec3::ZERO,
    };
    assert_eq!(corner.chunks(MESH_REACH).count(), 8);
    assert!(a.changes(&a.clone()).is_empty());
}
