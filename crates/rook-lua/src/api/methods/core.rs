//! Members Roblox's own CoreScripts call: starting more of them, and the
//! fast flags and engine features they check. See
//! docs/behavior/gui/core-scripts.md.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;

use super::{RawMethod, mutate};
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};

#[expect(
    clippy::match_same_arms,
    reason = "a row per member: members that behave alike stay listed apart"
)]
pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("ScriptContext", "AddCoreScriptLocal") => add_core_script_local,
        ("DataModel", "DefineFastFlag" | "GetFastFlag") => fast_flag,
        ("DataModel", "DefineFastInt" | "GetFastInt") => fast_int,
        ("DataModel", "DefineFastString" | "GetFastString") => fast_string,
        ("DataModel", "GetEngineFeature") => engine_feature,
        ("DataModel", "SetFlagVersion") => nothing,
        ("GlobalSettings", "GetFFlag") => global_flag,
        ("GlobalSettings", "GetFVariable") => global_variable,
        ("GuiService", "GetResolutionScale") => one,
        ("GuiService", "IsTenFootInterface") => no,
        ("UserInputService", "GetPlatform") => platform,
        ("Player", "GetUnder13") => no,
        ("UserGameSettings", "InFullScreen") => no,
        ("RenderSettings", "GetMaxQualityLevel") => max_quality_level,
        ("UserSettings", "IsUserFeatureEnabled") => no,
        ("UserInputService", "GetGamepadConnected") => no,
        ("UserInputService", "GetImageForKeyCode") => empty_string,
        ("UserInputService", "GetStringForKeyCode") => key_string,
        ("RunService", "GetRobloxVersion" | "GetCoreScriptVersion") => roblox_version,
        ("RunService", "GetRobloxClientChannel") => channel,
        ("VRService", "IsVRAppBuild") => no,
        ("GuiService", "GetInspectMenuEnabled") => no,
        ("MessageBusService", "GetMessageId") => empty_string,
        ("IXPService", "GetUserLayerVariables" | "GetBrowserTrackerLayerVariables") => empty_table,
        // Observed: the back button compares it with a number; no ad
        // teleport is ever pending locally.
        ("AdService", "GetAdTeleportInfo") => zero,
        _ => return None,
    })
}

fn nothing(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, ())
}

/// What services that ask Roblox's website (experiments, ads) answer
/// with nothing known.
fn empty_table(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, lua.create_table()?)
}

/// The Roblox version Rook's API is pinned to, as `version()` gives.
fn roblox_version(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, rook_reflection::roblox_version())
}

/// Roblox's release channel for everyone.
fn channel(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, "LIVE")
}

/// What a key is labelled on a US keyboard: its `Enum.KeyCode` name, which
/// is the label for letters and most keys (Approximation: Roblox gives the
/// symbol for punctuation keys).
fn key_string(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = crate::types::enums::item_name(lua, a.get(0), "KeyCode").unwrap_or_default();
    Outcome::ret(lua, name)
}

/// The graphics quality slider's top: Roblox's 21 levels
/// (`Enum.QualityLevel` Level21).
fn max_quality_level(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, 21)
}

fn zero(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, 0)
}

fn empty_string(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, "")
}

fn no(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, false)
}

/// The display's scale for high-DPI images; Rook draws GUI at 1:1.
fn one(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, 1.0)
}

/// Rook runs on desktops; scripts tell them apart only for platform
/// features Rook doesn't have.
fn platform(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let name = if cfg!(target_os = "macos") {
        "OSX"
    } else {
        "Windows"
    };
    Outcome::ret(lua, crate::types::enums::named_item(lua, "Platform", name)?)
}

/// Properties missing from Rook's API snapshot that CoreScripts read and
/// need a value of the right type from (the rest read as nil): the
/// version strings the settings menu shows, and the player's console
/// name.
pub(crate) fn hidden_property(class: &str, key: &str) -> Option<&'static str> {
    Some(match (class, key) {
        ("RunService", "ClientGitHash") => "rook",
        ("Player", "PlatformName") => "",
        _ => return None,
    })
}

/// `HttpService:RequestInternal(options)` gives a request whose
/// `Start(callback)` calls back with a failure: Roblox's web APIs are out
/// of reach locally, and the CoreScripts handle that as being offline.
pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("HttpService", "RequestInternal") => {
            r#"local task = (...).task
            return function(_, _options)
                local request = {}
                function request:Start(callback)
                    task.spawn(callback, false, { StatusCode = 0, Body = "", HttpError = "ConnectFail" })
                end
                function request:Cancel() end
                return request
            end"#
        }
        _ => return None,
    })
}

/// Start the CoreScript `name` (`"CoreScripts/PlayerList"`) under
/// `parent`, as a new CoreScript instance running its bytecode.
fn add_core_script_local(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let parent = a.instance(1)?;
    let script = mutate(lua, |w| {
        let found = w
            .core_scripts
            .and_then(|folder| w.dom.find_first_child(folder, &name, false))
            .and_then(|source| w.bytecode.get(&source).cloned());
        let Some(code) = found else {
            return Ok(None);
        };
        let id = w.dom.create_by_name("CoreScript")?;
        w.dom.set_name(id, name.clone())?;
        w.bytecode.insert(id, code);
        w.dom.set_parent(id, Some(parent))?;
        Ok(Some(id))
    })?;
    let Some(script) = script else {
        return Err(ScriptError(format!("CoreScript {name} not found")));
    };
    crate::api::scripts::run_script(lua, script);
    Outcome::ret(lua, ())
}

/// A flag's value as Studio last downloaded it (`FFlag` or `DFFlag` and
/// the name), else the default a script defined it with.
fn flag_value(lua: &Lua, prefixes: [&str; 2], name: &str) -> Option<String> {
    with_world(lua, |w| {
        prefixes.iter().find_map(|p| {
            let key = format!("{p}{name}");
            w.fast_flags
                .get(&key)
                .or_else(|| w.defined_flags.get(&key))
                .cloned()
        })
    })
}

/// `DefineFast*(name, default)` remembers the default for later
/// `GetFast*(name)` calls; both return the flag's value.
fn flag(lua: &Lua, prefixes: [&str; 2], a: &Args) -> ApiResult<Option<String>> {
    let name = a.string(0)?;
    if let Some(v) = flag_value(lua, prefixes, &name) {
        return Ok(Some(v));
    }
    let default = match a.get(1) {
        LuaValue::Nil => return Ok(None),
        LuaValue::Boolean(b) => b.to_string(),
        other => crate::ctx::args::display_value(lua, other),
    };
    with_world(lua, |w| {
        w.defined_flags
            .insert(format!("{}{name}", prefixes[0]), default.clone())
    });
    Ok(Some(default))
}

fn fast_flag(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let value = flag(lua, ["FFlag", "DFFlag"], a)?;
    Outcome::ret(lua, value.is_some_and(|v| v.eq_ignore_ascii_case("true")))
}

fn fast_int(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let value = flag(lua, ["FInt", "DFInt"], a)?.and_then(|v| v.parse::<f64>().ok());
    Outcome::ret(lua, value.unwrap_or(0.0))
}

fn fast_string(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    Outcome::ret(
        lua,
        flag(lua, ["FString", "DFString"], a)?.unwrap_or_default(),
    )
}

/// `settings():GetFFlag(name)`: the flag by its name without prefix.
fn global_flag(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let value =
        flag_value(lua, ["FFlag", "DFFlag"], &name).is_some_and(|v| v.eq_ignore_ascii_case("true"));
    Outcome::ret(lua, value)
}

/// `settings():GetFVariable(name)`: any flag's value as text, by its full
/// name or without its prefix; empty when there's none.
fn global_variable(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let value = with_world(lua, |w| w.fast_flags.get(&name).cloned()).or_else(|| {
        flag_value(lua, ["FFlag", "DFFlag"], &name)
            .or_else(|| flag_value(lua, ["FInt", "DFInt"], &name))
            .or_else(|| flag_value(lua, ["FString", "DFString"], &name))
    });
    Outcome::ret(
        lua,
        value
            .or_else(|| engine_flag(&name).map(str::to_owned))
            .unwrap_or_default(),
    )
}

/// Flags the engine defines itself, so Studio's file doesn't list them,
/// that CoreScripts read as numbers.
fn engine_flag(name: &str) -> Option<&'static str> {
    Some(match name {
        // Inference: the top bar's height, as `GuiService:GetGuiInset()`
        // measures it.
        "InGameGlobalGuiInset" => "58",
        _ => return None,
    })
}

/// Engine features name what the engine can do, so Rook says no: scripts
/// then keep to the older ways (Inference: CoreScripts guard newer API
/// behind them).
fn engine_feature(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, false)
}
