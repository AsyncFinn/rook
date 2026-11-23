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
    let shield = w.dom.find_first_child(gui, "Shield", false).unwrap();
    w.dom.bool_property(shield, "Visible") == Some(true)
}

/// Escape opens the menu and closes it again; games hear both through
/// GuiService, and the menu's script runs without errors.
#[test]
fn escape_opens_and_closes_the_menu() {
    let (mut session, index) = menu_session();
    assert!(!shown(session.client(index).unwrap()));
    press(&mut session, index, "Escape");
    assert!(shown(session.client(index).unwrap()));
    press(&mut session, index, "Escape");
    let client = session.client(index).unwrap();
    assert!(!shown(client));
    assert_eq!(lines(client), ["opened true", "closed false"]);
}

/// The logo button opens the menu too, and the volume slider's minus
/// lowers `MasterVolume` a step (the player's settings, which only
/// Roblox's and Rook's own code may change).
#[test]
fn the_button_opens_the_menu_and_the_slider_sets_the_volume() {
    let (mut session, index) = menu_session();
    click(&mut session, index, 38.0, 34.0);
    assert!(shown(session.client(index).unwrap()));
    // The Volume row's minus, where Roblox's is at this size.
    click(&mut session, index, 792.0, 334.0);
    let client = session.client(index).unwrap();
    let w = client.runtime.world();
    let settings = w.user_settings.expect("UserSettings() exists");
    let game = w
        .dom
        .find_first_child(settings, "GameSettings", false)
        .unwrap();
    let volume = w.dom.float_property(game, "MasterVolume").unwrap();
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

/// Camera Sensitivity takes any positive number typed in its box, beyond
/// Roblox's 4, and refuses what isn't one.
#[test]
fn any_sensitivity_can_be_typed() {
    let (mut session, index) = menu_session();
    press(&mut session, index, "Escape");
    let typed = |session: &mut Session, text: &str| {
        // The Camera Sensitivity row's box, at its right end.
        click(session, index, 1212.0, 622.0);
        let text = text.into();
        session.input(index, InputEvent::Text { text });
        press(session, index, "Return");
        let client = session.client(index).unwrap();
        let w = client.runtime.world();
        let game = w
            .dom
            .find_first_child(w.user_settings.unwrap(), "GameSettings", false)
            .unwrap();
        let v = w
            .dom
            .vector2_property(game, "MouseSensitivityThirdPerson")
            .unwrap();
        (v.x(), v.y())
    };
    assert_eq!(typed(&mut session, "0.37"), (0.37, 0.37));
    assert_eq!(typed(&mut session, "12.5"), (12.5, 12.5));
    assert_eq!(typed(&mut session, "fast"), (12.5, 12.5));
    assert_eq!(typed(&mut session, "-1"), (12.5, 12.5));
    // The slider's minus steps down from its last lit step (10, past 4).
    click(&mut session, index, 792.0, 622.0);
    let client = session.client(index).unwrap();
    let w = client.runtime.world();
    let game = w
        .dom
        .find_first_child(w.user_settings.unwrap(), "GameSettings", false)
        .unwrap();
    let v = w
        .dom
        .vector2_property(game, "MouseSensitivityThirdPerson")
        .unwrap();
    assert!((v.x() - 3.6).abs() < 1e-5, "{}", v.x());
}
