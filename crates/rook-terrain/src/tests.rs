//! The decoder on hand-built data, and the mesher against Studio's
//! raycasts (`data/studio-*.json`, recorded in Studio 0.742 on 2026-10-15
//! with `WriteVoxels` and `Workspace:Raycast`; see
//! `docs/behavior/terrain/surface.md`).

use glam::{IVec3, Vec3};
use serde_json::Value as Json;

use crate::grid::decode::DecodeError;
use crate::mesh::{Detail, mesh_chunk_with};
use crate::*;

/// Encode chunks given in `SmoothGrid` order.
fn encode(chunks: &[(IVec3, Vec<Voxel>)]) -> Vec<u8> {
    let mut grid = Grid::new();
    for (at, voxels) in chunks {
        grid.insert_chunk(*at, voxels.clone());
    }
    grid.encode()
}

#[test]
fn decodes_chunks_runs_and_partial_voxels() {
    let mut a = vec![Voxel::AIR; 32768];
    a[0] = Voxel::new(TerrainMaterial::Grass, 255);
    a[1] = Voxel::new(TerrainMaterial::Rock, 100);
    // Y is slowest, X fastest.
    a[32 * 32 + 3] = Voxel::new(TerrainMaterial::Sand, 255);
    let b = vec![Voxel::new(TerrainMaterial::Mud, 255); 32768];
    let bytes = encode(&[(IVec3::new(-1, 0, 2), a), (IVec3::new(-1, 0, 3), b)]);
    let grid = Grid::decode(&bytes).unwrap();
    let origin = IVec3::new(-32, 0, 64);
    assert_eq!(grid.voxel(origin).material, TerrainMaterial::Grass);
    assert_eq!(grid.voxel(origin + IVec3::X).occupancy, 100);
    assert_eq!(
        grid.voxel(origin + IVec3::new(3, 1, 0)).material,
        TerrainMaterial::Sand
    );
    assert_eq!(
        grid.voxel(origin + IVec3::new(0, 0, 32)).material,
        TerrainMaterial::Mud
    );
    assert_eq!(grid.voxel(IVec3::ZERO), Voxel::AIR);
    assert_eq!(grid.counts()[TerrainMaterial::Mud as usize], 32768);
}

#[test]
fn rejects_damaged_data() {
    assert!(Grid::decode(&[]).unwrap().is_empty());
    assert!(matches!(Grid::decode(&[2, 5]), Err(DecodeError::Header(_))));
    let bytes = encode(&[(IVec3::ZERO, vec![Voxel::AIR; 32768])]);
    assert!(matches!(
        Grid::decode(&bytes[..bytes.len() - 1]),
        Err(DecodeError::Truncated(0))
    ));
    let mut bad = bytes;
    bad[14] = 0x80 | 40;
    assert!(matches!(Grid::decode(&bad), Err(DecodeError::Material(40))));
}

/// Every triangle of the grid's surface in world space, with its material.
fn triangles(grid: &Grid, diagonals: [[bool; 2]; 3]) -> Vec<([Vec3; 3], TerrainMaterial)> {
    let mut out = Vec::new();
    for chunk in chunks_to_mesh(grid) {
        let mesh = mesh_chunk_with(grid, chunk, Detail::FULL, diagonals);
        for s in &mesh.surfaces {
            for t in s.indices.chunks(3) {
                let p = |i: u32| mesh.origin() + Vec3::from(s.positions[i as usize]);
                out.push(([p(t[0]), p(t[1]), p(t[2])], s.material));
            }
        }
    }
    out
}

/// The nearest triangle a ray hits: distance, geometric normal, material.
fn cast(
    tris: &[([Vec3; 3], TerrainMaterial)],
    origin: Vec3,
    dir: Vec3,
) -> Option<(f32, Vec3, TerrainMaterial)> {
    let mut best: Option<(f32, Vec3, TerrainMaterial)> = None;
    for ([a, b, c], m) in tris {
        let (e1, e2) = (*b - *a, *c - *a);
        let p = dir.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1e-9 {
            continue;
        }
        let s = origin - *a;
        let u = s.dot(p) / det;
        let q = s.cross(e1);
        let v = dir.dot(q) / det;
        let t = e2.dot(q) / det;
        let e = -1e-5;
        if u < e || v < e || u + v > 1.0 - e || t <= 0.0 {
            continue;
        }
        if best.is_none_or(|(bt, ..)| t < bt) {
            best = Some((t, e1.cross(e2).normalize(), *m));
        }
    }
    best
}

fn load(name: &str) -> Json {
    let path = format!("{}/data/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The stored byte of a recorded occupancy: the recordings keep
/// `ReadVoxels`' value times 255, rounded.
fn byte(recorded: u64) -> u8 {
    ((recorded as f32 * 256.0 / 255.0).round() - 1.0).clamp(0.0, 255.0) as u8
}

/// A recorded ray: where it starts, its direction, how far Studio's hit
/// was, the hit's normal and material.
type Ray = (Vec3, Vec3, f32, Vec3, Option<TerrainMaterial>);

/// The Studio tests' voxels started at 400 studs: voxel 100.
const STUDIO_ORIGIN: i32 = 100;

fn studio_slab() -> (Grid, Vec<Ray>) {
    let data = load("studio-slab.json");
    let mut grid = Grid::new();
    let occ = data["occ"].as_array().unwrap();
    for x in 0..8 {
        for z in 0..8 {
            let v = IVec3::new(x, 0, z) + IVec3::splat(STUDIO_ORIGIN);
            grid.set(v, Voxel::new(TerrainMaterial::Grass, 255));
            let o = byte(occ[(x * 8 + z) as usize].as_u64().unwrap());
            grid.set(v + IVec3::Y, Voxel::new(TerrainMaterial::Grass, o));
        }
    }
    let base = Vec3::splat(STUDIO_ORIGIN as f32 * 4.0);
    let rays = data["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            let n = |i: usize| h[i].as_f64().unwrap() as f32;
            let origin = base + Vec3::new(n(0), 40.0, n(1));
            let hit = Vec3::new(n(0), n(2), n(1));
            (
                origin,
                Vec3::NEG_Y,
                40.0 - hit.y,
                Vec3::new(n(3), n(4), n(5)),
                None,
            )
        })
        .collect();
    (grid, rays)
}

fn studio_blob(file: &str) -> (Grid, Vec<Ray>) {
    let data = load(file);
    // Newer recordings keep the stored byte itself.
    let exact = data["bytes"].as_bool() == Some(true);
    let mut grid = Grid::new();
    for v in data["vox"].as_array().unwrap() {
        let i = |k: usize| v[k].as_i64().unwrap() as i32;
        let material = TerrainMaterial::ALL
            .into_iter()
            .find(|m| m.name() == v[4].as_str().unwrap())
            .unwrap();
        let at = IVec3::new(i(0), i(1), i(2)) + IVec3::splat(STUDIO_ORIGIN);
        let occupancy = if exact { i(3) as u8 } else { byte(i(3) as u64) };
        grid.set(at, Voxel::new(material, occupancy));
    }
    let base = Vec3::splat(STUDIO_ORIGIN as f32 * 4.0);
    let side = 24.0;
    let rays = data["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            let n = |i: usize| h[i].as_f64().unwrap() as f32;
            let axis = h[0].as_u64().unwrap() as usize - 1;
            let sign = n(1);
            let mut origin = Vec3::ZERO;
            origin[axis] = if sign < 0.0 { side + 10.0 } else { -10.0 };
            let mut dir = Vec3::ZERO;
            dir[axis] = sign;
            let (a, b) = match axis {
                0 => (1, 2),
                1 => (0, 2),
                _ => (0, 1),
            };
            origin[a] = n(2);
            origin[b] = n(3);
            let hit = Vec3::new(n(4), n(5), n(6));
            let material = TerrainMaterial::ALL[1..]
                .iter()
                .copied()
                .find(|m| m.name().starts_with(h[10].as_str().unwrap()));
            (
                base + origin,
                dir,
                (hit - origin).length(),
                Vec3::new(n(7), n(8), n(9)),
                material,
            )
        })
        .collect();
    (grid, rays)
}

/// How far Rook's surface is from Studio's along each ray, measured
/// across the surface (along a grazing ray, a tiny offset of the surface
/// moves the hit far), splitting quads along [`mesh::DIAGONALS`], or, when
/// `either`, along whichever diagonal fits:
/// (worst error, rays off by more than `tolerance`, rays checked). The
/// vertices are Roblox's; which diagonal it splits a bent quad along isn't
/// known yet (`docs/behavior/terrain/surface.md`).
fn compare(grid: &Grid, rays: &[Ray], tolerance: f32, either: bool) -> (f32, usize, usize) {
    let mut meshes = vec![triangles(grid, mesh::DIAGONALS)];
    if either {
        meshes.push(triangles(grid, mesh::DIAGONALS.map(|d| d.map(|b| !b))));
    }
    let mut worst = 0.0f32;
    let mut off = 0;
    for (origin, dir, distance, normal, _) in rays {
        let err = meshes
            .iter()
            .map(|tris| match cast(tris, *origin, *dir) {
                Some((t, ..)) => (t - distance).abs() * dir.dot(*normal).abs(),
                None => f32::INFINITY,
            })
            .fold(f32::INFINITY, f32::min);
        worst = worst.max(err);
        if err > tolerance {
            off += 1;
        }
    }
    (worst, off, rays.len())
}

#[test]
fn single_voxel_is_a_cube_of_four_thirds_its_occupancy() {
    use TerrainMaterial::*;
    // Measured in a legacy place: a lone full voxel reaches 1.333 studs
    // from its center on every side, a half-full one 0.667; Cobblestone
    // 1.40, Pavement 1.00, Concrete and Brick 2.00 (any occupancy for
    // Brick).
    for (material, occupancy, half) in [
        (Grass, 255, 4.0 / 3.0),
        (Grass, 127, 4.0 / 3.0 * 0.5),
        (Cobblestone, 255, 2.0 - 0.1499 * 4.0),
        (Pavement, 255, 1.0),
        (Concrete, 255, 2.0),
        (Brick, 127, 2.0),
    ] {
        let mut grid = Grid::new();
        grid.set(IVec3::new(3, 3, 3), Voxel::new(material, occupancy));
        let tris = triangles(&grid, mesh::DIAGONALS);
        let center = Vec3::splat(14.0);
        for dir in [Vec3::X, Vec3::NEG_Y, Vec3::Z] {
            let (t, normal, m) = cast(&tris, center - dir * 10.0, dir).unwrap();
            assert!((t - (10.0 - half)).abs() < 1e-4, "{material:?} {dir}: {t}");
            assert!(normal.dot(-dir) > 0.999);
            assert_eq!(m, material);
        }
    }
}

#[test]
fn studio_slab_matches() {
    // A full layer under a layer of random occupancies: every quad of
    // this heightfield splits along `DIAGONALS`.
    let (grid, rays) = studio_slab();
    let (worst, off, n) = compare(&grid, &rays, 0.01, false);
    assert_eq!(off, 0, "worst {worst} over {n} rays");
}

#[test]
fn studio_blob_matches() {
    // A 4x4x4 block of random voxels (60% solid, 20-100% full) in three
    // materials. Six of 2,300 rays are off: where the ray crosses from a
    // quad Studio splits one way to a neighbor it splits the other.
    let (grid, rays) = studio_blob("studio-blob.json");
    let (worst, off, n) = compare(&grid, &rays, 0.01, true);
    assert!(off <= 6, "{off} rays off, worst {worst}, of {n}");
}

#[test]
fn studio_material_shapes_match() {
    // The same kind of block in one material that moves its vertices:
    // Brick (cubify), Pavement and Concrete (quantize).
    for file in [
        "studio-brick.json",
        "studio-pavement.json",
        "studio-concrete.json",
    ] {
        let (grid, rays) = studio_blob(file);
        let (worst, off, n) = compare(&grid, &rays, 0.01, true);
        assert_eq!(off, 0, "{file}: worst {worst} over {n} rays");
    }
    // Brick, WoodPlanks, Pavement, Concrete and Grass mixed: where a cell
    // mixes materials, Rook moves its vertex by the material with the most
    // occupancy there, and 13% of rays miss (Approximation).
    let (grid, rays) = studio_blob("studio-materials.json");
    let (worst, off, n) = compare(&grid, &rays, 0.01, true);
    assert!(off * 100 <= n * 13, "{off} rays off, worst {worst}, of {n}");
}

#[test]
fn water_sits_half_a_voxel_low() {
    // Measured: a lone full water voxel from 3.333 to 0.667 studs below its
    // center; a half-full one 2.667 to 1.333 below.
    for (occupancy, top, bottom) in [(255, -0.667, -3.333), (127, -1.333, -2.667)] {
        let mut grid = Grid::new();
        grid.set(
            IVec3::new(3, 3, 3),
            Voxel::new(TerrainMaterial::Water, occupancy),
        );
        let tris = triangles(&grid, mesh::DIAGONALS);
        let center = Vec3::splat(14.0);
        let (t, ..) = cast(&tris, center + Vec3::Y * 10.0, Vec3::NEG_Y).unwrap();
        assert!(
            (10.0 - t - top).abs() < 1e-3,
            "{occupancy}: top {}",
            10.0 - t
        );
        let (t, ..) = cast(&tris, center - Vec3::Y * 10.0, Vec3::Y).unwrap();
        assert!(
            (t - 10.0 - bottom).abs() < 1e-3,
            "{occupancy}: bottom {}",
            t - 10.0
        );
    }
}

/// Where a Grass slab meets a Rock one, the triangles whose corners take
/// different layers are drawn once per layer they hold, the first corner's
/// opaque: every triangle is drawn opaque once, the materials' seams lie
/// along their boundary, and each seam vertex names its triangle's
/// corners.
#[test]
fn seams_blend_layers() {
    use TerrainMaterial::{Grass, Rock};
    let mut grid = Grid::new();
    for z in 0..6 {
        for x in 0..8 {
            let m = if x < 4 { Grass } else { Rock };
            grid.set(IVec3::new(x, 0, z), Voxel::new(m, 255));
        }
    }
    let (mut opaque, mut surface, mut overs) = (0, 0, 0);
    for chunk in chunks_to_mesh(&grid) {
        let mesh = mesh_chunk(&grid, chunk);
        surface += mesh.triangles();
        for d in &mesh.draws {
            let tris = d.indices.len() / 3;
            match d.seam {
                None | Some(Seam::First) => opaque += tris,
                Some(Seam::Over) => overs += tris,
            }
            if d.seam.is_none() {
                continue;
            }
            assert_eq!(d.corners.len(), d.positions.len());
            for (c, p) in d.corners.iter().zip(&d.positions) {
                assert!(c.layers.contains(&d.layer));
                let distinct = c.layers.iter().any(|l| *l != c.layers[0]);
                assert!(distinct, "a seam's corners differ");
                // Grass's top and side meet round the slab's edge; the
                // materials only within a cell of their boundary, x = 16
                // (and Rock's shift, under a stud).
                let x = mesh.origin().x + p[0];
                if c.layers.iter().any(|l| l.material != c.layers[0].material) {
                    assert!((x - 16.0).abs() <= 5.0, "seam at x = {x}");
                }
            }
        }
    }
    assert_eq!(opaque, surface);
    assert!(overs > 0);
}

/// A voxel whose occupancy byte is 0 still holds a little of its material
/// (`Voxel::fill`): a cell whose only solid corners are such voxels takes
/// their material, not Air, which has no texture and drew white specks on
/// a grass hillside.
#[test]
fn faint_voxels_keep_their_material() {
    use TerrainMaterial::Grass;
    let mut grid = Grid::new();
    for z in 0..6 {
        for x in 0..6 {
            grid.set(IVec3::new(x, 0, z), Voxel::new(Grass, 255));
        }
    }
    for z in 2..4 {
        for x in 2..4 {
            grid.set(IVec3::new(x, 1, z), Voxel::new(Grass, 0));
        }
    }
    for chunk in chunks_to_mesh(&grid) {
        let mesh = mesh_chunk(&grid, chunk);
        for d in &mesh.draws {
            assert_eq!(d.layer.material, Grass);
            for c in &d.corners {
                assert!(c.layers.iter().all(|l| l.material == Grass));
            }
        }
    }
}
