//! A DataModel no renderer draws (a server with clients, a headless one)
//! keeps no render changes, and one drawn later starts from the whole tree.

use crate::tests::*;

/// Parts, lights and effects coming and going each frame, as a busy
/// server's scripts make them.
const CHURN: &str = "
    game:GetService('RunService').Heartbeat:Connect(function()
        local part = Instance.new('Part')
        Instance.new('PointLight').Parent = part
        Instance.new('ParticleEmitter').Parent = part
        part.Parent = workspace
        task.defer(part.Destroy, part)
    end)
";

#[test]
fn an_undrawn_server_keeps_no_render_changes() {
    let mut s = Server::new(&Options::default()).unwrap();
    s.start();
    s.run_extra_script(CHURN).unwrap();
    for _ in 0..30 {
        s.step();
    }
    let kept = &s.render_changes;
    assert!(kept.placed.is_empty(), "{} placed", kept.placed.len());
    assert!(kept.lights_placed.is_empty() && kept.effects_placed.is_empty());
    assert!(
        kept.collected.is_empty(),
        "{} collected",
        kept.collected.len()
    );

    // The first renderer to ask draws everything, then what changed.
    assert!(s.take_render_changes().full);
    s.step();
    let changes = s.take_render_changes();
    assert!(!changes.full && !changes.placed.is_empty());
}
