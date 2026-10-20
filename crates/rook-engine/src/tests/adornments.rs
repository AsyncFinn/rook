use super::*;

/// Moving a SelectionBox to another Adornee redraws both parts: the old
/// one loses its box, the new one gains it.
#[test]
fn adornee_changes_redraw_both_parts() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Boxes",
        r#"
        local a = Instance.new("Part") a.Name = "A" a.Anchored = true a.Parent = workspace
        local b = Instance.new("Part") b.Name = "B" b.Anchored = true b.Parent = workspace
        local box = Instance.new("SelectionBox") box.Adornee = a box.Parent = workspace
        task.wait(0.5)
        box.Adornee = b
        "#,
    );
    s.start();
    s.run_for(0.2);
    s.take_render_changes();
    s.run_for(0.5);
    let changes = s.take_render_changes();
    let dom = &s.runtime().world().dom;
    let ws = dom.find_service("Workspace").unwrap();
    let part = |name| dom.find_first_child(ws, name, false).unwrap();
    assert!(changes.parts.contains(&part("A")));
    assert!(changes.parts.contains(&part("B")));
}
