use crate::tests::*;

/// A session whose one player's client runs Rook's menu, at the window
/// size the menu's measurements were taken at, with a LocalScript saying
/// what GuiService tells games about the menu.
fn menu_session() -> (Session, usize) {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Listen",
        r#"
        local gs = game:GetService("GuiService")
        gs.MenuOpened:Connect(function() print("opened", gs.MenuIsOpen) end)
        gs.MenuClosed:Connect(function() print("closed", gs.MenuIsOpen) end)
        "#,
    );
    server.start();
    let options = Options {
        menu: true,
        ..Options::default()
    };
    let mut session = Session::new(server, options);
    let index = session.join("Player1", 1).unwrap();
    session.client_mut(index).unwrap().set_viewport(1732, 1121);
    session.run_for(0.1);
    (session, index)
}

fn press(session: &mut Session, index: usize, key: &str) {
    for down in [true, false] {
        let code = key.into();
        session.input(index, InputEvent::Key { code, down });
    }
    session.run_for(0.1);
}

fn click(session: &mut Session, index: usize, x: f32, y: f32) {
    session.input(index, InputEvent::MouseMove { x, y });
    for down in [true, false] {
        session.input(index, InputEvent::MouseButton { button: 1, down });
    }
    session.run_for(0.1);
}

/// The menu's darkened screen is showing.
fn shown(dm: &DataModel) -> bool {
    let w = dm.runtime.world();
    let gui = w
        .dom
        .find_service("CoreGui")
        .and_then(|c| w.dom.find_first_child(c, "RookMenu", false))
        .expect("the menu's ScreenGui is in CoreGui");
    let shield = w
        .dom
        .find_first_child(gui, "SettingsShield", false)
        .unwrap();
    w.dom.bool_property(shield, "Visible") == Some(true)
}

/// The screen rectangle (x, y, width, height) of the GUI object at `path`
/// under CoreGui, in window pixels.
fn rect(session: &Session, index: usize, path: &[&str]) -> (f32, f32, f32, f32) {
    let w = session.client(index).unwrap().runtime().world();
    let dom = &w.dom;
    let id = path
        .iter()
        .try_fold(dom.find_service("CoreGui").unwrap(), |at, child| {
            dom.find_first_child(at, child, false)
        })
        .unwrap_or_else(|| panic!("no {}", path.join(".")));
    let at = dom.vector2_property(id, "AbsolutePosition").unwrap();
    let size = dom.vector2_property(id, "AbsoluteSize").unwrap();
    // AbsolutePosition is counted from below the top bar's inset.
    (at.x(), at.y() + rook_gui::GUI_INSET, size.x(), size.y())
}

/// The middle of the GUI object at `path`.
fn middle(session: &Session, index: usize, path: &[&str]) -> (f32, f32) {
    let (x, y, w, h) = rect(session, index, path);
    (x + w / 2.0, y + h / 2.0)
}

/// A control in the Settings page's row `row`, `path` below it.
fn control<'a>(row: &'a str, path: &[&'a str]) -> Vec<&'a str> {
    let mut at = vec![
        "RookMenu",
        "SettingsShield",
        "MenuContainer",
        "PageViewClipper",
        "PageView",
        row,
        "Control",
    ];
    at.extend_from_slice(path);
    at
}

/// Open the menu with Escape and wait for it to finish sliding in.
fn open(session: &mut Session, index: usize) {
    press(session, index, "Escape");
    session.run_for(0.5);
}

/// Close it with Escape and wait for it to finish sliding out.
fn close(session: &mut Session, index: usize) {
    press(session, index, "Escape");
    session.run_for(0.5);
}

/// Open the menu on its Settings page (it opens on People).
fn open_settings(session: &mut Session, index: usize) {
    open(session, index);
    let (x, y) = middle(
        session,
        index,
        &[
            "RookMenu",
            "SettingsShield",
            "MenuContainer",
            "HubBar",
            "HubBarContainer",
            "GameSettingsTab",
        ],
    );
    click(session, index, x, y);
}

/// `read` of the player's GameSettings.
fn game_settings<T>(
    session: &Session,
    index: usize,
    read: impl FnOnce(&rook_dom::Dom, InstanceId) -> T,
) -> T {
    let w = session.client(index).unwrap().runtime().world();
    let game = w
        .dom
        .find_first_child(w.user_settings.unwrap(), "GameSettings", false)
        .unwrap();
    read(&w.dom, game)
}

/// Escape opens the menu and closes it again; games hear both through
/// GuiService, and the menu's script runs without errors.
#[test]
fn escape_opens_and_closes_the_menu() {
    let (mut session, index) = menu_session();
    assert!(!shown(session.client(index).unwrap()));
    press(&mut session, index, "Escape");
    assert!(shown(session.client(index).unwrap()));
    close(&mut session, index);
    let client = session.client(index).unwrap();
    assert!(!shown(client));
    assert_eq!(lines(client), ["opened true", "closed false"]);
}

/// The logo springs from 24 to 30 as the menu opens, rather than jumping,
/// and back as it closes.
#[test]
fn the_logo_springs_to_its_open_size() {
    let (mut session, index) = menu_session();
    let logo = ["TopBarApp", "MenuButton", "ScalingIcon"];
    let side = |s: &Session| rect(s, index, &logo).2;
    assert_eq!(side(&session), 24.0);
    // press() runs 0.1 s after the key: the logo is on its way.
    press(&mut session, index, "Escape");
    let early = side(&session);
    assert!(early > 25.0 && early < 29.0, "0.1 s in it's {early}");
    // AbsoluteSize is in whole pixels, so wait for the spring to rest.
    session.run_for(1.0);
    assert_eq!(side(&session), 30.0);
    close(&mut session, index);
    session.run_for(0.5);
    assert_eq!(side(&session), 24.0);
}

/// The logo button opens the menu too, and the volume slider's minus
/// lowers `MasterVolume` a step (the player's settings, which only
/// Roblox's and Rook's own code may change).
#[test]
fn the_button_opens_the_menu_and_the_slider_sets_the_volume() {
    let (mut session, index) = menu_session();
    let (x, y) = middle(&session, index, &["TopBarApp", "MenuButton"]);
    click(&mut session, index, x, y);
    session.run_for(0.5);
    assert!(shown(session.client(index).unwrap()));
    let (x, y) = middle(
        &session,
        index,
        &[
            "RookMenu",
            "SettingsShield",
            "MenuContainer",
            "HubBar",
            "HubBarContainer",
            "GameSettingsTab",
        ],
    );
    click(&mut session, index, x, y);
    let (x, y) = middle(
        &session,
        index,
        &control("VolumeFrame", &["Slider", "LeftButton"]),
    );
    click(&mut session, index, x, y);
    let volume = game_settings(&session, index, |dom, game| {
        dom.float_property(game, "MasterVolume").unwrap()
    });
    assert!((volume - 0.9).abs() < 1e-6, "{volume}");
}

/// Leave asks first; confirming shuts the client down.
#[test]
fn leave_asks_then_shuts_down() {
    let (mut session, index) = menu_session();
    press(&mut session, index, "Escape");
    press(&mut session, index, "L");
    assert!(!session.client(index).unwrap().shutdown_requested());
    press(&mut session, index, "Return");
    assert!(session.client(index).unwrap().shutdown_requested());
}

/// Game scripts can't shut the client down or say the menu is open.
#[test]
fn game_scripts_cant_use_the_menus_members() {
    let (mut session, index) = menu_session();
    session
        .client(index)
        .unwrap()
        .run_extra_script(
            r#"
            print(pcall(function() game:Shutdown() end))
            print(pcall(function() game:GetService("GuiService"):SetMenuIsOpen(true) end))
            "#,
        )
        .unwrap();
    session.run_for(0.1);
    let client = session.client(index).unwrap();
    assert!(!client.shutdown_requested());
    let said = lines(client);
    assert_eq!(
        said,
        [
            "false The current thread cannot call 'Shutdown' (lacking capability LocalUser)",
            "false The current thread cannot call 'SetMenuIsOpen' (lacking capability RobloxScript)",
        ]
    );
}

/// Without the setting, or with Roblox's CoreScripts, a client has no
/// Rook menu.
#[test]
fn the_menu_is_off_unless_asked_for() {
    let mut server = Server::new(&Options::default()).unwrap();
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Player1", 1).unwrap();
    session.run_for(0.1);
    let client = session.client(index).unwrap();
    let w = client.runtime.world();
    let menu = w
        .dom
        .find_service("CoreGui")
        .and_then(|c| w.dom.find_first_child(c, "RookMenu", false));
    assert!(menu.is_none());
}

/// A game turning the top bar off hides the button, and Escape still
/// opens the menu.
#[test]
fn topbar_enabled_hides_the_button() {
    let (mut session, index) = menu_session();
    session
        .client(index)
        .unwrap()
        .run_extra_script(
            r#"
            local sg = game:GetService("StarterGui")
            sg:SetCore("TopbarEnabled", false)
            print(sg:GetCore("TopbarEnabled"))
            "#,
        )
        .unwrap();
    session.run_for(0.1);
    press(&mut session, index, "Escape");
    let client = session.client(index).unwrap();
    assert!(shown(client));
    assert_eq!(lines(client), ["false", "opened true"]);
}

/// Camera Sensitivity takes a number typed in its box, kept to Roblox's
/// range (0.1 to 4), and refuses what isn't one.
#[test]
fn typed_sensitivity_stays_in_roblox_range() {
    let (mut session, index) = menu_session();
    open_settings(&mut session, index);
    let sensitivity = |session: &Session| {
        game_settings(session, index, |dom, game| {
            let v = dom
                .vector2_property(game, "MouseSensitivityThirdPerson")
                .unwrap();
            (v.x(), v.y())
        })
    };
    let typed = |session: &mut Session, text: &str| {
        let (x, y) = middle(
            session,
            index,
            &control(
                "Camera SensitivityFrame",
                &["Slider", "CameraSensitivityTextBox"],
            ),
        );
        click(session, index, x, y);
        let text = text.into();
        session.input(index, InputEvent::Text { text });
        press(session, index, "Return");
        sensitivity(session)
    };
    assert_eq!(typed(&mut session, "0.37"), (0.37, 0.37));
    assert_eq!(typed(&mut session, "12.5"), (4.0, 4.0));
    assert_eq!(typed(&mut session, "fast"), (4.0, 4.0));
    assert_eq!(typed(&mut session, "-1"), (0.1, 0.1));
    // The slider's plus steps up from its last lit step.
    let (x, y) = middle(
        &session,
        index,
        &control("Camera SensitivityFrame", &["Slider", "RightButton"]),
    );
    click(&mut session, index, x, y);
    let (v, _) = sensitivity(&session);
    assert!((v - 0.4).abs() < 1e-5, "{v}");
}

/// The volume slider follows a drag, past its ends too, and a press on a
/// step sets it.
#[test]
fn sliders_follow_a_drag() {
    let (mut session, index) = menu_session();
    open_settings(&mut session, index);
    let volume = |session: &Session| {
        game_settings(session, index, |dom, game| {
            (dom.float_property(game, "MasterVolume").unwrap() * 10.0).round()
        })
    };
    let (left, top, width, height) = rect(
        &session,
        index,
        &control("VolumeFrame", &["Slider", "StepsContainer"]),
    );
    let y = top + height / 2.0;
    let step = |i: f32| left + (i - 0.5) * width / 10.0;
    let to = |session: &mut Session, x: f32, y: f32| {
        session.input(index, InputEvent::MouseMove { x, y });
        session.run_for(0.05);
    };
    to(&mut session, step(2.0), y);
    session.input(
        index,
        InputEvent::MouseButton {
            button: 1,
            down: true,
        },
    );
    session.run_for(0.05);
    assert_eq!(volume(&session), 2.0);
    // Off the row while held.
    to(&mut session, step(8.0), y + 160.0);
    assert_eq!(volume(&session), 8.0);
    to(&mut session, left - 200.0, y);
    assert_eq!(volume(&session), 0.0);
    session.input(
        index,
        InputEvent::MouseButton {
            button: 1,
            down: false,
        },
    );
    session.run_for(0.05);
    to(&mut session, step(5.0), y);
    assert_eq!(volume(&session), 0.0);
    click(&mut session, index, step(6.0), y);
    assert_eq!(volume(&session), 6.0);
}

/// The open menu keeps clicks from the game's buttons behind it, where
/// its panel is and where only the darkened screen is.
#[test]
fn the_menu_blocks_clicks_to_the_game() {
    let (mut session, index) = menu_session();
    session
        .client(index)
        .unwrap()
        .run_extra_script(
            r#"
            local gui = Instance.new("ScreenGui")
            gui.IgnoreGuiInset = true
            local button = Instance.new("TextButton")
            button.Size = UDim2.fromScale(1, 1)
            button.Activated:Connect(function() print("clicked") end)
            button.Parent = gui
            gui.Parent = game:GetService("Players").LocalPlayer.PlayerGui
            "#,
        )
        .unwrap();
    session.run_for(0.1);
    click(&mut session, index, 200.0, 700.0);
    press(&mut session, index, "Escape");
    click(&mut session, index, 200.0, 700.0);
    click(&mut session, index, 600.0, 760.0);
    close(&mut session, index);
    click(&mut session, index, 200.0, 700.0);
    let said = lines(session.client(index).unwrap());
    assert_eq!(said, ["clicked", "opened true", "closed false", "clicked"]);
}

/// Roblox's menu as recorded in Studio 0.748 (docs/behavior/gui/menu/).
const RECORDING: &str = include_str!("../../../../../docs/behavior/gui/menu/studio-0.748.json");

/// The recorded node whose path ends with `suffix` in snapshot `label`:
/// its AbsolutePosition and AbsoluteSize.
fn recorded(label: &str, suffix: &str) -> ([f32; 2], [f32; 2]) {
    let json: serde_json::Value = serde_json::from_str(RECORDING).unwrap();
    let snapshot = json["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["label"] == label)
        .unwrap_or_else(|| panic!("no snapshot {label}"));
    let node = snapshot["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["path"].as_str().unwrap().ends_with(suffix))
        .unwrap_or_else(|| panic!("{label} has no {suffix}"));
    let pair = |key: &str| {
        let v = &node[key];
        [v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32]
    };
    (pair("AbsolutePosition"), pair("AbsoluteSize"))
}

/// Our object at `path` against Roblox's at `suffix` in snapshot `label`:
/// sizes always, positions where `at` (GUI space, as recorded).
fn matches(session: &Session, index: usize, path: &[&str], label: &str, suffix: &str, at: bool) {
    let (x, y, w, h) = rect(session, index, path);
    let (pos, size) = recorded(label, suffix);
    // Within a pixel: Roblox's page scrolls, taking a 1-pixel scroll bar
    // from its rows' width, where Rook's shorter page doesn't.
    let near = |a: f32, b: f32| (a - b).abs() <= 1.0;
    assert!(
        near(w, size[0]) && near(h, size[1]),
        "{}: size {w} x {h}, Roblox's {suffix} {} x {}",
        path.join("."),
        size[0],
        size[1]
    );
    if at {
        let y = y - rook_gui::GUI_INSET;
        assert!(
            near(x, pos[0]) && near(y, pos[1]),
            "{}: at ({x}, {y}), Roblox's {suffix} at ({}, {})",
            path.join("."),
            pos[0],
            pos[1]
        );
    }
}

/// The menu's layout is Roblox's, as recorded at both window sizes: the
/// button, the panel, the tab bar, the page, the rows and their controls,
/// and the buttons along the bottom.
#[test]
fn the_layout_is_roblox_s() {
    for (label, width) in [("open-settings", 1760), ("small-open-settings", 630)] {
        let (mut session, index) = menu_session();
        session.client_mut(index).unwrap().set_viewport(width, 990);
        session.run_for(0.1);
        let closed = if width == 1760 { "closed" } else { label };
        matches(
            &session,
            index,
            &["TopBarApp", "MenuButton"],
            closed,
            "TriggerPoint/IconHitArea",
            true,
        );
        open_settings(&mut session, index);
        let menu = ["RookMenu", "SettingsShield", "MenuContainer"];
        let at = |rest: &[&'static str]| [&menu[..], rest].concat();
        matches(
            &session,
            index,
            &menu,
            label,
            "SettingsShield/MenuContainer",
            true,
        );
        matches(
            &session,
            index,
            &at(&["HubBar"]),
            label,
            "Page/HubBar",
            true,
        );
        matches(
            &session,
            index,
            &at(&["HubBar", "HubBarContainer", "GameSettingsTab"]),
            label,
            "HubBarContainer/GameSettingsTab",
            false,
        );
        matches(
            &session,
            index,
            &at(&["PageViewClipper"]),
            label,
            "Page/PageViewClipper",
            true,
        );
        matches(
            &session,
            index,
            &at(&["PageViewClipper", "PageView"]),
            label,
            "PageViewClipper/PageView",
            true,
        );
        for (row, control, theirs) in [
            ("VolumeFrame", &["Slider"][..], "VolumeFrame/Slider"),
            (
                "VolumeFrame",
                &["Slider", "StepsContainer"],
                "VolumeFrame/Slider/StepsContainer",
            ),
            (
                "VolumeFrame",
                &["Slider", "StepsContainer", "Step1"],
                "VolumeFrame/Slider/StepsContainer/Step1",
            ),
            (
                "VolumeFrame",
                &["Slider", "LeftButton"],
                "VolumeFrame/Slider/LeftButton",
            ),
            (
                "Output DeviceFrame",
                &["Selector"],
                "Output DeviceFrame/Selector",
            ),
            (
                "Output DeviceFrame",
                &["Selector", "LeftButton"],
                "Output DeviceFrame/Selector/LeftButton",
            ),
            (
                "Camera SensitivityFrame",
                &["Slider"],
                "Camera SensitivityFrame/Slider",
            ),
            (
                "Camera SensitivityFrame",
                &["Slider", "CameraSensitivityTextBox"],
                "Slider/CameraSensitivityTextBox",
            ),
            (
                "Maximum Frame RateFrame",
                &["DropDownFrameButton"],
                "Maximum Frame RateFrame/DropDownFrameButton",
            ),
        ] {
            let ours = [
                &at(&["PageViewClipper", "PageView", row, "Control"])[..],
                control,
            ]
            .concat();
            matches(&session, index, &ours, label, theirs, false);
        }
        matches(
            &session,
            index,
            &at(&["PageViewClipper", "PageView", "VolumeFrame"]),
            label,
            "Page/VolumeFrame",
            false,
        );
        matches(
            &session,
            index,
            &at(&["PageViewClipper", "PageView", "VolumeFrame", "Label"]),
            label,
            "VolumeFrame/VolumeLabel",
            false,
        );
        matches(
            &session,
            index,
            &at(&["BottomButtonFrame"]),
            label,
            "Page/BottomButtonFrame",
            true,
        );
        matches(
            &session,
            index,
            &at(&["BottomButtonFrame", "LeaveButton"]),
            label,
            "MenuButtonContainer1/MenuButton",
            true,
        );
        matches(
            &session,
            index,
            &at(&["BottomButtonFrame", "ResumeButton"]),
            label,
            "MenuButtonContainer3/MenuButton",
            true,
        );
    }
}
