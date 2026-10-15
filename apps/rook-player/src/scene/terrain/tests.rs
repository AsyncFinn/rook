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
    let mut cache = SceneCache::new(&dom, content);
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
    assert_eq!(cache.apply(&dom, &changes), SceneChange::Parts(Vec::new()));
    assert_eq!(cache.scene.terrain.wave_size, 0.6);
    assert_eq!(cache.scene.terrain.wind, Vec3::new(5.0, 0.0, 0.0));
}
