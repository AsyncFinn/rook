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

/// A FloorWire's path bends when its `To` part moves: its `From` part is
/// drawn again, while `To` itself only moves.
#[test]
fn floor_wire_redraws_when_to_moves() {
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
        b.Position = Vector3.new(10, 5, 0)
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
    assert!(changes.moved.contains(&part("B")));
}

/// A Highlight in a Model covers all its parts: changing it redraws each.
#[test]
fn highlight_changes_redraw_its_models_parts() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Glow",
        r#"
        local m = Instance.new("Model") m.Name = "M" m.Parent = workspace
        for _, name in {"A", "B"} do
            local p = Instance.new("Part") p.Name = name p.Anchored = true p.Parent = m
        end
        local h = Instance.new("Highlight") h.Parent = m
        task.wait(0.5)
        h.FillColor = Color3.new(0, 1, 0)
        "#,
    );
    s.start();
    s.run_for(0.2);
    s.take_render_changes();
    s.run_for(0.5);
    let changes = s.take_render_changes();
    let dom = &s.runtime().world().dom;
    let ws = dom.find_service("Workspace").unwrap();
    let m = dom.find_first_child(ws, "M", false).unwrap();
    for name in ["A", "B"] {
        assert!(
            changes
                .parts
                .contains(&dom.find_first_child(m, name, false).unwrap())
        );
    }
}

/// A SelectionPartLasso draws with its Humanoid's root part: moving
/// either end redraws that part in full, as the bar bends.
#[test]
fn lasso_redraws_its_root_part_as_either_end_moves() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Lasso",
        r#"
        local m = Instance.new("Model") m.Name = "Dummy"
        local root = Instance.new("Part") root.Name = "HumanoidRootPart" root.Anchored = true root.Parent = m
        local h = Instance.new("Humanoid") h.Parent = m
        m.Parent = workspace
        local target = Instance.new("Part") target.Name = "Target" target.Anchored = true target.Parent = workspace
        local lasso = Instance.new("SelectionPartLasso") lasso.Humanoid = h lasso.Part = target lasso.Parent = workspace
        task.wait(0.5)
        target.Position = Vector3.new(10, 0, 0)
        task.wait(0.5)
        root.Position = Vector3.new(0, 10, 0)
        "#,
    );
    s.start();
    s.run_for(0.2);
    s.take_render_changes();
    let root = {
        let dom = &s.runtime().world().dom;
        let ws = dom.find_service("Workspace").unwrap();
        let m = dom.find_first_child(ws, "Dummy", false).unwrap();
        dom.find_first_child(m, "HumanoidRootPart", false).unwrap()
    };
    s.run_for(0.5);
    assert!(s.take_render_changes().parts.contains(&root));
    s.run_for(0.5);
    assert!(s.take_render_changes().parts.contains(&root));
}
