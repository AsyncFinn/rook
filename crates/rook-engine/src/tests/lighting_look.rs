//! Scripts changing how the sky and lighting look reach the renderer.

use super::*;

/// Lighting's properties and its children's (a Sky arriving, changing or
/// leaving) mark the lighting as changed; instances elsewhere don't.
#[test]
fn sky_changes_mark_the_lighting() {
    let mut s = Server::new(&Options::default()).unwrap();
    s.start();
    s.take_render_changes();
    let lighting = |s: &mut Server, source: &str| {
        s.run_extra_script(source).unwrap();
        let changes = s.take_render_changes();
        assert!(!changes.full, "{source}");
        changes.lighting
    };
    for source in [
        "game.Lighting.FogEnd = 500",
        "Instance.new('Sky').Parent = game.Lighting",
        "game.Lighting.Sky.SkyboxUp = 'rbxassetid://1'",
        "game.Lighting.Sky.Parent = nil",
    ] {
        assert!(lighting(&mut s, source), "{source}");
    }
    assert!(!lighting(
        &mut s,
        "Instance.new('Sky').Parent = game.ReplicatedStorage"
    ));
}
