use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::{Change, Dom, InstanceId};
use rook_types::RunContext;

use super::instance::{LuaInstance, push_instance};
use crate::bytecode;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, api_fn, api_ret, with_world};
use crate::threads::spawn;
use crate::world::{Job, MessageType, WaitKind, World};

fn source_of(lua: &Lua, id: InstanceId) -> String {
    with_world(lua, |w| {
        w.dom.string_property(id, "Source").unwrap_or_default()
    })
}

/// A fresh environment whose globals fall back to the shared ones, with
/// `script` set. Global assignments stay in the script's own environment.
fn environment(lua: &Lua, script: InstanceId) -> mlua::Result<Table> {
    let env = fresh_environment(lua)?;
    env.raw_set("script", push_instance(lua, script)?)?;
    Ok(env)
}

/// Compile a script's source to run in `env` (once per source, see
/// [`bytecode`](crate::bytecode)), or load its bytecode if it came
/// compiled. Chunk names are full names, so errors read
/// `Workspace.Script:3: ...` as on Roblox.
fn compile(lua: &Lua, id: InstanceId, env: Table) -> mlua::Result<Function> {
    let (name, came_compiled) =
        with_world(lua, |w| (w.dom.full_name(id), w.bytecode.get(&id).cloned()));
    if came_compiled.as_ref().is_some_and(|c| c.core) {
        core_envs(lua)?.raw_set(env.clone(), true)?;
    }
    let source = came_compiled.is_none().then(|| source_of(lua, id));
    let code = match (&came_compiled, &source) {
        (Some(compiled), _) => Some(compiled.code.clone()),
        (None, Some(source)) => bytecode::of(source),
        (None, None) => None,
    };
    let chunk = match &code {
        Some(code) => lua.load(&code[..]).set_mode(mlua::chunk::ChunkMode::Binary),
        // It doesn't compile: the text reports why.
        None => lua
            .load(source.unwrap_or_default())
            .set_compiler(bytecode::compiler()),
    };
    chunk
        .set_name(format!("={name}"))
        .set_environment(env)
        .into_function()
}

/// The environments of scripts that came compiled: Roblox's own, which
/// run as [`Identity::Core`](super::instance::Identity) (weak keys).
fn core_envs(lua: &Lua) -> mlua::Result<Table> {
    lua.named_registry_value("rook.core_envs")
}

/// Whether the code running now is Roblox's own: the nearest frame of a
/// script's code (not Rook's own Luau, named `[rook] ...`, nor native
/// functions) belongs to a script that came compiled. Only asked when a
/// member needs more than the game identity, so stack walks stay off the
/// common paths.
pub(crate) fn caller_is_core(lua: &Lua) -> bool {
    let Ok(core) = core_envs(lua) else {
        return false;
    };
    for level in 0.. {
        let frame = lua.inspect_stack(level, |d| {
            let rook = d
                .source()
                .source
                .is_some_and(|s| s.trim_start_matches('=').starts_with("[rook]"));
            (!rook).then(|| d.function().environment()).flatten()
        });
        match frame {
            None => return false,
            Some(Some(env)) => return core.raw_get::<bool>(env).unwrap_or(false),
            Some(None) => {}
        }
    }
    false
}

/// Start a Script on a new thread. Compile errors go to the output.
/// Returns false if the script was already started or failed to compile.
pub(crate) fn run_script(lua: &Lua, id: InstanceId) -> bool {
    if with_world(lua, |w| w.started.contains_key(&id)) {
        return false;
    }
    let compiled = environment(lua, id).and_then(|env| Ok((compile(lua, id, env.clone())?, env)));
    let (func, env) = match compiled {
        Ok(f) => f,
        Err(e) => {
            with_world(lua, |w| {
                w.errors += 1;
                w.push_log(MessageType::Error, crate::ctx::lua_error_message(&e));
            });
            return false;
        }
    };
    match lua.create_thread(func) {
        Ok(thread) => {
            with_world(lua, |w| w.started.insert(id, env));
            crate::threads::resume(lua, &thread, ());
            true
        }
        Err(_) => false,
    }
}

/// Stop a running Script: its threads, delayed calls and connections never
/// run again, though code running right now carries on until it yields
/// (Confirmed, Studio recording 2026-10-17). Threads are matched to the
/// script by the environment its functions share, which also covers ones
/// the script made with `task.spawn` or `coroutine`. Returns false if the
/// script wasn't running.
pub(crate) fn stop_script(lua: &Lua, id: InstanceId) -> bool {
    let Some(env) = with_world(lua, |w| w.started.remove(&id)) else {
        return false;
    };
    let stopped = lua
        .named_registry_value::<Table>("rook.stopped_envs")
        .and_then(|t| t.raw_set(env, true));
    with_world(lua, |w| w.any_stopped = true);
    stopped.is_ok()
}

/// The running scripts `change` stops: ones it disabled, or moved (or
/// destroyed) out of anywhere they run. They stop at once, so handlers
/// already queued for them never run; a move between two places a script
/// runs keeps it running, and one that leaves and comes back in the same
/// frame starts afresh (Confirmed, Studio 2026-11-11).
pub(crate) fn stopped_by(w: &World, change: &Change) -> Vec<InstanceId> {
    if w.started.is_empty() {
        return Vec::new();
    }
    let running = |id: &&InstanceId| w.started.contains_key(*id);
    match change {
        Change::Reparented { subtree, .. } => subtree
            .iter()
            .filter(running)
            .filter(|id| !runs_here(&w.dom, **id, w.is_client))
            .copied()
            .collect(),
        Change::Property { id, property }
            if w.started.contains_key(id)
                && w.dom.database().property(*property).name == "Disabled"
                && w.dom.bool_property(*id, "Disabled") == Some(true) =>
        {
            vec![*id]
        }
        _ => Vec::new(),
    }
}

/// Whether a Script or LocalScript runs where it is now, enabled. It
/// starts when it gets somewhere it runs and stops the moment it leaves
/// ([`stopped_by`]).
///
/// Server: legacy-context Scripts under Workspace or
/// ServerScriptService (Confirmed: creator-docs Script); Server-context
/// Scripts anywhere except ServerStorage and the Starter containers
/// (Inference).
///
/// Client: LocalScripts under the player's Backpack, character,
/// PlayerGui or PlayerScripts, or under ReplicatedFirst (Confirmed:
/// creator-docs LocalScript); Client-context Scripts anywhere the client
/// sees except the Starter containers (Inference).
pub(crate) fn runs_here(dom: &Dom, id: InstanceId, is_client: bool) -> bool {
    let class = dom.class_name(id);
    if !matches!(class, "Script" | "LocalScript") {
        return false;
    }
    if dom.bool_property(id, "Disabled") == Some(true) {
        return false;
    }
    let Some(top) = top_level(dom, id) else {
        return false;
    };
    let top_class = dom.class_name(top);
    if is_client {
        return client_runs_here(dom, id, class, top_class);
    }
    if class != "Script" {
        return false;
    }
    match run_context(dom, id) {
        Some(RunContext::Server) => !(is_starter(top_class) || top_class == "ServerStorage"),
        Some(RunContext::Client | RunContext::Plugin) => false,
        // A player's Backpack too: classic gear's server Scripts hear
        // their tool's first `Equipped` there (Inference, from gear
        // like the LinkedSword working on the first equip).
        Some(RunContext::Legacy) | None => {
            matches!(top_class, "Workspace" | "ServerScriptService")
                || dom.ancestors(id).any(|a| {
                    dom.class_name(a) == "Backpack"
                        && dom.parent(a).is_some_and(|p| dom.is_a(p, "Player"))
                })
        }
    }
}

fn client_runs_here(dom: &Dom, id: InstanceId, class: &str, top_class: &str) -> bool {
    if class == "LocalScript" {
        if top_class == "ReplicatedFirst" {
            return true;
        }
        let local_player = dom.local_player();
        let character = local_player.and_then(|p| dom.ref_property(p, "Character"));
        return dom.ancestors(id).any(|a| {
            Some(a) == character
                || (matches!(
                    dom.class_name(a),
                    "Backpack" | "PlayerGui" | "PlayerScripts"
                ) && dom.parent(a) == local_player)
        });
    }
    run_context(dom, id) == Some(RunContext::Client)
        && !(is_starter(top_class) || matches!(top_class, "ServerStorage" | "ServerScriptService"))
}

/// Containers whose contents are templates copied to players, so scripts
/// in them never run in place.
fn is_starter(class: &str) -> bool {
    matches!(class, "StarterGui" | "StarterPack" | "StarterPlayer")
}

/// The child of the DataModel that contains `id`.
fn top_level(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    let root = dom.root();
    let mut cur = id;
    loop {
        let parent = dom.parent(cur)?;
        if parent == root {
            return Some(cur);
        }
        cur = parent;
    }
}

fn run_context(dom: &Dom, id: InstanceId) -> Option<RunContext> {
    RunContext::from_u32(dom.enum_property(id, "RunContext")?)
}

/// Whether `env` is the environment of a stopped script.
pub(crate) fn is_stopped_env(lua: &Lua, env: Table) -> bool {
    lua.named_registry_value::<Table>("rook.stopped_envs")
        .and_then(|t| t.raw_get::<bool>(env))
        .unwrap_or(false)
}

/// Run a chunk of source as if it were a Script at `id` (used by tests and
/// the conformance runner).
pub(crate) fn run_source(
    lua: &Lua,
    chunk_name: &str,
    source: &str,
    script: Option<InstanceId>,
) -> mlua::Result<()> {
    let env = match script {
        Some(id) => environment(lua, id)?,
        None => fresh_environment(lua)?,
    };
    run_in(lua, chunk_name, source, env)
}

/// An environment that reads the shared globals and keeps its own writes.
/// It's marked `safeenv`, as Roblox's script sandbox is, so Luau calls
/// builtins (`math.floor`, `type`, `select`) and runs `pairs` loops on its
/// fast paths. `getfenv` and `setfenv` clear the mark (Luau's own rule),
/// so scripts that change environments get the general paths.
fn fresh_environment(lua: &Lua) -> mlua::Result<Table> {
    let env = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.raw_set("__index", lua.globals())?;
    env.set_metatable(Some(mt))?;
    env.set_safeenv(true);
    Ok(env)
}

/// Run `source` in the environment kept under `name` across calls, so
/// globals one call sets the next one sees.
pub(crate) fn run_source_in(
    lua: &Lua,
    name: &str,
    chunk_name: &str,
    source: &str,
) -> mlua::Result<()> {
    let key = format!("rook.env.{name}");
    let env = match lua.named_registry_value::<Option<Table>>(&key)? {
        Some(env) => env,
        None => {
            let env = fresh_environment(lua)?;
            lua.set_named_registry_value(&key, &env)?;
            env
        }
    };
    run_in(lua, chunk_name, source, env)
}

fn run_in(lua: &Lua, chunk_name: &str, source: &str, env: Table) -> mlua::Result<()> {
    let f = lua
        .load(source)
        .set_name(format!("={chunk_name}"))
        .set_compiler(bytecode::compiler())
        .set_environment(env)
        .into_function()?;
    spawn(lua, f, ())?;
    Ok(())
}

// require

fn require(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "require");
    let module = match a.get(0) {
        LuaValue::UserData(u) => u.borrow::<LuaInstance>().ok().map(|i| i.0),
        LuaValue::Integer(n) => return super::methods::assets::require_asset(lua, *n as f64),
        LuaValue::Number(n) => return super::methods::assets::require_asset(lua, *n),
        _ => None,
    }
    .ok_or_else(|| ScriptError("Attempted to call require with invalid argument(s).".into()))?;
    if !with_world(lua, |w| w.dom.is_a(module, "ModuleScript")) {
        return Err(ScriptError(
            "Attempted to call require with invalid argument(s).".into(),
        ));
    }

    let cached = with_world(lua, |w| {
        w.modules
            .results
            .get(&module)
            .map(|k| lua.registry_value::<LuaValue>(k))
    });
    if let Some(v) = cached {
        return Outcome::ret(lua, v?);
    }
    // Another thread loading the module: wait for it (observed in Studio
    // 0.742: a second require while the body yielded got the same value).
    let thread = lua.current_thread();
    let loader = with_world(lua, |w| w.modules.loading.get(&module).cloned());
    if let Some(loader) = loader {
        if loader == thread {
            return Err(ScriptError(
                "Requested module was required recursively".into(),
            ));
        }
        with_world(lua, |w| {
            w.modules.waiting.entry(module).or_default().push(thread)
        });
        let wait: Function = lua.named_registry_value("rook.module_wait")?;
        return Ok(Outcome::Call(wait, MultiValue::new()));
    }
    let func = compile(lua, module, environment(lua, module)?).map_err(|e| {
        with_world(lua, |w| {
            w.errors += 1;
            w.push_log(MessageType::Error, crate::ctx::lua_error_message(&e));
        });
        ScriptError("Requested module experienced an error while loading".into())
    })?;
    with_world(lua, |w| w.modules.loading.insert(module, thread));
    let runner: Function = lua.named_registry_value("rook.module_runner")?;
    Ok(Outcome::Call(
        runner,
        MultiValue::from_vec(vec![
            LuaValue::UserData(push_instance(lua, module)?),
            LuaValue::Function(func),
        ]),
    ))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    // Stopped scripts' environments, and the function each thread `task.*`
    // made calls (see `threads::thread_for`). Weak, so entries go with
    // what they describe.
    for key in [
        "rook.stopped_envs",
        "rook.thread_functions",
        "rook.core_envs",
    ] {
        let t = lua.create_table()?;
        let mt = lua.create_table()?;
        mt.raw_set("__mode", "k")?;
        t.set_metatable(Some(mt))?;
        lua.set_named_registry_value(key, t)?;
    }
    // Runs the module body from Lua, so modules may yield while loading.
    let finished = api_ret(lua, |lua, args| {
        let a = Args::new(args, "require");
        let module = a.instance(0)?;
        let ok = a.truthy(1);
        let value = a.get(2).clone();
        let waiting = with_world(lua, |w| {
            w.modules.loading.remove(&module);
            w.modules.waiting.remove(&module).unwrap_or_default()
        });
        if ok {
            let key = lua.create_registry_value(value.clone())?;
            with_world(lua, |w| w.modules.results.insert(module, key));
        } else {
            let msg = crate::ctx::args::display_value(lua, &value);
            with_world(lua, |w| {
                w.errors += 1;
                w.push_log(MessageType::Error, msg);
            });
        }
        // Waiters resume after this thread's require returns.
        for thread in waiting {
            let result = if ok { value.clone() } else { LuaValue::Nil };
            let job = Job::Resume {
                thread,
                args: vec![LuaValue::Boolean(ok), result],
                wait: WaitKind::None,
            };
            with_world(lua, |w| w.scheduler.defer(job));
        }
        Ok(())
    })?;
    // Waits for a module another thread is loading: its value, or the
    // error the loading require raised.
    let wait_native = api_fn(lua, |_, _| Ok(Outcome::Yield))?;
    let wait: Function = lua
        .load(
            r##"
            local wait, error = ...
            return function()
                local ok, value = wait()
                if not ok then
                    error("Requested module experienced an error while loading", 0)
                end
                return value
            end
            "##,
        )
        .set_name("=[rook] require")
        .call((wait_native, lua.globals().get::<Function>("error")?))?;
    lua.set_named_registry_value("rook.module_wait", wait)?;
    let runner: Function = lua
        .load(
            r##"
            local finished, pcall, select, error = ...
            local function check(module, ok, ...)
                if not ok then
                    finished(module, false, (...))
                    error("Requested module experienced an error while loading", 0)
                end
                if select("#", ...) ~= 1 then
                    finished(module, false, "Module code did not return exactly one value")
                    error("Module code did not return exactly one value", 0)
                end
                local value = ...
                finished(module, true, value)
                return value
            end
            return function(module, body)
                return check(module, pcall(body))
            end
            "##,
        )
        .set_name("=[rook] require")
        .call((
            finished,
            lua.globals().get::<Function>("pcall")?,
            lua.globals().get::<Function>("select")?,
            lua.globals().get::<Function>("error")?,
        ))?;
    lua.set_named_registry_value("rook.module_runner", runner)?;
    lua.globals().set("require", api_fn(lua, require)?)?;
    Ok(())
}
