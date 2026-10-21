use super::*;

/// A script changing the waves or the wind reaches the scene's terrain
/// look without rebuilding the scene.
#[test]
fn script_changes_update_the_terrain_look() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain = dom.create_by_name("Terrain").unwrap();
    dom.set_parent(terrain, Some(ws)).unwrap();
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, content, glam::Vec3::ZERO);
    assert_eq!(cache.scene.terrain.wave_size, 0.15);
    dom.set_property_by_name(terrain, "WaterWaveSize", Value::Float32(0.6))
        .unwrap();
    dom.set_property_by_name(
        ws,
        "GlobalWind",
        Value::Vector3(rook_types::Vector3::new(5.0, 0.0, 0.0)),
    )
    .unwrap();
    let changes = RenderChanges {
        terrain: true,
        ..RenderChanges::default()
    };
    assert_eq!(
        cache.apply(&dom, &changes),
        SceneChange::Parts {
            changed: Vec::new(),
            moved: Vec::new()
        }
    );
    assert_eq!(cache.scene.terrain.wave_size, 0.6);
    assert_eq!(cache.scene.terrain.wind, Vec3::new(5.0, 0.0, 0.0));
}

/// An edit meshes again only the chunks it changed (and their
/// neighbors); the far chunk keeps its meshes, and the edited chunk's old
/// ones leave the scene.
#[test]
fn edits_remesh_only_changed_chunks() {
    use rook_terrain::Voxel;
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain = dom.create_by_name("Terrain").unwrap();
    dom.set_parent(terrain, Some(ws)).unwrap();
    let mut grid = Grid::new();
    let (near, far) = (IVec3::new(5, 5, 5), IVec3::new(200, 5, 5));
    grid.set(near, Voxel::new(TerrainMaterial::Grass, 255));
    grid.set(far, Voxel::new(TerrainMaterial::Rock, 255));
    let write = |dom: &mut Dom, grid: &Grid| {
        dom.set_property_by_name(
            terrain,
            "SmoothGrid",
            Value::BinaryString(grid.encode().into()),
        )
        .unwrap();
    };
    write(&mut dom, &grid);
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, content, glam::Vec3::ZERO);
    let meshes_of = |cache: &SceneCache, material| -> Vec<MeshHash> {
        cache
            .terrain
            .surfaces
            .iter()
            .filter(|s| s.layer.material == material)
            .map(|s| s.mesh)
            .collect()
    };
    let (rock, grass) = (
        meshes_of(&cache, TerrainMaterial::Rock),
        meshes_of(&cache, TerrainMaterial::Grass),
    );
    assert!(!rock.is_empty() && !grass.is_empty());
    grid.set(near + IVec3::X, Voxel::new(TerrainMaterial::Grass, 255));
    write(&mut dom, &grid);
    let changes = RenderChanges {
        terrain: true,
        ..RenderChanges::default()
    };
    assert_eq!(cache.apply(&dom, &changes), SceneChange::Full);
    assert_eq!(meshes_of(&cache, TerrainMaterial::Rock), rock);
    let regrown = meshes_of(&cache, TerrainMaterial::Grass);
    assert!(!regrown.is_empty() && regrown.iter().all(|m| !grass.contains(m)));
    assert!(grass.iter().all(|m| !cache.scene.meshes.contains_key(m)));
}
