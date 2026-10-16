//! RemoteEvent, UnreliableRemoteEvent and RemoteFunction.
//!
//! Calls are queued in the World's outbox; the session delivers them to
//! the other DataModel (creator-docs "Remote events and callbacks").
//! Error texts are Inference.

use mlua::{Lua, MultiValue, Value as LuaValue};
use rook_dom::InstanceId;

use super::methods::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};
use crate::net::{RemoteKind, RemoteMessage, to_net};

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("RemoteEvent" | "UnreliableRemoteEvent", "FireServer") => fire_server,
        ("RemoteEvent" | "UnreliableRemoteEvent", "FireClient") => fire_client,
        ("RemoteEvent" | "UnreliableRemoteEvent", "FireAllClients") => fire_all_clients,
        _ => return super::input::native(owner, name),
    })
}

pub(crate) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("RemoteFunction", "InvokeServer") => {
            r#"local std = ...
            local invoke, unpack = std.invoke, std.table.unpack
            return function(self, ...)
                local ok, results = invoke(self, "server", nil, ...)
                if not ok then error(results, 0) end
                return unpack(results, 1, results.n)
            end"#
        }
        ("RemoteFunction", "InvokeClient") => {
            r#"local std = ...
            local invoke, unpack = std.invoke, std.table.unpack
            return function(self, player, ...)
                local ok, results = invoke(self, "client", player, ...)
                if not ok then error(results, 0) end
                return unpack(results, 1, results.n)
            end"#
        }
        _ => return None,
    })
}

fn is_client(lua: &Lua) -> bool {
    with_world(lua, |w| w.is_client)
}

fn push(lua: &Lua, remote: InstanceId, kind: RemoteKind, args: &[LuaValue]) {
    let args = args.iter().map(|v| to_net(v, 0)).collect();
    with_world(lua, |w| {
        w.outbox.push(RemoteMessage {
            remote: Some(remote),
            kind,
            args,
        });
    });
}

fn player_arg(lua: &Lua, a: &Args, method: &str) -> ApiResult<InstanceId> {
    let player = a.opt_instance(0).ok().flatten();
    match player {
        Some(p) if with_world(lua, |w| w.dom.is_a(p, "Player")) => Ok(p),
        _ => Err(ScriptError(format!(
            "{method}: player argument must be a Player object"
        ))),
    }
}

fn fire_server(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    if !is_client(lua) {
        return Err("FireServer can only be called from the client".into());
    }
    push(lua, this, RemoteKind::FireServer, &a.into_vec());
    Outcome::ret(lua, ())
}

fn fire_client(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    if is_client(lua) {
        return Err("FireClient can only be called from the server".into());
    }
    let player = player_arg(lua, &a, "FireClient")?;
    push(lua, this, RemoteKind::FireClient { player }, &a.rest(1));
    Outcome::ret(lua, ())
}

fn fire_all_clients(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    if is_client(lua) {
        return Err("FireAllClients can only be called from the server".into());
    }
    push(lua, this, RemoteKind::FireAllClients, &a.into_vec());
    Outcome::ret(lua, ())
}

/// `std.invoke(remote, "server" | "client", player, ...)`: queue an invoke
/// and yield until the reply resumes this thread with `(ok, results)`.
fn invoke(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "Invoke");
    let remote = a.instance(0)?;
    let target = a.string(1)?;
    let client = is_client(lua);
    let kind_for = |call: u64| -> ApiResult<RemoteKind> {
        match (target.as_str(), client) {
            ("server", true) => Ok(RemoteKind::InvokeServer { call }),
            ("server", false) => Err("InvokeServer can only be called from the client".into()),
            ("client", false) => {
                let player = a
                    .opt_instance(2)
                    .ok()
                    .flatten()
                    .filter(|p| with_world(lua, |w| w.dom.is_a(*p, "Player")))
                    .ok_or_else(|| {
                        ScriptError("InvokeClient: player argument must be a Player object".into())
                    })?;
                Ok(RemoteKind::InvokeClient { player, call })
            }
            _ => Err("InvokeClient can only be called from the server".into()),
        }
    };
    let call = with_world(lua, |w| {
        w.next_call += 1;
        w.next_call
    });
    let kind = kind_for(call)?;
    let thread = lua.current_thread();
    with_world(lua, |w| {
        w.pending_calls.insert(call, thread);
    });
    push(lua, remote, kind, &a.rest(3));
    Ok(Outcome::Yield)
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let std: mlua::Table = lua.named_registry_value("rook.std")?;
    std.set("invoke", api_fn(lua, invoke)?)?;
    std.set(
        "reply",
        lua.create_function(
            |lua, (call, ok, rest): (u64, bool, mlua::Variadic<LuaValue>)| {
                let args = rest.iter().map(|v| to_net(v, 0)).collect();
                with_world(lua, |w| {
                    w.outbox.push(RemoteMessage {
                        remote: None,
                        kind: RemoteKind::Reply { call, ok },
                        args,
                    });
                });
                Ok(())
            },
        )?,
    )?;
    // `std.wait_for_callback(object, name)`: the callback, once one is set.
    // Invokes queue until then (creator-docs BindableFunction.OnInvoke,
    // RemoteFunction.OnServerInvoke); Rook checks once per frame.
    let wait_for_callback: mlua::Function = lua
        .load(
            r#"
            local std = ...
            local get_callback, wait = std.get_callback, std.task.wait
            return function(object, name)
                local callback = get_callback(object, name)
                while not callback do
                    wait()
                    callback = get_callback(object, name)
                end
                return callback
            end
            "#,
        )
        .set_name("=[rook] wait_for_callback")
        .call(std.clone())?;
    std.set("wait_for_callback", wait_for_callback)?;
    // Runs an incoming invoke on the receiving side: waits for the callback,
    // calls it, and sends back the results or the error.
    let runner: mlua::Function = lua
        .load(
            r#"
            local std = ...
            local wait_for_callback, reply, pcall, pack, unpack =
                std.wait_for_callback, std.reply, std.pcall, std.table.pack, std.table.unpack
            return function(remote, name, call, ...)
                local callback = wait_for_callback(remote, name)
                local results = pack(pcall(callback, ...))
                reply(call, unpack(results, 1, results.n))
            end
            "#,
        )
        .set_name("=[rook] Remote")
        .call(std)?;
    lua.set_named_registry_value("rook.remote_runner", runner)?;
    Ok(())
}
