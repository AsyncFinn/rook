//! Published models: `require(assetId)` and `InsertService:LoadAsset`.
//!
//! Scripts queue asset ids in the World ([`AssetRequests`]); the engine
//! fetches each through its content loader, reads the file and hands back
//! a function that fills a Model with the file's instances
//! (`Runtime::deliver_asset`), so this crate needs no network or file
//! formats of its own. The calling thread waits meanwhile. What Studio does
//! is in `docs/behavior/content.md` (Published models).

use std::collections::HashMap;

use mlua::{Function, Lua, MultiValue, Thread, Value as LuaValue};
use rook_content::FailureKind;
use rook_dom::{Dom, InstanceId};

use super::instance::push_instance;
use super::methods::RawMethod;
use crate::ctx::args::{Args, number_of, type_name};
use crate::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};

/// Copies a fetched asset's instances into a Model.
pub(crate) type Fill = Box<dyn FnOnce(&mut Dom, InstanceId)>;

/// A [`Fill`] for the asset, or why it didn't load.
pub type AssetFill = Result<Fill, FailureKind>;

/// Asset loads scripts are waiting on.
#[derive(Debug, Default)]
pub struct AssetRequests {
    /// Asset ids to fetch, by request id, since the engine last took them.
    pub(crate) outbox: Vec<(u64, i64)>,
    waiting: HashMap<u64, Waiter>,
    /// What `require` found for each asset id: its MainModule, or the
    /// error every later `require` of it raises.
    required: HashMap<i64, Result<InstanceId, String>>,
    /// Threads waiting to require each asset id still downloading (one
    /// download serves them all).
    requiring: HashMap<i64, Vec<Thread>>,
    next: u64,
}

#[derive(Debug)]
enum Waiter {
    LoadAsset(Thread),
    Require(i64),
}

impl AssetRequests {
    fn queue(&mut self, asset: i64, waiter: Waiter) {
        self.next += 1;
        self.outbox.push((self.next, asset));
        self.waiting.insert(self.next, waiter);
    }
}

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("InsertService", "LoadAsset" | "loadAsset") => load_asset,
        _ => return None,
    })
}

/// An asset id from a number, rounded to the nearest as Studio does.
fn asset_id(n: f64) -> i64 {
    n.round() as i64
}

fn load_asset(lua: &Lua, _this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let id = match a.get(0) {
        LuaValue::Nil => return Err(ScriptError("Argument 1 missing or nil".into())),
        v => number_of(v)
            .map(asset_id)
            .ok_or_else(|| ScriptError(format!("Unable to cast {} to int64", type_name(v))))?,
    };
    if with_world(lua, |w| w.is_client) {
        return Err(ScriptError(
            "InsertService cannot be used to load assets from the client".into(),
        ));
    }
    let helper: Function = lua.named_registry_value("rook.load_asset")?;
    Ok(Outcome::Call(
        helper,
        MultiValue::from_vec(vec![LuaValue::Integer(id)]),
    ))
}

/// `require` with a number: the asset's MainModule, loaded once per id.
pub(crate) fn require_asset(lua: &Lua, n: f64) -> ApiResult<Outcome> {
    let id = asset_id(n);
    if with_world(lua, |w| w.is_client) {
        return Err(ScriptError(format!(
            "require(assetId) cannot be called from a client.  assetId = {id}"
        )));
    }
    let helper: Function = lua.named_registry_value("rook.require_asset")?;
    Ok(Outcome::Call(
        helper,
        MultiValue::from_vec(vec![LuaValue::Integer(id)]),
    ))
}

/// Wait for an asset: `(id, true)` for require's MainModule (answered at
/// once if the id loaded before), `(id, false)` for a LoadAsset Model.
/// Resumes with the instance, or nil and the error.
fn wait_asset(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "require");
    let id = asset_id(a.number(0)?);
    let for_require = a.truthy(1);
    let thread = lua.current_thread();
    let known = with_world(lua, |w| {
        let r = &mut w.assets;
        if !for_require {
            r.queue(id, Waiter::LoadAsset(thread));
            return None;
        }
        if let Some(found) = r.required.get(&id) {
            return Some(found.clone());
        }
        let first = !r.requiring.contains_key(&id);
        r.requiring.entry(id).or_default().push(thread);
        if first {
            r.queue(id, Waiter::Require(id));
        }
        None
    });
    match known {
        Some(Ok(module)) => Outcome::ret(lua, push_instance(lua, module)?),
        Some(Err(msg)) => Outcome::ret(lua, (LuaValue::Nil, msg)),
        None => Ok(Outcome::Yield),
    }
}

fn download_failed(id: i64) -> String {
    format!(
        "Downloading asset failed for asset id {id}.  Is the asset id correct and is the asset type \"Model\"?"
    )
}

/// A Model with no parent holding the asset's instances.
fn fill_model(lua: &Lua, name: String, fill: Fill) -> InstanceId {
    with_world(lua, |w| {
        let model = w.dom.create_by_name("Model").expect("Model is a class");
        // A new instance has no parent, so renaming it can't fail.
        let _ = w.dom.set_name(model, name);
        fill(&mut w.dom, model);
        model
    })
}

/// Deliver a finished asset: resume the LoadAsset or the requires waiting
/// for it.
pub(crate) fn deliver(lua: &Lua, request: u64, result: AssetFill) {
    let Some(waiter) = with_world(lua, |w| w.assets.waiting.remove(&request)) else {
        return;
    };
    match waiter {
        Waiter::LoadAsset(thread) => match result {
            Ok(fill) => {
                let model = fill_model(lua, "Model".into(), fill);
                if let Ok(m) = push_instance(lua, model) {
                    crate::threads::resume(lua, &thread, m);
                }
            }
            Err(kind) => {
                let msg = match kind {
                    FailureKind::Forbidden => "User is not authorized to access Asset.",
                    _ => "Request asset was not found",
                };
                crate::threads::resume(lua, &thread, (LuaValue::Nil, msg));
            }
        },
        Waiter::Require(id) => {
            let found = match result {
                Ok(fill) => {
                    let model = fill_model(lua, format!("required_asset_{id}"), fill);
                    with_world(lua, |w| {
                        w.dom
                            .children(model)
                            .iter()
                            .copied()
                            .find(|&c| {
                                w.dom.name(c) == "MainModule"
                                    && w.dom.is_a(c, "ModuleScript")
                            })
                    })
                    .ok_or_else(|| {
                        format!(
                            "Unable to find module for asset id {id}.  Does the asset have a ModuleScript named \"MainModule\"?"
                        )
                    })
                }
                Err(_) => Err(download_failed(id)),
            };
            let threads = with_world(lua, |w| {
                w.assets.required.insert(id, found.clone());
                w.assets.requiring.remove(&id).unwrap_or_default()
            });
            for thread in threads {
                match &found {
                    Ok(module) => {
                        if let Ok(m) = push_instance(lua, *module) {
                            crate::threads::resume(lua, &thread, m);
                        }
                    }
                    Err(msg) => crate::threads::resume(lua, &thread, (LuaValue::Nil, msg.clone())),
                }
            }
        }
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let wait = api_fn(lua, wait_asset)?;
    let helpers: (Function, Function) = lua
        .load(
            r##"
            local wait, require, error = ...
            local function load_asset(id)
                local model, err = wait(id, false)
                if not model then
                    error(err, 0)
                end
                return model
            end
            local function require_asset(id)
                local module, err = wait(id, true)
                if not module then
                    error(err, 0)
                end
                return require(module)
            end
            return load_asset, require_asset
            "##,
        )
        .set_name("=[rook] assets")
        .call((
            wait,
            lua.globals().get::<Function>("require")?,
            lua.globals().get::<Function>("error")?,
        ))?;
    lua.set_named_registry_value("rook.load_asset", helpers.0)?;
    lua.set_named_registry_value("rook.require_asset", helpers.1)?;
    Ok(())
}
