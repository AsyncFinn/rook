//! RBXScriptSignal and RBXScriptConnection.

use mlua::{Function, Lua, MultiValue, Table, UserData, UserDataFields, Value as LuaValue};
use rook_dom::InstanceId;
use rook_signal::{ConnectionId, SignalKey, SignalKind};

use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};
use crate::world::{Handler, Job};

#[derive(Debug, Clone)]
pub(crate) struct LSignal {
    pub(crate) instance: InstanceId,
    pub(crate) kind: SignalKind,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LConnection(pub(crate) ConnectionId);

impl LSignal {
    fn key(&self) -> SignalKey {
        SignalKey {
            instance: self.instance,
            kind: self.kind.clone(),
        }
    }

    fn name(&self) -> String {
        match &self.kind {
            SignalKind::Event(n) => (*n).to_owned(),
            SignalKind::Property(_) => "PropertyChangedSignal".to_owned(),
            SignalKind::Attribute(_) => "AttributeChangedSignal".to_owned(),
        }
    }
}

impl UserData for LSignal {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "RBXScriptSignal");
        f.add_meta_field_with("__index", |lua| {
            lua.named_registry_value::<Function>("rook.signal.index")
        });
        f.add_meta_field_with("__tostring", |lua| {
            lua.create_function(|_, s: mlua::UserDataRef<LSignal>| {
                Ok(format!("Signal {}", s.name()))
            })
        });
    }
}

impl UserData for LConnection {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "RBXScriptConnection");
        f.add_meta_field_with("__index", |lua| {
            lua.named_registry_value::<Function>("rook.connection.index")
        });
        f.add_meta_field_with("__tostring", |lua| {
            lua.create_function(|_, ()| Ok("Connection"))
        });
    }
}

fn this_signal(args: &mut Vec<LuaValue>, name: &str) -> ApiResult<LSignal> {
    let sig = match args.first() {
        Some(LuaValue::UserData(u)) => u.borrow::<LSignal>().ok().map(|s| s.clone()),
        _ => None,
    };
    let sig = sig.ok_or_else(|| {
        ScriptError(format!(
            "Expected ':' not '.' calling member function {name}"
        ))
    })?;
    args.remove(0);
    Ok(sig)
}

fn connect(lua: &Lua, args: MultiValue, name: &'static str, once: bool) -> ApiResult<Outcome> {
    let mut v = args.into_vec();
    let sig = this_signal(&mut v, name)?;
    let args = Args::new(MultiValue::from_vec(v), name);
    let func = args.function(0).map_err(|_| {
        ScriptError("Attempt to connect failed: Passed value is not a function".into())
    })?;
    let id = with_world(lua, |w| {
        w.signals.connect(sig.key(), Handler::Call(func), once)
    });
    Outcome::ret(lua, LConnection(id))
}

fn wait(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let mut v = args.into_vec();
    let sig = this_signal(&mut v, "Wait")?;
    let thread = lua.current_thread();
    with_world(lua, |w| {
        w.signals.connect(sig.key(), Handler::Resume(thread), true)
    });
    Ok(Outcome::Yield)
}

fn disconnect(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let conn = match args.into_vec().first() {
        Some(LuaValue::UserData(u)) => u.borrow::<LConnection>().ok().map(|c| *c),
        _ => None,
    }
    .ok_or_else(|| ScriptError("Expected ':' not '.' calling member function Disconnect".into()))?;
    with_world(lua, |w| {
        w.signals.disconnect(conn.0);
        // Deferred mode: disconnecting drops pending invocations
        // (Confirmed: creator-docs "Deferred events").
        w.scheduler
            .cancel(|j| matches!(j, Job::Fire { connection, .. } if *connection == conn.0));
    });
    Outcome::ret(lua, ())
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let methods = lua.create_table()?;
    for (name, once) in [("Connect", false), ("connect", false), ("Once", true)] {
        methods.set(
            name,
            api_fn(lua, move |lua, a| connect(lua, a, name, once))?,
        )?;
    }
    // Rook runs everything on one thread, so ConnectParallel behaves like
    // Connect until Actors exist.
    methods.set(
        "ConnectParallel",
        api_fn(lua, |lua, a| connect(lua, a, "ConnectParallel", false))?,
    )?;
    let wait_fn = api_fn(lua, wait)?;
    methods.set("Wait", wait_fn.clone())?;
    methods.set("wait", wait_fn)?;
    let index = index_fn(lua, methods, "RBXScriptSignal")?;
    lua.set_named_registry_value("rook.signal.index", index)?;

    let conn_methods = lua.create_table()?;
    let disconnect_fn = api_fn(lua, disconnect)?;
    conn_methods.set("Disconnect", disconnect_fn.clone())?;
    conn_methods.set("disconnect", disconnect_fn)?;
    let conn_index = api_fn(lua, move |lua, args| {
        let v = args.into_vec();
        let key = match v.get(1) {
            Some(LuaValue::String(s)) => s.to_string_lossy(),
            _ => String::new(),
        };
        if key == "Connected" || key == "connected" {
            let conn = match v.first() {
                Some(LuaValue::UserData(u)) => u.borrow::<LConnection>().ok().map(|c| *c),
                _ => None,
            };
            let connected = conn.is_some_and(|c| with_world(lua, |w| w.signals.is_connected(c.0)));
            return Outcome::ret(lua, connected);
        }
        match conn_methods.raw_get::<Option<Function>>(key.as_str())? {
            Some(f) => Outcome::ret(lua, f),
            None => Err(ScriptError(format!(
                "{key} is not a valid member of RBXScriptConnection"
            ))),
        }
    })?;
    lua.set_named_registry_value("rook.connection.index", conn_index)?;
    Ok(())
}

fn index_fn(lua: &Lua, methods: Table, type_name: &'static str) -> mlua::Result<Function> {
    api_fn(lua, move |lua, args| {
        let v = args.into_vec();
        let key = match v.get(1) {
            Some(LuaValue::String(s)) => s.to_string_lossy(),
            _ => String::new(),
        };
        match methods.raw_get::<Option<Function>>(key.as_str())? {
            Some(f) => Outcome::ret(lua, f),
            None => Err(ScriptError(format!(
                "{key} is not a valid member of {type_name}"
            ))),
        }
    })
}
