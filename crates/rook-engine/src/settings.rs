//! Every setting Rook has, resolved once here and handed to the crates
//! below as plain values; no other crate reads the environment
//! (docs/design/data-and-logging.md#settings): the build's
//! ([`crate::settings::build_config`]), the player's ([`crate::settings::user_settings`]), and
//! run-time ones, each declared once in [`table`]. A flag beats the
//! variable, which beats the default; [`Tier::Developer`] settings count
//! only in builds with the `developer-overrides` feature. The Open Cloud
//! key comes from the OS credential store, read only when it can be used
//! ([`api_key`]).

pub mod build_config;
mod content;
pub mod table;
pub mod user_settings;

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::data_folder::{self, DataDir};
use rook_content::resolver::ApiKey;
use rook_content::resolver::cache;
use rook_content::resolver::install::{self, Places};
use rook_content::resolver::packages::Packages;
use rook_content::{Config, ProviderKind};
use rook_platform::endpoints::Endpoints;

pub use content::*;
pub use table::*;

/// Whether this build takes [`Tier::Developer`] settings (the
/// `developer-overrides` feature). Such a build says so in its window's
/// title and its log's first line.
pub const DEVELOPER_BUILD: bool = cfg!(feature = "developer-overrides");

#[cfg(all(feature = "published", feature = "developer-overrides"))]
compile_error!(
    "`published` with `developer-overrides`: a build for other people must not let flags or \
     variables redirect its web requests or run other code. Build it with `cargo dist`, which \
     leaves `developer-overrides` out."
);

/// Who may change a setting at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Any,
    /// Only builds with developer overrides ([`DEVELOPER_BUILD`]): what
    /// could redirect a published build's traffic or run other code.
    Developer,
}

/// What a setting is when nothing gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unset {
    /// This value, read as a variable's would be.
    Is(&'static str),
    /// No value; what the caller does instead, as the docs say it.
    Means(&'static str),
}

#[derive(Debug)]
pub struct Setting {
    /// Its name in docs and warnings.
    pub name: &'static str,
    pub tier: Tier,
    pub unset: Unset,
    pub var: &'static str,
    /// Its flag, without the dashes, for the programs that take it.
    pub flag: Option<&'static str>,
}

impl Setting {
    /// The flag, for clap's `#[arg(long = ...)]`.
    pub const fn long(&self) -> &'static str {
        match self.flag {
            Some(flag) => flag,
            None => panic!("this setting has no flag"),
        }
    }

    pub fn allowed(&self) -> bool {
        self.tier == Tier::Any || DEVELOPER_BUILD
    }

    /// The setting's value: `flag` (what the program parsed from its
    /// flag), else the variable, else [`Self::unset`]'s value; `None` when
    /// that means none. A variable `parse` refuses is skipped with a
    /// warning; a build that doesn't take the setting ignores both, with a
    /// warning.
    pub fn get<T>(&self, flag: Option<T>, parse: impl Fn(&str) -> Result<T, String>) -> Option<T> {
        let var = std::env::var_os(self.var)
            .filter(|v| !v.is_empty())
            .and_then(|value| match value.into_string() {
                Ok(text) => Some(text),
                Err(_) => {
                    warn_once(format!("{} ignored: not valid Unicode", self.var));
                    None
                }
            });
        if !self.allowed() {
            let given = match (&flag, &var) {
                (Some(_), _) => self.flag.map(|f| format!("--{f}")),
                (None, Some(_)) => Some(self.var.to_owned()),
                (None, None) => None,
            };
            if let Some(given) = given {
                warn_once(format!(
                    "{given} ignored: {} is a developer setting, and this build has no developer overrides",
                    self.name
                ));
            }
        } else {
            if flag.is_some() {
                return flag;
            }
            if let Some(text) = var {
                // Never the value itself: the Open Cloud key is one.
                match parse(&text) {
                    Ok(v) => return Some(v),
                    Err(why) => warn_once(format!("{} ignored: {why}", self.var)),
                }
            }
        }
        match self.unset {
            Unset::Is(text) => {
                Some(parse(text).unwrap_or_else(|why| panic!("{}'s default: {why}", self.name)))
            }
            Unset::Means(_) => None,
        }
    }

    pub fn switch(&self, flag: Option<bool>) -> Option<bool> {
        self.get(flag, |s| {
            rook_content::resolver::parse_switch(s).ok_or_else(|| "not on or off".to_owned())
        })
    }

    pub fn text(&self, flag: Option<String>) -> Option<String> {
        self.get(flag, |s| Ok(s.trim().to_owned()))
    }
}

/// Warn once per message; until logging starts (the data folder is found
/// first) the warnings wait for it.
fn warn_once(message: String) {
    let mut warned = WARNED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if warned.said.contains(&message) {
        return;
    }
    if tracing::dispatcher::has_been_set() {
        tracing::warn!(target: "rook", "{message}");
    } else {
        warned.waiting.push(message.clone());
    }
    warned.said.push(message);
}

struct Warned {
    said: Vec<String>,
    waiting: Vec<String>,
}

static WARNED: Mutex<Warned> = Mutex::new(Warned {
    said: Vec::new(),
    waiting: Vec::new(),
});

/// Log the warnings made before logging started; `logging::init` calls it.
pub(crate) fn warn_waiting() {
    let waiting = std::mem::take(
        &mut WARNED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .waiting,
    );
    for message in waiting {
        tracing::warn!(target: "rook", "{message}");
    }
}

/// The logical cores to plan background threads for, if set.
pub fn threads() -> Option<usize> {
    THREADS.get(None, |s| {
        s.trim()
            .parse()
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| "not a whole number above 0".into())
    })
}

/// The frame rate cap the settings give over the player's `FramerateCap`,
/// read as it is: -1 at the display's rate, 0 uncapped, else a cap.
/// `uncapped` (`--uncapped`) wins over `framerateCap` (`--framerate-cap`).
pub fn framerate_cap(uncapped: Option<bool>, cap: Option<i32>) -> Option<i32> {
    if UNCAPPED.switch(uncapped) == Some(true) {
        return Some(0);
    }
    FRAMERATE_CAP.get(cap, |s| {
        s.trim().parse().map_err(|_| "not a whole number".into())
    })
}

/// The output device to play through: `audioDevice`, then the player's
/// choice (`saved`, from [`crate::settings::user_settings::PlayerSettings`]); `None`
/// is no sound.
pub fn audio_device(saved: Option<&str>) -> Option<rook_audio::Device> {
    let chosen = AUDIO_DEVICE.text(None);
    match chosen.as_deref().or(saved).map(str::trim) {
        None | Some("") => Some(rook_audio::Device::Default),
        Some(v) if v.eq_ignore_ascii_case("default") || v.eq_ignore_ascii_case("on") => {
            Some(rook_audio::Device::Default)
        }
        Some(v) if v.eq_ignore_ascii_case("off") => None,
        Some(name) => Some(rook_audio::Device::Named(name.to_owned())),
    }
}
