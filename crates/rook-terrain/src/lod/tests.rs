use glam::Vec2;

use super::*;
use crate::random::Random;
use crate::{Sides, TerrainMaterial, Voxel, mesh_chunk_at, mesh_drawn_chunk};

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
    // Past a level-0 chunk and the slack: the finest box moves, the
    // coarser ones stay.
    let before = rings;
    assert!(rings.follow(Vec3::new(280.0, 10.0, 10.0)));
    assert_ne!(rings.starts[0], before.starts[0]);
    assert_eq!(rings.starts[1..], before.starts[1..]);
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
fn drawn_chunks_stay_in_their_boxes() {
    // Cut to their boxes, neighbors meet on the planes between them and
    // never overlap.
    let levels = slab(3);
    for level in 0..LEVELS {
        let mesh = mesh_drawn_chunk(&levels, level, IVec3::new(0, -1, 0), ALL);
        let side = chunk_studs(level);
        for p in mesh.draws.iter().flat_map(|d| &d.positions) {
            for a in [0, 2] {
                assert!((0.0..=side).contains(&p[a]), "level {level}: {p:?}");
            }
        }
    }
}

/// Every side closed against the next ring.
const ALL: Sides = Sides::NONE.with(0).with(1).with(2).with(3);

/// The highest and lowest drawn point of the chunks of `level` in the
/// column of chunk `column` (its row and the one above, where the surface
/// lying on the plane between them is drawn), in world studs, their sides
/// `seams` closed against the next level.
fn drawn_heights(levels: &[Grid], level: u8, column: IVec3, seams: Sides) -> (f32, f32) {
    let mut out = (f32::MIN, f32::MAX);
    for chunk in [column, column + IVec3::Y] {
        let mesh = mesh_drawn_chunk(levels, level, chunk, seams);
        let origin = mesh.origin();
        for p in mesh.draws.iter().flat_map(|d| d.positions.iter()) {
            out = (out.0.max(p[1] + origin.y), out.1.min(p[1] + origin.y));
        }
    }
    out
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
        let (top, _) = drawn_heights(&levels, level, IVec3::new(0, -1, 0), Sides::NONE);
        assert!((top - 2.0).abs() < 1e-3, "level {level}: top at {top}");
    }
}

/// Closing a seam reaches no higher or lower than the two rings'
/// surfaces: skirts under a slab thinner than a coarse voxel stood up
/// through it as walls and shards, and at their full depth hung out under
/// thin ground as curtains.
#[test]
fn seams_stay_between_the_two_rings() {
    for depth in [1, 6] {
        let levels = slab(depth);
        let column = IVec3::new(0, -1, 0);
        for level in 0..LEVELS - 1 {
            let (top, bottom) = drawn_heights(&levels, level, column, ALL);
            let own = drawn_heights(&levels, level, column, Sides::NONE);
            let next = drawn_heights(
                &levels,
                level + 1,
                column.div_euclid(IVec3::splat(2)),
                Sides::NONE,
            );
            assert!(
                top <= own.0.max(next.0) + 1e-3,
                "depth {depth}, level {level}: reaches {top}"
            );
            assert!(
                bottom >= own.1.min(next.1) - 1e-3,
                "depth {depth}, level {level}: to {bottom}"
            );
        }
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
        let (top, _) = drawn_heights(&levels, level, IVec3::ZERO, Sides::NONE);
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
    let (top, _) = drawn_heights(&pyramid(grid), 3, IVec3::ZERO, Sides::NONE);
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
                assert!(
                    !levels_at(&rings, p).contains(&k),
                    "level {k} at {p} from {camera}"
                );
            }
        }
    }
}

#[test]
fn slivers_on_the_ground_stay_slivers_far_away() {
    // Thick ground up to 0 with a layer a tenth full on it: its surface is
    // at 2.4 near (the layer's 0.4 studs, half a 4-stud voxel past). A
    // coarse voxel holding only the layer reached its center (4 in 8-stud
    // cells, 8 in 16, ...), so far rings raised the ground and slivers in
    // a tunnel closed it.
    let mut grid = Grid::new();
    for x in -384..384 {
        for z in -384..384 {
            for y in -40..0 {
                grid.set(IVec3::new(x, y, z), Voxel::new(TerrainMaterial::Snow, 255));
            }
            grid.set(IVec3::new(x, 0, z), Voxel::new(TerrainMaterial::Snow, 24));
        }
    }
    let levels = pyramid(grid);
    for level in 0..LEVELS {
        let (top, _) = drawn_heights(&levels, level, IVec3::ZERO, Sides::NONE);
        // Within a step of the coarse occupancy byte (1/256 of the voxel).
        let step = Detail::at(level).voxel_studs() / 256.0;
        assert!(
            (top - 2.4).abs() < step + 0.05,
            "level {level}: top at {top}"
        );
    }
}

#[test]
fn no_ring_is_ever_skipped() {
    // Walking out from the camera, the level rises one at a time: Studio
    // never skips one. The 4-wide squares let a level sit against the
    // edge of the one around it, so from a hallway 260 studs out the
    // 4-stud ring met the 16-stud one and closed the hallway.
    let mut r = Random::at(IVec3::new(7, 8, 9));
    let mut next = |span: f32| (r.next() * 2.0 - 1.0) * span;
    let mut rings = Rings::around(Vec3::ZERO);
    let mut camera = Vec3::ZERO;
    for _ in 0..1000 {
        camera += Vec3::new(next(300.0), 0.0, next(300.0));
        rings.follow(camera);
        let angle = next(std::f32::consts::PI);
        let way = Vec3::new(angle.cos(), 0.0, angle.sin());
        let mut level = 0;
        for step in 0..2000 {
            let now = levels_at(&rings, camera + way * (step as f32 * 8.0))[0];
            assert!(
                now == level || now == level + 1,
                "{level} to {now} from {camera}"
            );
            level = now;
        }
    }
}

/// After edits, every level averaged again over what they reached matches
/// the pyramid built from scratch, stored alike: a crater, grass added in
/// open air, and ground cleared from under a whole coarse chunk.
#[test]
fn update_pyramid_matches_a_fresh_pyramid() {
    let mut grid = Grid::new();
    for x in -40..90 {
        for z in -40..90 {
            let mut random = Random::at(IVec3::new(x, 0, z));
            let top = 10 + (random.next() * 6.0) as i32;
            for y in -8..top {
                let material = if y < 0 {
                    TerrainMaterial::Rock
                } else {
                    TerrainMaterial::Grass
                };
                let occupancy = if y == top - 1 {
                    60 + (random.next() * 195.0) as u8
                } else {
                    255
                };
                grid.set(IVec3::new(x, y, z), Voxel::new(material, occupancy));
            }
        }
    }
    let mut levels = pyramid(grid.clone());
    let mut changed = Vec::new();
    let mut edit = |at: IVec3, r: i32, voxel: Voxel| {
        for v in (Bounds {
            min: at - r,
            max: at + r,
        })
        .voxels()
        {
            if (v - at).length_squared() <= r * r {
                grid.set(v, voxel);
            }
        }
        changed.push(Bounds {
            min: at - r,
            max: at + r,
        });
    };
    edit(IVec3::new(31, 12, 33), 5, Voxel::AIR);
    edit(
        IVec3::new(60, 40, -10),
        3,
        Voxel::new(TerrainMaterial::Grass, 200),
    );
    // All the ground under the coarse chunk at -1, -1, -1.
    let (min, max) = (IVec3::new(-40, -8, -40), IVec3::splat(-1));
    for v in (Bounds { min, max }).voxels() {
        grid.set(v, Voxel::AIR);
    }
    changed.push(Bounds { min, max });
    levels[0] = grid.clone();
    update_pyramid(&mut levels, &changed);
    for (k, (updated, fresh)) in levels.iter().zip(pyramid(grid)).enumerate() {
        assert!(updated.stored() == fresh.stored(), "level {k}");
        // Stored alike, too: one voxel for a chunk all one.
        for c in fresh.chunks() {
            assert_eq!(updated.uniform(c), fresh.uniform(c), "level {k} chunk {c}");
        }
    }
}

/// Steep hills crossing the seam of the 16- and 32-stud rings, seen
/// from a hilltop 1,400 studs away: every ray that reaches the ground
/// meets its front, none the back of the far side through a crack. With
/// skirts one voxel deep, rays went through where the slope turned the
/// two rings' edges, apart across the seam, into a gap taller than that.
#[test]
fn steep_hills_show_no_sky_through_ring_seams() {
    let height = |x: i32, z: i32| {
        let (x, z) = (x as f32, z as f32);
        let cliff = if (x / 60.0).sin() > 0.6 { 9.0 } else { 0.0 };
        12.0 + 7.0 * (x / 23.0).sin() * (z / 31.0).cos()
            + 3.0 * (z / 9.0 + x / 13.0).sin()
            + cliff
            + 10.0 * (x / 97.0 + 1.0).sin() * (z / 83.0).sin()
    };
    use TerrainMaterial as M;
    let materials = [
        M::Grass,
        M::Rock,
        M::Slate,
        M::Sand,
        M::Ground,
        M::Mud,
        M::Snow,
        M::Basalt,
    ];
    let mut grid = Grid::new();
    for x in 0..192 {
        for z in 64..400 {
            let h = height(x, z);
            let material =
                materials[(x.div_euclid(48) + 3 * z.div_euclid(48)).rem_euclid(8) as usize];
            for y in -4..36 {
                let fill = (h - y as f32).clamp(0.0, 1.0);
                if fill > 0.0 {
                    let occupancy = ((fill * 255.0).round() as u8).max(1);
                    grid.set(IVec3::new(x, y, z), Voxel::new(material, occupancy));
                }
            }
        }
    }
    let levels = pyramid(grid);
    let camera = Vec3::new(0.0, 190.0, 0.0);
    let rings = Rings::around(camera);
    let triangles = drawn_triangles(&levels, &rings, (-1, 1));
    let cast = |d: Vec3| cast(&triangles, camera, d);
    // The view the cracks showed in: 70 degrees high, 16:9, towards
    // (500, 60, 500), the part of it over the hills.
    let forward = (Vec3::new(500.0, 60.0, 500.0) - camera).normalize();
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward);
    let tan = 35f32.to_radians().tan();
    let mut through = Vec::new();
    for y in (290..400).step_by(2) {
        for x in (780..900).step_by(2) {
            let sx = ((x as f32 + 0.5) / 640.0 - 1.0) * tan * 16.0 / 9.0;
            let sy = (1.0 - (y as f32 + 0.5) / 360.0) * tan;
            let d = (forward + right * sx + up * sy).normalize();
            if let Some((t, false)) = cast(d) {
                through.push(camera + d * t);
            }
        }
    }
    assert!(
        through.is_empty(),
        "{} rays saw the back of the ground, first at {}",
        through.len(),
        through[0]
    );
}

/// Every triangle the rings draw of `levels`, in world studs, `rows` the
/// lowest and highest level-0 chunk rows holding voxels.
fn drawn_triangles(levels: &[Grid], rings: &Rings, rows: (i32, i32)) -> Vec<[Vec3; 3]> {
    let mut triangles = Vec::new();
    for level in 0..LEVELS {
        let grid = &levels[usize::from(level)];
        for c in rings.chunks(level, grid, rows) {
            let mesh = mesh_drawn_chunk(levels, level, c, rings.seams(level, c));
            let origin = mesh.origin();
            for d in &mesh.draws {
                for t in d.indices.chunks(3) {
                    triangles
                        .push([0, 1, 2].map(|k| Vec3::from(d.positions[t[k] as usize]) + origin));
                }
            }
        }
    }
    triangles
}

/// The nearest of `triangles` along the ray from `from` along `d`, and
/// whether it faces the ray.
fn cast(triangles: &[[Vec3; 3]], from: Vec3, d: Vec3) -> Option<(f32, bool)> {
    let mut best: Option<(f32, bool)> = None;
    for p in triangles {
        let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
        let pv = d.cross(e2);
        let det = e1.dot(pv);
        if det.abs() < 1e-9 {
            continue;
        }
        let tv = from - p[0];
        let u = tv.dot(pv) / det;
        let q = tv.cross(e1);
        let v = d.dot(q) / det;
        let t = e2.dot(q) / det;
        if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t > 0.0 && best.is_none_or(|b| t < b.0) {
            best = Some((t, e1.cross(e2).dot(d) < 0.0));
        }
    }
    best
}

/// A cave: a floor and a thin ceiling of Rock, both rolling, walled in
/// 1,600 studs square.
fn cave() -> Vec<Grid> {
    let mut grid = Grid::new();
    for x in -200i32..200 {
        for z in -200i32..200 {
            let (fx, fz) = (x as f32 * 4.0, z as f32 * 4.0);
            let roll = 6.0 * (fx / 37.0).sin() * (fz / 23.0).cos();
            // Walled in, so every ray from inside meets rock.
            let wall = x.abs() >= 196 || z.abs() >= 196;
            let layers: &[(f32, f32)] = if wall {
                &[(-14.0 + roll, 46.0 - roll)]
            } else {
                &[(-14.0 + roll, -4.0 + roll), (40.0 - roll, 46.0 - roll)]
            };
            for &(bottom, top) in layers {
                for y in (bottom / 4.0).floor() as i32 - 1..=(top / 4.0).ceil() as i32 {
                    let center = y as f32 * 4.0 + 2.0;
                    let depth = (center - bottom).min(top - center);
                    let fill = (depth / 4.0 + 0.5).clamp(0.0, 1.0);
                    if fill > 0.0 {
                        let occupancy = ((fill * 255.0).round() as u8).max(1);
                        grid.set(
                            IVec3::new(x, y, z),
                            Voxel::new(TerrainMaterial::Rock, occupancy),
                        );
                    }
                }
            }
        }
    }
    pyramid(grid)
}

/// Seen from inside a cave, aimed along where rings meet on its ceiling
/// and floor, every ray meets the cave's inside: none goes through a
/// crack to the rock's far side or the sky. Skirts closed only seams in
/// ground facing up, and a ceiling's seam let the sky through in a band.
#[test]
fn caves_show_no_sky_through_ring_seams() {
    let camera = Vec3::new(30.0, 18.0, -50.0);
    let rings = Rings::around(camera);
    let levels = cave();
    let triangles = Triangles::new(drawn_triangles(&levels, &rings, (-2, 1)));
    let mut through = Vec::new();
    let mut aimed = 0;
    for k in 0..2 {
        let side = chunk_studs(k as u8);
        let lo = rings.starts[k].as_vec2() * side;
        let hi = lo + width(k) as f32 * side;
        for edge in 0..4 {
            for n in 0..16 {
                let along = lo.x.max(-700.0)
                    + (hi.x.min(700.0) - lo.x.max(-700.0)) * (n as f32 + 0.5) / 16.0;
                let along_z = lo.y.max(-700.0)
                    + (hi.y.min(700.0) - lo.y.max(-700.0)) * (n as f32 + 0.5) / 16.0;
                let seam = match edge {
                    0 => Vec2::new(lo.x, along_z),
                    1 => Vec2::new(hi.x, along_z),
                    2 => Vec2::new(along, lo.y),
                    _ => Vec2::new(along, hi.y),
                };
                if seam.abs().max_element() > 700.0 {
                    continue;
                }
                let across = if edge < 2 { Vec2::X } else { Vec2::Y };
                let heights = (0..24)
                    .map(|i| -18.0 + i as f32)
                    .chain((0..18).map(|i| 31.0 + i as f32));
                for y in heights {
                    for off in [-0.5, 0.5] {
                        let p = seam + across * off;
                        let d = (Vec3::new(p.x, y, p.y) - camera).normalize();
                        aimed += 1;
                        if !matches!(triangles.cast(camera, d), Some((_, true))) {
                            through.push(Vec3::new(p.x, y, p.y));
                        }
                    }
                }
            }
        }
    }
    assert!(aimed > 1000);
    assert!(
        through.is_empty(),
        "{} of {aimed} rays went through, first toward {}",
        through.len(),
        through[0]
    );
}

/// Triangles bucketed by 64-stud columns, for casting many rays.
struct Triangles {
    all: Vec<[Vec3; 3]>,
    columns: rustc_hash::FxHashMap<glam::IVec2, Vec<usize>>,
}

impl Triangles {
    const COLUMN: f32 = 64.0;

    fn new(all: Vec<[Vec3; 3]>) -> Self {
        let mut columns: rustc_hash::FxHashMap<glam::IVec2, Vec<usize>> = Default::default();
        for (i, t) in all.iter().enumerate() {
            let lo = t.iter().fold(Vec3::MAX, |a, p| a.min(*p));
            let hi = t.iter().fold(Vec3::MIN, |a, p| a.max(*p));
            let (lo, hi) = (
                (lo / Self::COLUMN).floor().as_ivec3(),
                (hi / Self::COLUMN).floor().as_ivec3(),
            );
            for x in lo.x..=hi.x {
                for z in lo.z..=hi.z {
                    columns.entry(glam::IVec2::new(x, z)).or_default().push(i);
                }
            }
        }
        Triangles { all, columns }
    }

    /// [`cast`] over the triangles in the columns the ray crosses, to
    /// 2,000 studs.
    fn cast(&self, from: Vec3, d: Vec3) -> Option<(f32, bool)> {
        let mut seen = rustc_hash::FxHashSet::default();
        let mut near = Vec::new();
        for step in 0..2000 {
            let p = from + d * step as f32;
            let c = (glam::Vec2::new(p.x, p.z) / Self::COLUMN)
                .floor()
                .as_ivec2();
            for dx in -1..=1 {
                for dz in -1..=1 {
                    let c = c + glam::IVec2::new(dx, dz);
                    if seen.insert(c)
                        && let Some(list) = self.columns.get(&c)
                    {
                        near.extend(list.iter().map(|&i| self.all[i]));
                    }
                }
            }
        }
        cast(&near, from, d)
    }
}
