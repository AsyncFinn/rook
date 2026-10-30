//! The player's settings, `UserSettings().GameSettings`, kept in the data
//! folder as `GlobalBasicSettings.xml`, in the place-file XML Roblox's
//! client saves its own in (docs/behavior/user-settings.md).

use std::path::{Path, PathBuf};

use rbx_dom_weak::WeakDom;
use rook_dom::{Change, Dom, InstanceId};
use rook_reflection::{PropertyKind, Serialization};
use rook_types::Value;

use crate::bridges::journal_watch::JournalWatch;

/// Rook's file, in the data folder.
pub const FILE: &str = "GlobalBasicSettings.xml";

/// Rook's own property for the output device, which Roblox's
/// UserGameSettings has none for; kept in the file beside Roblox's
/// (docs/behavior/user-settings.md).
pub const OUTPUT_DEVICE: &str = "RookOutputDevice";

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
    /// The output device's name ([`OUTPUT_DEVICE`]); `None` is the
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

impl PlayerSettings {
    pub fn pacing(&self) -> Pacing {
        match u32::try_from(self.framerate_cap) {
            Err(_) => Pacing::Display,
            Ok(0) => Pacing::Uncapped,
            Ok(n) => Pacing::Cap(n),
        }
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
            output_device: dom.get(game).and_then(|i| {
                i.unknown_properties
                    .iter()
                    .find_map(|(name, value)| match value {
                        Value::String(s) if name == OUTPUT_DEVICE && !s.is_empty() => {
                            Some(s.clone())
                        }
                        _ => None,
                    })
            }),
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

/// A client's settings and where they are saved.
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
    install_saved(dom, path.and_then(saved).as_ref())
}

fn install_saved(dom: &mut Dom, saved: Option<&WeakDom>) -> (InstanceId, InstanceId) {
    let root = rook_lua::user_settings::root(dom);
    if let Some(weak) = saved {
        rook_formats::into_dom(weak, dom, root);
    }
    (root, rook_lua::user_settings::complete(dom, root))
}

/// Set each of `values` the file at `path` doesn't hold yet and save it
/// (made from Roblox's defaults if there is none), for settings an older
/// Rook kept elsewhere ([`crate::data_folder::migrate`]). Names that
/// aren't UserGameSettings properties are kept as Rook's own. Returns the
/// names set.
pub(crate) fn carry_over(
    path: &Path,
    values: Vec<(&'static str, Value)>,
) -> Result<Vec<&'static str>, String> {
    let saved = saved(path);
    let holds = |name: &str| {
        saved.as_ref().is_some_and(|weak| {
            weak.descendants()
                .any(|i| i.class == "UserGameSettings" && i.properties.keys().any(|k| k == name))
        })
    };
    let mut dom = Dom::new();
    let (_, game) = install_saved(&mut dom, saved.as_ref());
    let mut set = Vec::new();
    for (name, value) in values {
        if holds(name) {
            continue;
        }
        match dom.property_id(game, name) {
            Some(p) => dom
                .set_property(game, p, value)
                .map_err(|e| e.to_string())?,
            None => dom
                .instance_mut(game)
                .expect("exists")
                .unknown_properties
                .push((name.to_owned(), value)),
        }
        set.push(name);
    }
    if !set.is_empty() {
        save(&dom, game, path)?;
    }
    Ok(set)
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

    /// Look at the frame's changes; save once they have settled.
    pub(crate) fn check(&mut self, dom: &Dom, now: f64) {
        let (first, changes) = self.watch.look(dom);
        let game = self.game;
        let changed = changes
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
            let w = self.runtime.world();
            kept.check(&w.dom, w.scheduler.now());
        }
    }

    /// The player's settings as they are now, on a client that keeps them.
    pub fn player_settings(&self) -> Option<PlayerSettings> {
        self.user_settings.as_ref().map(|k| k.current.clone())
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
mod tests;
