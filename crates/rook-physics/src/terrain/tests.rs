//! Terrain collision and queries. Expected values were measured in Studio
//! 0.742 on the same voxels (`docs/behavior/terrain.md`).

use glam::{IVec3, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_terrain::{Grid, TerrainMaterial, Voxel};
use rook_types::{BinaryString, CFrame, Material, Value, Vector3};

use crate::query::{CastShape, Filter, Region};
use crate::{Physics, Touch};

/// Fill voxels `from` to `to` (inclusive) with full voxels of `material`.
fn fill(grid: &mut Grid, from: IVec3, to: IVec3, material: TerrainMaterial) {
    for z in from.z..=to.z {
        for y in from.y..=to.y {
            for x in from.x..=to.x {
                grid.set(IVec3::new(x, y, z), Voxel::new(material, 255));
            }
        }
    }
}

/// Studio's test terrain, moved to the origin: a lone Grass voxel centred
/// at (2, 42, 2), a 64 × 4 × 64 Grass slab centred at (102, 42, 2), the
/// same in Brick at x = 202, and 64 × 16 × 64 of Water at x = 302 (y 40
/// to 56).
fn world() -> (Dom, InstanceId) {
    let mut grid = Grid::new();
    fill(
        &mut grid,
        IVec3::new(0, 10, 0),
        IVec3::new(0, 10, 0),
        TerrainMaterial::Grass,
    );
    for (x, material) in [(25, TerrainMaterial::Grass), (50, TerrainMaterial::Brick)] {
        fill(
            &mut grid,
            IVec3::new(x - 8, 10, -8),
            IVec3::new(x + 7, 10, 7),
            material,
        );
    }
    fill(
        &mut grid,
        IVec3::new(67, 10, -8),
        IVec3::new(82, 13, 7),
        TerrainMaterial::Water,
    );
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain = dom.create_by_name("Terrain").unwrap();
    let bytes = Value::BinaryString(BinaryString::from(grid.encode()));
    dom.set_property_by_name(terrain, "SmoothGrid", bytes)
        .unwrap();
    dom.set_parent(terrain, Some(ws)).unwrap();
    (dom, terrain)
}

fn part(dom: &mut Dom, class: &str, at: Vec3, size: Vec3, anchored: bool) -> InstanceId {
    let ws = dom.get_service("Workspace").unwrap();
    let id = dom.create_by_name(class).unwrap();
    let set = |dom: &mut Dom, name, v| dom.set_property_by_name(id, name, v).unwrap();
    set(dom, "CFrame", Value::CFrame(CFrame::from_position(at)));
    set(dom, "Size", Value::Vector3(Vector3(size)));
    set(dom, "Anchored", Value::Bool(anchored));
    dom.set_parent(id, Some(ws)).unwrap();
    id
}

fn position(dom: &Dom, id: InstanceId) -> Vec3 {
    dom.property_by_name(id, "CFrame")
        .and_then(|v| v.as_cframe())
        .unwrap()
        .position
}

#[test]
fn parts_come_to_rest_on_the_smooth_surface() {
    let (mut dom, terrain) = world();
    // Studio: 43.831 (the lone voxel's top is 43.333, not its box's 44),
    // 46.495 on the Grass slab and 44.495 on the Brick one (cubified).
    let ball = part(
        &mut dom,
        "Part",
        Vec3::new(2.0, 50.0, 2.0),
        Vec3::ONE,
        false,
    );
    dom.set_property_by_name(ball, "Shape", Value::Enum(0))
        .unwrap();
    let grass = part(
        &mut dom,
        "Part",
        Vec3::new(102.0, 52.0, 2.0),
        Vec3::new(2.0, 1.0, 2.0),
        false,
    );
    let brick = part(
        &mut dom,
        "Part",
        Vec3::new(202.0, 52.0, 2.0),
        Vec3::new(2.0, 1.0, 2.0),
        false,
    );
    let mut physics = Physics::new();
    let mut touches: Vec<Touch> = Vec::new();
    for _ in 0..180 {
        physics.sync(&mut dom);
        physics.step(1.0 / 60.0);
        physics.write_back(&mut dom);
        touches.extend(physics.take_touches());
    }
    for (id, y) in [(ball, 43.833), (grass, 46.5), (brick, 44.5)] {
        let at = position(&dom, id);
        assert!((at.y - y).abs() < 0.02, "{at} against {y}");
    }
    // Touched fires with Terrain, as in Studio.
    assert!(touches.iter().any(|t| t.began
        && t.part.min(t.other) == terrain.min(grass)
        && t.part.max(t.other) == terrain.max(grass)));
}

#[test]
fn rays_hit_the_surface_from_outside_with_its_material() {
    let (mut dom, terrain) = world();
    let mut physics = Physics::new();
    let all = Filter::legacy_ray(Vec::new(), false);
    let mut ray = |dom: &mut Dom, from: Vec3, dir: Vec3, filter: &Filter| {
        physics
            .raycast(dom, from, dir, filter)
            .map(|h| (h.part, h.position.y, h.normal.y, h.material))
    };
    let down = Vec3::new(0.0, -40.0, 0.0);
    let grass = Material::Grass as u32;
    let lone = ray(&mut dom, Vec3::new(2.0, 60.0, 2.0), down, &all).unwrap();
    assert_eq!((lone.0, lone.3), (terrain, grass));
    assert!((lone.1 - 43.333).abs() < 1e-3, "{lone:?}");
    assert_eq!(
        ray(&mut dom, Vec3::new(102.0, 60.0, 2.0), down, &all),
        Some((terrain, 46.0, 1.0, grass))
    );
    let brick = ray(&mut dom, Vec3::new(202.0, 60.0, 2.0), down, &all).unwrap();
    assert_eq!((brick.1, brick.3), (44.0, Material::Brick as u32));
    // From below; from inside, nothing (Studio).
    assert_eq!(
        ray(&mut dom, Vec3::new(102.0, 20.0, 2.0), -down, &all),
        Some((terrain, 38.0, -1.0, grass))
    );
    assert_eq!(
        ray(&mut dom, Vec3::new(102.0, 42.0, 2.0), -down, &all),
        None
    );
    assert_eq!(ray(&mut dom, Vec3::new(102.0, 43.0, 2.0), down, &all), None);
    // Water, unless ignored.
    let deep = Vec3::new(0.0, -60.0, 0.0);
    let water = ray(&mut dom, Vec3::new(302.0, 80.0, 2.0), deep, &all).unwrap();
    assert_eq!((water.1, water.3), (56.0, Material::Water as u32));
    let dry = Filter {
        ignore_water: true,
        ..all.clone()
    };
    assert_eq!(ray(&mut dom, Vec3::new(302.0, 80.0, 2.0), deep, &dry), None);
    // Ignored like a part.
    let ignore = Filter::legacy_ray(vec![terrain], false);
    assert_eq!(
        ray(&mut dom, Vec3::new(102.0, 60.0, 2.0), down, &ignore),
        None
    );
    // A sphere of radius 1 stops 13 studs down, touching the slab.
    let cast = physics
        .shapecast(
            &mut dom,
            CastShape::Sphere(1.0),
            CFrame::from_position(Vec3::new(102.0, 60.0, 2.0)),
            down,
            &all,
        )
        .unwrap();
    assert_eq!((cast.part, cast.material), (terrain, grass));
    assert!((cast.distance - 13.0).abs() < 1e-3, "{cast:?}");
}

#[test]
fn bounds_queries_skip_terrain_and_touching_parts_find_it() {
    let (mut dom, terrain) = world();
    let mut physics = Physics::new();
    let region = Region::Box {
        cframe: CFrame::from_position(Vec3::new(102.0, 42.0, 2.0)),
        size: Vec3::splat(4.0),
    };
    let none = Filter::legacy_region(Vec::new(), false);
    assert!(
        physics
            .parts_in_bounds(&mut dom, region, &none, 0)
            .is_empty()
    );
    let probe = part(
        &mut dom,
        "Part",
        Vec3::new(102.0, 44.0, 2.0),
        Vec3::splat(4.0),
        true,
    );
    assert!(physics.parts_in_part(&mut dom, probe, &none, 0).is_empty());
    assert_eq!(
        physics.touching_parts(&mut dom, probe, false),
        vec![terrain]
    );
    // Off the surface, or in water: nothing.
    dom.set_property_by_name(
        probe,
        "CFrame",
        Value::CFrame(CFrame::from_position(Vec3::new(102.0, 48.5, 2.0))),
    )
    .unwrap();
    assert!(physics.touching_parts(&mut dom, probe, false).is_empty());
    dom.set_property_by_name(
        probe,
        "CFrame",
        Value::CFrame(CFrame::from_position(Vec3::new(302.0, 48.0, 2.0))),
    )
    .unwrap();
    assert!(physics.touching_parts(&mut dom, probe, false).is_empty());
    // IsRegion3Empty tests voxels: the air voxel above the slab is empty
    // though the surface reaches into it, a corner of the lone voxel
    // isn't though the surface doesn't, nor is water.
    let mut voxels = |min: Vec3, max: Vec3| physics.terrain_voxels_in(&mut dom, min, max);
    assert!(!voxels(
        Vec3::new(100.0, 44.0, 0.0),
        Vec3::new(104.0, 48.0, 4.0)
    ));
    assert!(voxels(
        Vec3::new(100.0, 43.99, 0.0),
        Vec3::new(104.0, 47.0, 4.0)
    ));
    assert!(voxels(Vec3::new(3.5, 43.5, 3.5), Vec3::new(6.0, 46.0, 6.0)));
    assert!(voxels(
        Vec3::new(300.0, 44.0, 0.0),
        Vec3::new(304.0, 48.0, 4.0)
    ));
}

/// New voxels in `SmoothGrid` (a script's edit replicated) rebuild only
/// the chunks that changed: the Brick slab, a chunk away, keeps its
/// colliders, and rays find the lone voxel gone.
#[test]
fn smooth_grid_changes_rebuild_only_changed_chunks() {
    let (mut dom, terrain) = world();
    let mut physics = Physics::new();
    let all = Filter::legacy_ray(Vec::new(), false);
    let down = Vec3::new(0.0, -40.0, 0.0);
    let slab = Vec3::new(202.0, 60.0, 2.0);
    assert!(physics.raycast(&mut dom, slab, down, &all).is_some());
    assert!(
        physics
            .raycast(&mut dom, Vec3::new(2.0, 60.0, 2.0), down, &all)
            .is_some()
    );
    let built = physics.terrain.pieces.len();
    let mut grid = physics.terrain.grid.clone();
    grid.set(IVec3::new(0, 10, 0), Voxel::AIR);
    let bytes = Value::BinaryString(BinaryString::from(grid.encode()));
    dom.set_property_by_name(terrain, "SmoothGrid", bytes)
        .unwrap();
    physics.sync(&mut dom);
    // Only the lone voxel's chunk went.
    assert!(physics.terrain.pieces.len() < built && !physics.terrain.pieces.is_empty());
    assert_eq!(
        physics.raycast(&mut dom, Vec3::new(2.0, 60.0, 2.0), down, &all),
        None
    );
    assert!(physics.raycast(&mut dom, slab, down, &all).is_some());
}
