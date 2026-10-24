//! `UserSettings()`: the player's own settings, `GameSettings` (a
//! UserGameSettings) under a `UserSettings` root outside the DataModel.
//! The engine fills it from the player's saved file before scripts start
//! and saves it again (`rook-engine`'s `user_settings.rs`); see
//! docs/behavior/user-settings.md.

use rook_dom::{Dom, InstanceId};
use rook_types::{Value, Vector2};

/// Roblox's defaults for a player who never changed anything, from a
/// fresh client's saved settings and creator-docs UserGameSettings.
fn defaults() -> Vec<(&'static str, Value)> {
    let item = |e: &str, name: &str| {
        let db = rook_reflection::database();
        let id = db.enum_by_name(e).expect("enum in the database").id;
        Value::Enum(rook_reflection::enum_item_value(id, name).expect("enum item"))
    };
    let one = Value::Vector2(Vector2::new(1.0, 1.0));
    vec![
        ("AllTutorialsDisabled", Value::Bool(false)),
        ("BadgeVisible", Value::Bool(true)),
        ("CameraMode", item("CustomCameraMode", "Default")),
        ("CameraYInverted", Value::Bool(false)),
        // Translating needs Roblox's servers, so there's nothing to introduce.
        ("ChatTranslationEnabled", Value::Bool(false)),
        ("ChatTranslationFTUXShown", Value::Bool(true)),
        ("ChatVisible", Value::Bool(true)),
        (
            "ComputerCameraMovementMode",
            item("ComputerCameraMovementMode", "Default"),
        ),
        (
            "ComputerMovementMode",
            item("ComputerMovementMode", "Default"),
        ),
        ("ControlMode", item("ControlMode", "Classic")),
        ("FramerateCap", Value::Int32(-1)),
        ("Fullscreen", Value::Bool(false)),
        ("GamepadCameraSensitivity", Value::Float32(1.0)),
        ("GraphicsQualityLevel", Value::Int32(0)),
        ("HapticStrength", Value::Float32(1.0)),
        ("HasEverUsedVR", Value::Bool(false)),
        ("IsUsingCameraYInverted", Value::Bool(false)),
        ("MasterVolume", Value::Float32(1.0)),
        ("MicroProfilerWebServerEnabled", Value::Bool(false)),
        ("MicroProfilerWebServerIP", Value::String("0.0.0.0".into())),
        ("MicroProfilerWebServerPort", Value::Int32(0)),
        ("MouseSensitivity", Value::Float32(1.0)),
        ("MouseSensitivityFirstPerson", one.clone()),
        ("MouseSensitivityThirdPerson", one),
        ("OnScreenProfilerEnabled", Value::Bool(false)),
        ("PartyVoiceVolume", Value::Float32(1.0)),
        ("PerformanceStatsVisible", Value::Bool(false)),
        ("PlayerNamesEnabled", Value::Bool(true)),
        ("PreferredTextSize", item("PreferredTextSize", "Medium")),
        ("PreferredTransparency", Value::Float32(1.0)),
        ("ReducedMotion", Value::Bool(false)),
        ("RotationType", item("RotationType", "MovementRelative")),
        (
            "SavedQualityLevel",
            item("SavedQualitySetting", "Automatic"),
        ),
        (
            "TouchCameraMovementMode",
            item("TouchCameraMovementMode", "Default"),
        ),
        ("TouchMovementMode", item("TouchMovementMode", "Default")),
        ("UiNavigationKeyBindEnabled", Value::Bool(true)),
        ("VREnabled", Value::Bool(false)),
        ("VoiceChatVolume", Value::Float32(1.0)),
    ]
}

/// A new `UserSettings` root, outside the DataModel.
pub fn root(dom: &mut Dom) -> InstanceId {
    let id = dom
        .create_by_name("UserSettings")
        .expect("UserSettings class");
    dom.set_name(id, "UserSettings".into())
        .expect("just created");
    id
}

/// `root`'s `GameSettings`, made if the saved file had none, with Roblox's
/// default for each setting it didn't save. Not journaled: it is what the
/// settings were before anything ran.
pub fn complete(dom: &mut Dom, root: InstanceId) -> InstanceId {
    let journaling = dom.journal().is_enabled();
    dom.journal_mut().set_enabled(false);
    let game = match game_settings(dom, root) {
        Some(game) => game,
        None => {
            let game = dom
                .create_by_name("UserGameSettings")
                .expect("UserGameSettings class");
            dom.set_name(game, "GameSettings".into())
                .expect("just created");
            dom.attach_unchecked(game, root);
            game
        }
    };
    let class = dom.get(game).expect("exists").class;
    let db = dom.database();
    for (name, value) in defaults() {
        let p = db
            .find_canonical_property(class, name)
            .expect("UserGameSettings property");
        if !dom.is_set(game, p.id) {
            dom.load_property(game, p.id, value);
        }
    }
    dom.journal_mut().set_enabled(journaling);
    game
}

/// `UserSettings().GameSettings`, once something has made it.
pub fn game_settings(dom: &Dom, root: InstanceId) -> Option<InstanceId> {
    dom.children(root)
        .iter()
        .copied()
        .find(|&c| dom.class_name(c) == "UserGameSettings")
}

/// `UserSettings()` and its `GameSettings`, made on first use when the
/// engine gave none.
pub(crate) fn ensure(w: &mut crate::world::World) -> (InstanceId, InstanceId) {
    let root = *w.user_settings.get_or_insert_with(|| root(&mut w.dom));
    (root, complete(&mut w.dom, root))
}
