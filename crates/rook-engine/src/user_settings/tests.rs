use super::*;
use crate::{DataModel, Options};
use rook_types::Value;

fn client(path: &Path) -> DataModel {
    DataModel::new_client(&Options {
        user_settings: Some(path.to_owned()),
        ..Options::default()
    })
    .unwrap()
}

fn game_settings(dm: &DataModel) -> InstanceId {
    let w = dm.runtime().world();
    rook_lua::user_settings::game_settings(&w.dom, w.user_settings.unwrap()).unwrap()
}

#[test]
fn changes_are_saved_once_settled_and_load_next_time() {
    let dir = std::env::temp_dir()
        .join("rook-engine-tests")
        .join(format!("user-settings-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join(FILE);

    let mut dm = client(&path);
    let game = game_settings(&dm);
    dm.set_property(game, "MasterVolume", Value::Float32(0.3));
    dm.set_property(game, "CameraYInverted", Value::Bool(true));
    dm.step();
    assert_eq!(dm.player_settings().unwrap().master_volume, 0.3);
    assert!(!path.exists(), "saved before the change settled");
    dm.run_for(SAVE_DELAY + 0.1);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(r#"<Item class="UserGameSettings""#));
    assert!(text.contains(r#"<float name="MasterVolume">0.3</float>"#));
    // Not saved by Roblox either.
    assert!(!text.contains("RotationType"));

    let next = client(&path);
    let settings = next.player_settings().unwrap();
    assert_eq!(settings.master_volume, 0.3);
    assert!(settings.camera_y_inverted);
    assert!(!settings.fullscreen);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn settings_rook_doesnt_know_are_written_back() {
    let dir = std::env::temp_dir()
        .join("rook-engine-tests")
        .join(format!("user-settings-unknown-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(FILE);
    std::fs::write(
        &path,
        r#"<roblox version="4"><Item class="UserGameSettings" referent="RBX0"><Properties>
<string name="Name">GameSettings</string>
<float name="MasterVolume">0.5</float>
<string name="SettingFromLater">kept</string>
</Properties></Item></roblox>"#,
    )
    .unwrap();
    let mut dm = client(&path);
    assert_eq!(dm.player_settings().unwrap().master_volume, 0.5);
    let game = game_settings(&dm);
    dm.set_property(game, "Fullscreen", Value::Bool(true));
    dm.step();
    dm.save_user_settings();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(r#"<string name="SettingFromLater">kept</string>"#));
    assert!(text.contains(r#"<bool name="Fullscreen">true</bool>"#));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The menu's Output Device list (Roblox's own code asks): the system's
/// default first, then each device; choosing one keeps it as
/// `RookOutputDevice`, saved and read back, which the window follows.
#[test]
fn the_menu_lists_output_devices_and_keeps_the_choice() {
    let dir = std::env::temp_dir()
        .join("rook-engine-tests")
        .join(format!("user-settings-device-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join(FILE);
    let mut dm = client(&path);
    dm.set_output_devices(rook_audio::OutputDevices {
        default: Some("Speakers (A)".into()),
        names: vec!["Speakers (A)".into(), "Headphones (B)".into()],
    });
    let source = r#"
        local S = game:GetService("SoundService")
        local function show()
            local names, ids, index = S:GetOutputDevices()
            print(table.concat(names, "|"), table.concat(ids, "|"), index)
        end
        show()
        S:SetOutputDevice("Headphones (B)", "Headphones (B)")
        show()
    "#;
    let script = dm
        .runtime()
        .world_mut()
        .dom
        .create_by_name("Script")
        .unwrap();
    let code = mlua::chunk::Compiler::new().compile(source).unwrap();
    let core = rook_lua::Compiled {
        code: code.into(),
        core: true,
    };
    dm.runtime().world_mut().bytecode.insert(script, core);
    assert!(dm.runtime().run_script(script));
    dm.step();
    let lines: Vec<String> = dm
        .runtime()
        .take_log()
        .into_iter()
        .map(|e| e.message)
        .collect();
    assert_eq!(
        lines,
        [
            "Speakers (A)|Speakers (A)|Headphones (B) |Speakers (A)|Headphones (B) 1",
            "Speakers (A)|Speakers (A)|Headphones (B) |Speakers (A)|Headphones (B) 3",
        ]
    );
    assert_eq!(
        dm.player_settings().unwrap().output_device.as_deref(),
        Some("Headphones (B)")
    );
    dm.save_user_settings();
    assert_eq!(
        client(&path)
            .player_settings()
            .unwrap()
            .output_device
            .as_deref(),
        Some("Headphones (B)")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
