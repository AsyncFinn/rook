//! Network ownership a server script sets: the owner's client simulates
//! the assembly, forces only it applies included, and the server takes
//! where it puts it.

use crate::tests::*;

const SETUP: &str = r#"
    local floor = Instance.new("Part")
    floor.Name = "Floor"
    floor.Anchored = true
    floor.Size = Vector3.new(200, 1, 200)
    floor.Position = Vector3.new(0, -0.5, 0)
    floor.Parent = workspace
    local crate = Instance.new("Part")
    crate.Name = "Crate"
    crate.Size = Vector3.new(2, 2, 2)
    crate.Position = Vector3.new(20, 1, 20)
    crate.Parent = workspace
"#;

fn crate_of(dm: &DataModel) -> InstanceId {
    let w = dm.runtime.world();
    let ws = w.dom.find_service("Workspace").unwrap();
    w.dom.find_first_child(ws, "Crate", false).unwrap()
}

/// A client drags a crate with a BodyPosition of its own, as games'
/// draggers do once the server makes it the crate's owner.
const DRAG: &str = r#"
    local mover = Instance.new("BodyPosition")
    mover.P = 25000
    mover.D = 1500
    mover.MaxForce = Vector3.new(math.huge, math.huge, math.huge)
    mover.Position = Vector3.new(30, 6, 20)
    mover.Parent = workspace.Crate
"#;

#[test]
fn the_owner_simulates_what_it_was_given() {
    let mut s = session_with(SETUP, &["A"], 1.0, &Options::default());
    s.server()
        .run_extra_script(
            r#"
            local crate = workspace.Crate
            print(crate:GetNetworkOwner(), crate:GetNetworkOwnershipAuto())
            crate:SetNetworkOwner(game.Players.A)
            print(crate:GetNetworkOwner(), crate:GetNetworkOwnershipAuto())
            "#,
        )
        .unwrap();
    assert_eq!(lines(s.server()), ["nil true", "A false"]);
    s.run_for(0.2);
    s.client(0).unwrap().run_extra_script(DRAG).unwrap();
    s.run_for(2.0);
    let held = position(s.server(), crate_of(s.server()));
    assert!(
        held.distance(glam::Vec3::new(30.0, 6.0, 20.0)) < 0.5,
        "the server sees the client's drag: {held}"
    );

    // Back to automatic: the server simulates it, without the client's
    // mover, and it falls.
    s.server()
        .run_extra_script("workspace.Crate:SetNetworkOwnershipAuto()")
        .unwrap();
    s.run_for(1.5);
    let dropped = position(s.server(), crate_of(s.server()));
    assert!(dropped.y < 1.5, "the server lets it fall: {dropped}");
    let client = s.client(0).unwrap();
    let seen = position(client, crate_of(client));
    assert!(
        seen.distance(dropped) < 0.5,
        "the client follows the server again: {seen} vs {dropped}"
    );
}

/// Nobody's mover but the owner's moves an assembly: a client's own
/// BodyPosition on a part the server simulates does nothing anywhere.
#[test]
fn a_client_cant_move_what_it_doesnt_own() {
    let mut s = session_with(SETUP, &["A"], 1.0, &Options::default());
    s.client(0).unwrap().run_extra_script(DRAG).unwrap();
    s.run_for(1.0);
    let at = position(s.server(), crate_of(s.server()));
    assert!(at.distance(glam::Vec3::new(20.0, 1.0, 20.0)) < 0.1, "{at}");
}

/// The ownership API's answers and errors (measured in Studio 0.747).
#[test]
fn the_ownership_api_answers_as_studio_does() {
    let s = session_with(SETUP, &["A"], 0.5, &Options::default());
    s.server()
        .run_extra_script(
            r#"
            local function try(f) print(pcall(f)) end
            local crate, floor = workspace.Crate, workspace.Floor
            crate:SetNetworkOwner(workspace)
            print(crate:GetNetworkOwner(), crate:GetNetworkOwnershipAuto())
            crate:SetNetworkOwner()
            print(crate:GetNetworkOwner(), crate:GetNetworkOwnershipAuto())
            try(function() crate:SetNetworkOwner(5) end)
            try(function() floor:SetNetworkOwner(nil) end)
            try(function() return floor:GetNetworkOwner() end)
            print(floor:CanSetNetworkOwnership())
            print(crate:CanSetNetworkOwnership())
            local loose = Instance.new("Part")
            try(function() loose:SetNetworkOwner(nil) end)
            local root = game.Players.A.Character.HumanoidRootPart
            print(root:GetNetworkOwner(), root:GetNetworkOwnershipAuto())
            root:SetNetworkOwner(nil)
            print(root:GetNetworkOwner(), root:GetNetworkOwnershipAuto())
            root:SetNetworkOwnershipAuto()
            print(root:GetNetworkOwner(), root:GetNetworkOwnershipAuto())
            "#,
        )
        .unwrap();
    let anchored = "Network Ownership API cannot be called on Anchored parts or parts welded to Anchored parts.";
    assert_eq!(
        lines(s.server()),
        [
            "nil false".to_owned(),
            "nil false".to_owned(),
            "false Unable to cast value to Object".to_owned(),
            format!("false {anchored}"),
            format!("false {anchored}"),
            format!("false {anchored}"),
            "true".to_owned(),
            "false Can only call Network Ownership API on a part that is descendent of Workspace"
                .to_owned(),
            "A true".to_owned(),
            "nil false".to_owned(),
            "A true".to_owned(),
        ]
    );
    s.client(0)
        .unwrap()
        .run_extra_script(
            "print(pcall(function() return workspace.Crate:GetNetworkOwner() end))
            print(workspace.Crate:CanSetNetworkOwnership())",
        )
        .unwrap();
    let only = "Network Ownership API can only be called from the Server.";
    assert_eq!(
        lines(s.client(0).unwrap()),
        [format!("false {only}"), format!("false {only}")]
    );
}
