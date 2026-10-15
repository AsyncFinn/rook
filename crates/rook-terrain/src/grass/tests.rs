use glam::IVec3;

use super::*;
use crate::{Grid, Voxel, chunks_to_mesh, mesh_chunk};

/// The Grass tops of a full one-voxel slab `size` voxels square, of
/// `material`.
fn slab_tops(material: TerrainMaterial, size: i32) -> GrassTops {
    let mut grid = Grid::new();
    for z in 0..size {
        for x in 0..size {
            grid.set(IVec3::new(x, 0, z), Voxel::new(material, 255));
        }
    }
    let mut tops = GrassTops::default();
    for chunk in chunks_to_mesh(&grid) {
        add_grass_tops(&mesh_chunk(&grid, chunk), &mut tops);
    }
    tops
}

#[test]
fn blades_grow_on_grass_tops_only() {
    // 16 by 16 voxels: two by two tiles, and a top flat but for its
    // edges, which slope down over the outer voxels.
    let tops = slab_tops(TerrainMaterial::Grass, 16);
    assert_eq!(tops.len(), 4);
    let mut count = 0;
    for (tile, triangles) in &tops {
        let b = blades(*tile, triangles);
        count += b.count();
        assert_eq!(b.indices.len(), b.count() * 6);
        let origin = tile_origin(*tile);
        for (p, uv) in b.positions.iter().zip(&b.uvs) {
            let p = origin + Vec3::from(*p);
            // The slab's top is 4 studs above its voxels' centres.
            if uv[1] == 0.0 && (6.0..58.0).contains(&p.x) && (6.0..58.0).contains(&p.z) {
                assert!((p.y - 6.0).abs() < 1e-3, "foot at {p}");
            }
            assert!(uv[1] <= BLADE_HEIGHT);
        }
    }
    // About one blade per square stud of top, sloped edges and all.
    let area: f32 = tops
        .values()
        .flatten()
        .map(|t| (t[1] - t[0]).cross(t[2] - t[0]).length() / 2.0)
        .sum();
    assert!(
        (count as f32 / area - DENSITY).abs() < 0.1,
        "{count} blades"
    );
    // The same ground grows the same blades.
    let tile = *tops.keys().next().unwrap();
    assert_eq!(blades(tile, &tops[&tile]), blades(tile, &tops[&tile]));
    assert!(slab_tops(TerrainMaterial::Rock, 16).is_empty());
}

#[test]
fn blades_are_darker_than_the_ground() {
    let c = blade_color(Color3uint8::new(111, 126, 62));
    assert!(c.iter().all(|v| (0.0..0.6).contains(v)), "{c:?}");
    assert!(c[1] > c[0] && c[0] > c[2]);
}
