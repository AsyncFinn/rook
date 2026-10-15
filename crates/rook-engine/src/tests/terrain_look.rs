//! Scripts changing how the terrain looks reach the renderer.

use super::*;

/// The Terrain's water properties and `Workspace.GlobalWind` mark the
/// terrain's look as changed; other properties of Workspace don't.
#[test]
fn terrain_and_wind_changes_mark_the_terrain() {
    let mut s = Server::new(&Options::default()).unwrap();
    let ws = service(&s, "Workspace");
    add_under(&s, ws, "Terrain", "Terrain", "");
    s.start();
    s.take_render_changes();
    let terrain = |s: &mut Server, source: &str| {
        s.run_extra_script(source).unwrap();
        let changes = s.take_render_changes();
        assert!(!changes.full, "{source}");
        changes.terrain
    };
    for source in [
        "workspace.Terrain.WaterWaveSize = 0.5",
        "workspace.Terrain.WaterWaveSpeed = 30",
        "workspace.Terrain.WaterColor = Color3.new(1, 0, 0)",
        "workspace.Terrain.WaterTransparency = 0.8",
        "workspace.GlobalWind = Vector3.new(10, 0, 0)",
    ] {
        assert!(terrain(&mut s, source), "{source}");
    }
    assert!(!terrain(&mut s, "workspace.Gravity = 100"));
}
