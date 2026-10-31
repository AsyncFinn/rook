//! Destroyed instances leave the arena once nothing holds them, with every
//! reader done with them first: debug builds fail any read of a freed one.

use super::*;

/// Parts a script destroyed are freed within a few frames, and every
/// system that saw them (physics, rendering, GUI, sounds) runs on without
/// reading them again; one a script still holds stays readable until let go.
#[test]
fn destroyed_parts_are_freed_once_nothing_holds_them() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Script",
        r#"
        task.wait(0.2)
        for i = 1, 20 do
            local p = Instance.new("Part")
            p.Parent = workspace
            p:Destroy()
        end
        local held = Instance.new("Part")
        held.Name = "Held"
        held.Parent = workspace
        task.wait(0.3)
        held:Destroy()
        _G.held = held
        "#,
    );
    s.start();
    s.run_for(0.1);
    let before = s.runtime.world().dom.len();
    s.run_for(0.15);
    let held = {
        let w = s.runtime.world();
        let ws = w.dom.find_service("Workspace").unwrap();
        w.dom.find_first_child(ws, "Held", false).unwrap()
    };
    s.run_for(0.35);
    let collect = |s: &mut Server| {
        s.runtime.lua().gc_collect().unwrap();
        s.run_for(0.1);
    };
    collect(&mut s);
    assert_eq!(s.runtime.world().dom.len(), before + 1);
    assert!(s.runtime.world().dom.is_destroyed(held));

    s.runtime
        .run_source("Release", "_G.held = nil", None)
        .unwrap();
    collect(&mut s);
    assert_eq!(s.runtime.world().dom.len(), before);
}

/// What `GetChildren` keeps for a parent never holds a child it lost: a
/// part destroyed after its folder (still held) was listed, and never
/// listed again, is freed like any other.
#[test]
fn kept_children_do_not_hold_what_left() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Script",
        r#"
        local folder = Instance.new("Folder")
        folder.Parent = workspace
        _G.folder = folder
        task.wait(0.2)
        Instance.new("Part").Parent = folder
        local children = folder:GetChildren()
        print(#children)
        children[1]:Destroy()
        children = nil
        "#,
    );
    s.start();
    s.run_for(0.1);
    let before = s.runtime.world().dom.len();
    s.run_for(0.3);
    for _ in 0..2 {
        s.runtime.lua().gc_collect().unwrap();
        s.run_for(0.1);
    }
    assert_eq!(lines(&s), ["1"]);
    assert_eq!(s.runtime.world().dom.len(), before);
}
