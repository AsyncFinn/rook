use crate::tests::*;

/// StyleSheets style what scripts change after the frame's restyle (a tag
/// added on Heartbeat, an object made after `task.wait`), though the
/// journal is trimmed at each frame's end.
#[test]
fn styles_reach_changes_made_later_in_the_frame() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Style",
        r#"
        local gui = script.Parent
        local sheet = Instance.new("StyleSheet")
        sheet.Parent = gui
        local link = Instance.new("StyleLink")
        link.StyleSheet = sheet
        link.Parent = gui
        local rule = Instance.new("StyleRule")
        rule.Selector = ".dim"
        rule:SetProperties({ BackgroundTransparency = 0.5 })
        rule.Parent = sheet
        task.wait(0.1)
        local made = Instance.new("Frame")
        made:AddTag("dim")
        made.Parent = gui
        local tagged = Instance.new("Frame")
        tagged.Parent = gui
        game:GetService("RunService").Heartbeat:Once(function()
            tagged:AddTag("dim")
        end)
        task.wait(0.2)
        print(made.BackgroundTransparency, tagged.BackgroundTransparency)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Styler", 1).unwrap();
    session.run_for(0.5);
    let client = session.client_mut(index).unwrap();
    assert_eq!(lines(client), ["0.5 0.5"]);
}

/// A property a script set itself after a rule did stays the script's
/// when the rule stops matching, and once reset to its default the rule
/// sets it again (React clears the player list's hover color so).
#[test]
fn a_script_write_outlasts_the_rule_it_replaced() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Style",
        r#"
        local gui = script.Parent
        local sheet = Instance.new("StyleSheet")
        sheet.Parent = gui
        local link = Instance.new("StyleLink")
        link.StyleSheet = sheet
        link.Parent = gui
        local rule = Instance.new("StyleRule")
        rule.Selector = ".hot"
        rule:SetProperties({ BackgroundColor3 = Color3.new(1, 0, 0) })
        rule.Parent = sheet
        local f = Instance.new("Frame")
        f:AddTag("hot")
        f.Parent = gui
        task.wait(0.1)
        print(f.BackgroundColor3)
        f.BackgroundColor3 = Color3.new(0, 0, 1)
        f:RemoveTag("hot")
        task.wait(0.1)
        print(f.BackgroundColor3)
        f:AddTag("hot")
        f:ResetPropertyToDefault("BackgroundColor3")
        task.wait(0.1)
        print(f.BackgroundColor3)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Styler", 1).unwrap();
    session.run_for(0.5);
    let client = session.client_mut(index).unwrap();
    assert_eq!(lines(client), ["1, 0, 0", "0, 0, 1", "1, 0, 0"]);
}

/// Where StyleLinks nest, the nearest one styles a tree: an outer empty
/// sheet (CoreGui's own) doesn't undo an inner one's styles.
#[test]
fn the_nearest_style_link_styles_a_tree() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Style",
        r#"
        local gui = script.Parent
        local function link(parent)
            local sheet = Instance.new("StyleSheet")
            sheet.Parent = parent
            local l = Instance.new("StyleLink")
            l.StyleSheet = sheet
            l.Parent = parent
            return sheet
        end
        link(gui)
        local frames = {}
        for i = 1, 8 do
            local group = Instance.new("Frame")
            local rule = Instance.new("StyleRule")
            rule.Selector = ".dim"
            rule:SetProperties({ BackgroundTransparency = 0.5 })
            rule.Parent = link(group)
            local f = Instance.new("Frame")
            f:AddTag("dim")
            f.Parent = group
            group.Parent = gui
            frames[i] = f
        end
        task.wait(0.2)
        local seen = {}
        for _, f in frames do
            table.insert(seen, f.BackgroundTransparency)
        end
        print(table.concat(seen, " "))
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Styler", 1).unwrap();
    session.run_for(0.5);
    let client = session.client_mut(index).unwrap();
    assert_eq!(lines(client), ["0.5 0.5 0.5 0.5 0.5 0.5 0.5 0.5"]);
}

/// A window resize reaches scripts and GUI layout the frame it happens,
/// and a camera a script swaps in gets the window's size.
#[test]
fn resizing_the_window_lays_gui_out_again() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Watch",
        r#"
        local camera = workspace.CurrentCamera
        camera:GetPropertyChangedSignal("ViewportSize"):Connect(function()
            print("viewport", camera.ViewportSize)
        end)
        local hud = script.Parent
        hud:GetPropertyChangedSignal("AbsoluteSize"):Connect(function()
            print("hud", hud.AbsoluteSize)
        end)
        print("ready", hud.AbsoluteSize)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Resizer", 1).unwrap();
    session.client_mut(index).unwrap().set_viewport(800, 600);
    session.run_for(0.2);
    let client = session.client_mut(index).unwrap();
    assert_eq!(lines(client), ["ready 800, 542"]);

    client.set_viewport(1000, 700);
    session.step();
    let client = session.client_mut(index).unwrap();
    assert_eq!(lines(client), ["viewport 1000, 700", "hud 1000, 642"]);

    client
        .runtime()
        .run_source(
            "Swap",
            r#"
            local camera = Instance.new("Camera")
            camera.Parent = workspace
            workspace.CurrentCamera = camera
            "#,
            None,
        )
        .unwrap();
    session.step();
    let client = session.client_mut(index).unwrap();
    let camera = client.current_camera().unwrap();
    let world = client.runtime().world();
    assert_eq!(
        world.dom.vector2_property(camera, "ViewportSize"),
        Some(rook_types::Vector2::new(1000.0, 700.0))
    );
    let hud = world
        .dom
        .local_player()
        .and_then(|p| world.dom.find_first_child(p, "PlayerGui", false))
        .and_then(|g| world.dom.find_first_child(g, "Hud", false))
        .unwrap();
    assert_eq!(
        world.dom.vector2_property(hud, "AbsoluteSize"),
        Some(rook_types::Vector2::new(1000.0, 642.0))
    );
}

/// The TextBox keys type into has one record, which scripts and the
/// drawn caret both read: releasing focus clears it even after the box
/// has left the arena, where a second copy for scripts was cleared alone
/// and the caret's kept the box.
#[test]
fn text_box_focus_is_one_record() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Focus",
        r#"
        local box = Instance.new("TextBox")
        box.Name = "Box"
        box.Size = UDim2.fromOffset(100, 20)
        box.Parent = script.Parent
        box:CaptureFocus()
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Typist", 1).unwrap();
    session.run_for(0.1);
    let client = session.client_mut(index).unwrap();
    let focused = client.focused_text_box().expect("the box has focus");
    assert_eq!(client.runtime.world().gui.focus(), Some(focused));
    client.runtime.destroy(focused);
    client.runtime.lua().gc_collect().unwrap();
    session.run_for(0.1);
    let client = session.client_mut(index).unwrap();
    client.runtime.release_text_box(false, None);
    assert_eq!(client.focused_text_box(), None);
    assert_eq!(client.runtime.world().gui.focus(), None);
}

/// A focused TextBox a script destroys gives up the keys: the record keys
/// type into empties with it, where it kept the destroyed box.
#[test]
fn destroyed_text_box_loses_focus() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Focus",
        r#"
        local box = Instance.new("TextBox")
        box.Parent = script.Parent
        box:CaptureFocus()
        task.wait()
        box:Destroy()
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Typist", 1).unwrap();
    session.run_for(0.2);
    let client = session.client_mut(index).unwrap();
    assert_eq!(client.focused_text_box(), None);
    assert_eq!(client.runtime.world().gui.focus(), None);
}

/// `GuiService:Select(parent)` picks the shown, selectable, interactable,
/// on-screen descendant highest up, then leftmost, then drawn on top
/// (Studio's answers for each case).
#[test]
fn select_picks_what_studio_picks() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Probe",
        r#"
        local GS = game:GetService("GuiService")
        local pg = script.Parent.Parent
        local function btn(name, parent, x, y, extra)
            local b = Instance.new("TextButton")
            b.Name = name
            b.Size = UDim2.fromOffset(50, 50)
            b.Position = UDim2.fromOffset(x, y)
            b.Parent = parent
            if extra then extra(b) end
            return b
        end
        local function case(label, build)
            GS.SelectedObject = nil
            local sg = Instance.new("ScreenGui")
            sg.Parent = pg
            local root = Instance.new("Frame")
            root.Name = "Root"
            root.Size = UDim2.fromOffset(600, 600)
            root.Parent = sg
            build(root)
            GS:Select(root)
            print(label, if GS.SelectedObject then GS.SelectedObject.Name else "nil")
            sg:Destroy()
        end
        case("order by child", function(r) btn("Second", r, 300, 300) btn("First", r, 10, 10) end)
        case("top vs near", function(r) btn("TopFar", r, 400, 0) btn("LeftNear", r, 0, 20) end)
        case("left only", function(r) btn("X300", r, 300, 100) btn("X0", r, 0, 100) end)
        case("hidden first", function(r)
            btn("Hidden", r, 0, 0, function(b) b.Visible = false end) btn("Shown", r, 300, 300)
        end)
        case("parent hidden", function(r)
            local f = Instance.new("Frame") f.Visible = false f.Parent = r
            btn("In", f, 0, 0) btn("Out", r, 300, 300)
        end)
        case("not selectable", function(r)
            btn("NoSel", r, 0, 0, function(b) b.Selectable = false end) btn("Sel", r, 300, 300)
        end)
        case("disabled", function(r)
            btn("NoInteract", r, 0, 0, function(b) b.Interactable = false end) btn("Out", r, 300, 300)
        end)
        case("inactive", function(r)
            btn("Inactive", r, 0, 0, function(b) b.Active = false end) btn("Out", r, 300, 300)
        end)
        case("frame selectable", function(r)
            local f = Instance.new("Frame") f.Name = "SelFrame" f.Selectable = true
            f.Size = UDim2.fromOffset(50, 50) f.Parent = r
            btn("Btn", r, 300, 300)
        end)
        case("nested", function(r)
            local f = Instance.new("Frame") f.Size = UDim2.fromOffset(100, 100) f.Parent = r
            btn("Deep", f, 0, 0) btn("Top", r, 300, 300)
        end)
        case("offscreen", function(r) btn("Off", r, -5000, 0) btn("On", r, 300, 300) end)
        case("same spot", function(r) btn("P", r, 100, 100) btn("Q", r, 100, 100) end)
        case("same spot z first", function(r)
            btn("HighZ", r, 100, 100, function(b) b.ZIndex = 5 end) btn("LowZ", r, 100, 100)
        end)
        case("none", function(r) end)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Selector", 1).unwrap();
    session.client_mut(index).unwrap().set_viewport(800, 600);
    session.run_for(0.2);
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "order by child First",
            "top vs near TopFar",
            "left only X0",
            "hidden first Shown",
            "parent hidden Out",
            "not selectable Sel",
            "disabled Out",
            "inactive Inactive",
            "frame selectable SelFrame",
            "nested Deep",
            "offscreen On",
            "same spot Q",
            "same spot z first HighZ",
            "none nil",
        ]
    );
}
