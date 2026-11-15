//! The player's settings, `UserSettings().GameSettings`, kept in the data
//! folder as `GlobalBasicSettings.xml`, in the place-file XML Roblox's
//! client saves its own in (docs/behavior/user-settings.md).

use std::path::{Path, PathBuf};

use rbx_dom_weak::WeakDom;
use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::{PropertyKind, Serialization};

use crate::bridges::journal_watch::JournalWatch;

/// Rook's file, in the data folder.
pub const FILE: &str = "GlobalBasicSettings.xml";

/// Seconds a change waits before it is saved, so a dragged slider writes
/// the file once.
const SAVE_DELAY: f64 = 1.0;

/// The settings Rook follows itself.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSettings {
    /// Everything heard, 0 to 1.
    pub master_volume: f32,
    /// How far the camera turns per pixel, across and up, against
    /// Roblox's default of 1: `MouseSensitivityThirdPerson`, which the
    /// menu's slider sets (with the first-person one).
    pub mouse_sensitivity: (f32, f32),
    pub camera_y_inverted: bool,
    pub fullscreen: bool,
    /// `FramerateCap`, read as [`Pacing`].
    pub framerate_cap: i32,
    /// The output device's name (`RookOutputDevice`); `None` is the
    /// system's default.
    pub output_device: Option<String>,
}

/// How often the player draws, from `FramerateCap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pacing {
    /// Roblox's default, -1 (and any other negative cap): at the display's
    /// rate.
    Display,
    /// At most this many frames a second, not waiting for the display.
    Cap(u32),
    /// 0: as fast as frames are drawn (Rook's reading).
    Uncapped,
}

impl Pacing {
    pub fn from_cap(cap: i32) -> Self {
        match u32::try_from(cap) {
            Err(_) => Self::Display,
            Ok(0) => Self::Uncapped,
            Ok(n) => Self::Cap(n),
        }
    }
}

impl PlayerSettings {
    pub fn pacing(&self) -> Pacing {
        Pacing::from_cap(self.framerate_cap)
    }

    fn read(dom: &Dom, game: InstanceId) -> Self {
        let sensitivity = dom
            .vector2_property(game, "MouseSensitivityThirdPerson")
            .map_or((1.0, 1.0), |v| (v.x(), v.y()));
        Self {
            master_volume: dom
                .float_property(game, "MasterVolume")
                .unwrap_or(1.0)
                .clamp(0.0, 1.0),
            mouse_sensitivity: sensitivity,
            camera_y_inverted: dom.bool_property(game, "CameraYInverted") == Some(true),
            fullscreen: dom.bool_property(game, "Fullscreen") == Some(true),
            framerate_cap: dom
                .number_property(game, "FramerateCap")
                .map_or(-1, |n| n as i32),
            output_device: rook_lua::user_settings::output_device(dom, game),
        }
    }

    /// The settings saved at `path` (Roblox's defaults without one),
    /// before any game runs: the window's first look and sound.
    pub fn load(path: Option<&Path>) -> Self {
        let mut dom = Dom::new();
        let (_, game) = install(&mut dom, path);
        Self::read(&dom, game)
    }
}

#[derive(Debug)]
pub(crate) struct Kept {
    path: PathBuf,
    game: InstanceId,
    watch: JournalWatch,
    /// When a change not yet saved was made, in game time.
    changed: Option<f64>,
    pub(crate) current: PlayerSettings,
}

/// The settings saved at `path`, if any.
fn saved(path: &Path) -> Option<WeakDom> {
    let files = &rook_platform::get().files;
    if !files.exists(path) {
        return None;
    }
    let loaded = files
        .read(path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| rook_formats::load(&bytes).map_err(|e| e.to_string()));
    match loaded {
        Ok(loaded) => Some(loaded.dom),
        Err(e) => {
            tracing::warn!(
                "the player's settings in {} are unreadable ({e}); using Roblox's defaults",
                path.display()
            );
            None
        }
    }
}

/// `UserSettings()` with the settings saved at `path`, outside the
/// DataModel; returns it and its `GameSettings`.
fn install(dom: &mut Dom, path: Option<&Path>) -> (InstanceId, InstanceId) {
    let root = rook_lua::user_settings::root(dom);
    if let Some(weak) = path.and_then(saved) {
        rook_formats::copy_into_dom(&weak, dom, root);
    }
    (root, rook_lua::user_settings::complete(dom, root))
}

impl Kept {
    /// Load the settings into a client's DOM; returns `UserSettings()`.
    pub(crate) fn install(dom: &mut Dom, path: &Path) -> (InstanceId, Self) {
        let (root, game) = install(dom, Some(path));
        let kept = Self {
            path: path.to_owned(),
            game,
            watch: JournalWatch::starting_at(dom.journal().end()),
            changed: None,
            current: PlayerSettings::read(dom, game),
        };
        (root, kept)
    }

    pub(crate) fn rescan(&mut self, cursor: u64) {
        self.watch.rescan(cursor);
    }

    /// Look at the frame's changes, and `chose_device` (the output
    /// device changed, which the journal can't note); save once they have
    /// settled.
    pub(crate) fn check(&mut self, dom: &Dom, now: f64, chose_device: bool) {
        let (first, changes) = self.watch.look(dom);
        rook_types::count!(JournalReadBridges, changes.len());
        let game = self.game;
        let changed = chose_device
            || changes
                .iter()
                .any(|c| matches!(c, Change::Property { id, .. } if *id == game));
        // A first look reads them whole, as any watch's does.
        if first || changed {
            self.current = PlayerSettings::read(dom, game);
        }
        if changed {
            self.changed = Some(now);
        }
        if self.changed.is_some_and(|t| now - t >= SAVE_DELAY) {
            self.save(dom);
        }
    }

    /// Save now if anything changed (on exit).
    pub(crate) fn flush(&mut self, dom: &Dom) {
        if self.changed.is_some() {
            self.save(dom);
        }
    }

    fn save(&mut self, dom: &Dom) {
        self.changed = None;
        if let Err(e) = save(dom, self.game, &self.path) {
            tracing::warn!(
                "couldn't save the player's settings to {}: {e}",
                self.path.display()
            );
        }
    }
}

/// Write `game` as Roblox does: its saved properties, in place-file XML,
/// replacing the file.
fn save(dom: &Dom, game: InstanceId, path: &Path) -> Result<(), String> {
    let mut weak = rook_formats::from_dom(dom, &[game]);
    let db = dom.database();
    let class = dom.get(game).expect("exists").class;
    let saved = weak.root().children()[0];
    weak.get_by_ref_mut(saved)
        .expect("just built")
        .properties
        .retain(|name, _| {
            db.find_canonical_property(class, name).is_none_or(|p| {
                !matches!(
                    p.kind,
                    PropertyKind::Canonical(Serialization::DoesNotSerialize)
                )
            })
        });
    let bytes = rook_formats::save(&weak, rook_formats::Format::Xml).map_err(|e| e.to_string())?;
    let files = &rook_platform::get().files;
    if let Some(parent) = path.parent() {
        files.create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    files.write(path, &bytes).map_err(|e| e.to_string())
}

impl crate::DataModel {
    pub(crate) fn check_user_settings(&mut self) {
        if let Some(kept) = &mut self.user_settings {
            let mut w = self.runtime.world_mut();
            let chose_device = std::mem::take(&mut w.output_device_chosen);
            kept.check(&w.dom, w.scheduler.now(), chose_device);
        }
    }

    /// The machine's output devices, for the settings menu to offer.
    pub fn set_output_devices(&mut self, devices: rook_audio::OutputDevices) {
        self.runtime.world_mut().output_devices = devices;
    }

    /// The player's settings as they are now, on a client that keeps them.
    pub fn player_settings(&self) -> Option<&PlayerSettings> {
        self.user_settings.as_ref().map(|k| &k.current)
    }

    /// Save the player's settings now if they changed (before exiting).
    pub fn save_user_settings(&mut self) {
        if let Some(kept) = &mut self.user_settings {
            kept.flush(&self.runtime.world().dom);
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests set up files on the machine running them"
)]
mod tests {
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
}
