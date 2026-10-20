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

/// Changing any adornment kind (here a BoxHandleAdornment) redraws its
/// Adornee.
#[test]
fn handle_adornment_changes_redraw_their_part() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Handles",
        r#"
        local a = Instance.new("Part") a.Name = "A" a.Anchored = true a.Parent = workspace
        local box = Instance.new("BoxHandleAdornment") box.Adornee = a box.Parent = workspace
        task.wait(0.5)
        box.Color3 = Color3.new(1, 0, 0)
        "#,
    );
    s.start();
    s.run_for(0.2);
    s.take_render_changes();
    s.run_for(0.5);
    let changes = s.take_render_changes();
    let dom = &s.runtime().world().dom;
    let ws = dom.find_service("Workspace").unwrap();
    assert!(
        changes
            .parts
            .contains(&dom.find_first_child(ws, "A", false).unwrap())
    );
}

/// A FloorWire draws with its `From` part: changing it redraws that part.
#[test]
fn floor_wire_changes_redraw_its_from_part() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Wire",
        r#"
        local a = Instance.new("Part") a.Name = "A" a.Anchored = true a.Parent = workspace
        local b = Instance.new("Part") b.Name = "B" b.Anchored = true b.Parent = workspace
        local wire = Instance.new("FloorWire") wire.From = a wire.To = b wire.Parent = workspace
        task.wait(0.5)
        wire.Color3 = Color3.new(1, 0, 0)
        "#,
    );
    s.start();
    s.run_for(0.2);
    s.take_render_changes();
    s.run_for(0.5);
    let changes = s.take_render_changes();
    let dom = &s.runtime().world().dom;
    let ws = dom.find_service("Workspace").unwrap();
    assert!(
        changes
            .parts
            .contains(&dom.find_first_child(ws, "A", false).unwrap())
    );
}
